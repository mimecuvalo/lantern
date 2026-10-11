//! macOS integration: shaped windows, context menus, media keys and Now
//! Playing.
//!
//! Media keys go via `MPRemoteCommandCenter` and `MPNowPlayingInfoCenter`
//! through objc2 — deliberately not a `CGEventTap`, which would demand
//! Accessibility permission and be capable of keylogging.

// The one module allowed to call into Objective-C. Each `unsafe` is a message
// send to an object whose type AppKit documents; see the SAFETY notes.
#![allow(unsafe_code)]

use lantern_skin::Polygon;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_graphics::CGMutablePath;
use objc2_quartz_core::{CALayer, CAShapeLayer};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// Put the window's backing store in sRGB, the space skin colours are
/// authored in. Pairs with [`tag_frame_srgb`].
///
/// softbuffer tags every frame "device RGB". Left alone, Core Animation
/// converted each one to the display's space on the CPU — most of the main
/// thread's work while playing (`vImageConvert_AnyToAny`). M6 made the
/// window device RGB too, which skipped that but sent skin colours to the
/// display raw: oversaturated on a wide-gamut screen, as a side-by-side
/// with Winamp showed. With the window *and* the frames in sRGB, nothing is
/// converted in-process, and the window server matches the window to the
/// display on the GPU, as it does for any colour-managed app.
pub fn use_srgb(window: &Window) {
    let Some(view) = ns_view(window) else { return };
    let Some(class) = objc2::runtime::AnyClass::get(c"NSColorSpace") else {
        return;
    };
    // SAFETY: documented AppKit messages with the argument types given, on
    // a view winit keeps alive for the call.
    unsafe {
        let ns_window: Option<Retained<AnyObject>> = msg_send![view, window];
        let Some(ns_window) = ns_window else { return };
        let space: Retained<AnyObject> = msg_send![class, sRGBColorSpace];
        let _: () = msg_send![&*ns_window, setColorSpace: &*space];
    }
}

/// Re-tag the frame softbuffer just presented as sRGB. Call after each
/// present.
///
/// The copy shares the frame's pixels — `CGImageCreateCopyWithColorSpace`
/// only swaps the label — so this costs a few allocations, not a pass over
/// the image. softbuffer draws into the one sublayer it adds to the view's
/// layer; anything unexpected there is left alone.
pub fn tag_frame_srgb(window: &Window) {
    use objc2_core_graphics::{CGColorSpace, CGImage, kCGColorSpaceSRGB};
    use objc2_quartz_core::CATransaction;
    use std::sync::OnceLock;

    struct Space(objc2_core_foundation::CFRetained<CGColorSpace>);
    // SAFETY: CGColorSpace is immutable and documented thread-safe.
    unsafe impl Send for Space {}
    unsafe impl Sync for Space {}
    static SRGB: OnceLock<Option<Space>> = OnceLock::new();
    let Some(Space(srgb)) =
        SRGB.get_or_init(|| CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).map(Space))
    else {
        return;
    };

    let Some(view) = ns_view(window) else { return };
    // SAFETY: `-[NSView layer]` returns a CALayer or nil; `sublayers` an
    // array of CALayer or nil; a layer's `contents` here is the CGImage
    // softbuffer set, checked by its CF type before use.
    unsafe {
        let Some(root): Option<Retained<CALayer>> = msg_send![view, layer] else {
            return;
        };
        let Some(layer) = root.sublayers().and_then(|s| s.firstObject()) else {
            return;
        };
        let Some(contents) = layer.contents() else {
            return;
        };
        let cf: &objc2_core_foundation::CFType = &*(Retained::as_ptr(&contents).cast());
        let Some(image) = cf.downcast_ref::<CGImage>() else {
            return;
        };
        if CGImage::color_space(Some(image)).as_deref() == Some(&**srgb) {
            return;
        }
        let Some(tagged) = CGImage::new_copy_with_color_space(Some(image), Some(srgb)) else {
            return;
        };
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        let object: &AnyObject = &*(objc2_core_foundation::CFRetained::as_ptr(&tagged)
            .as_ptr()
            .cast::<AnyObject>());
        layer.setContents(Some(object));
        CATransaction::commit();
    }
}

/// The window's content view.
fn ns_view(window: &Window) -> Option<&AnyObject> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };
    // SAFETY: winit's `ns_view` is a live `NSView *` for as long as the
    // window exists, which outlives the returned borrow of `window`.
    Some(unsafe { appkit.ns_view.cast().as_ref() })
}

/// Show `menu` as a context menu at the pointer, over `window`. Blocks until
/// it closes; the choice, if any, arrives on `muda::MenuEvent::receiver()`.
pub fn popup_menu(window: &Window, menu: &muda::Menu) {
    use muda::ContextMenu;
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    // SAFETY: winit's `ns_view` is a live `NSView *` for the window's life,
    // which spans this call.
    unsafe {
        menu.show_context_menu_for_nsview(appkit.ns_view.as_ptr().cast_const(), None);
    }
}

/// A request from the media keys, the Control Centre, headphones or a
/// Bluetooth remote.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MediaCommand {
    Play,
    Pause,
    TogglePlayPause,
    Stop,
    Next,
    Previous,
    /// Scrubbed to this many seconds into the track.
    Seek(f64),
}

/// What Now Playing shows.
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: Option<String>,
    pub duration_secs: Option<f64>,
    pub elapsed_secs: f64,
    pub playing: bool,
}

/// The system's remote-control hooks. Handlers run on the main thread;
/// they queue a command and call `wake` so an idle event loop notices.
pub struct MediaRemote {
    commands: std::sync::mpsc::Receiver<MediaCommand>,
    /// Handler registrations, kept alive for as long as this is.
    _targets: Vec<Retained<AnyObject>>,
}

impl MediaRemote {
    pub fn install(wake: impl Fn() + Send + Sync + 'static) -> Self {
        use objc2_media_player::{
            MPChangePlaybackPositionCommandEvent, MPRemoteCommand, MPRemoteCommandCenter,
            MPRemoteCommandEvent, MPRemoteCommandHandlerStatus,
        };
        use std::ptr::NonNull;
        use std::sync::Arc;

        let (tx, commands) = std::sync::mpsc::channel();
        let wake = Arc::new(wake);
        let mut targets = Vec::new();
        // SAFETY: the shared command centre is a process-wide singleton.
        let center = unsafe { MPRemoteCommandCenter::sharedCommandCenter() };

        let mut register =
            |command: Retained<MPRemoteCommand>,
             make: fn(&MPRemoteCommandEvent) -> Option<MediaCommand>| {
                let (tx, wake) = (tx.clone(), Arc::clone(&wake));
                let handler = block2::RcBlock::new(
                    move |event: NonNull<MPRemoteCommandEvent>| -> MPRemoteCommandHandlerStatus {
                        // SAFETY: the system passes a valid event for the call.
                        let event = unsafe { event.as_ref() };
                        match make(event) {
                            Some(c) if tx.send(c).is_ok() => {
                                wake();
                                MPRemoteCommandHandlerStatus::Success
                            }
                            _ => MPRemoteCommandHandlerStatus::CommandFailed,
                        }
                    },
                );
                // SAFETY: the block is retained by the command for as long as
                // the target is registered; `addTargetWithHandler:` returns the
                // token that keeps it so.
                let target = unsafe { command.addTargetWithHandler(&handler) };
                // SAFETY: plain property setter.
                unsafe { command.setEnabled(true) };
                targets.push(target);
            };

        // SAFETY: each accessor returns the centre's own command object.
        let (play, pause, toggle, stop, next, previous, position) = unsafe {
            (
                center.playCommand(),
                center.pauseCommand(),
                center.togglePlayPauseCommand(),
                center.stopCommand(),
                center.nextTrackCommand(),
                center.previousTrackCommand(),
                Retained::into_super(center.changePlaybackPositionCommand()),
            )
        };
        register(play, |_| Some(MediaCommand::Play));
        register(pause, |_| Some(MediaCommand::Pause));
        register(toggle, |_| Some(MediaCommand::TogglePlayPause));
        register(stop, |_| Some(MediaCommand::Stop));
        register(next, |_| Some(MediaCommand::Next));
        register(previous, |_| Some(MediaCommand::Previous));
        register(position, |event| {
            let scrub = event.downcast_ref::<MPChangePlaybackPositionCommandEvent>()?;
            // SAFETY: plain property getter.
            Some(MediaCommand::Seek(unsafe { scrub.positionTime() }))
        });
        Self {
            commands,
            _targets: targets,
        }
    }

    /// Commands received since the last call. Never blocks.
    pub fn poll(&self) -> Vec<MediaCommand> {
        self.commands.try_iter().collect()
    }

    /// Tell the system what is playing; `None` clears it. The position is
    /// published with a rate, and the system extrapolates from there, so
    /// this is needed only when something changes — not every frame.
    pub fn publish(&self, now: Option<&NowPlaying>) {
        use objc2_foundation::{NSDictionary, NSNumber, NSString};
        use objc2_media_player::{
            MPMediaItemPropertyArtist, MPMediaItemPropertyPlaybackDuration,
            MPMediaItemPropertyTitle, MPNowPlayingInfoCenter,
            MPNowPlayingInfoPropertyElapsedPlaybackTime, MPNowPlayingInfoPropertyPlaybackRate,
            MPNowPlayingPlaybackState,
        };

        // SAFETY: the default centre is a process-wide singleton.
        let center = unsafe { MPNowPlayingInfoCenter::defaultCenter() };
        let Some(now) = now else {
            // SAFETY: nil clears the info; the state is a plain enum.
            unsafe {
                center.setNowPlayingInfo(None);
                center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
            }
            return;
        };
        let mut keys: Vec<&NSString> = Vec::new();
        let mut values: Vec<Retained<AnyObject>> = Vec::new();
        let title = NSString::from_str(&now.title);
        // SAFETY: the property keys are constant `NSString`s.
        unsafe {
            keys.push(MPMediaItemPropertyTitle);
            values.push(Retained::into_super(Retained::into_super(title)));
            if let Some(artist) = &now.artist {
                keys.push(MPMediaItemPropertyArtist);
                let artist = NSString::from_str(artist);
                values.push(Retained::into_super(Retained::into_super(artist)));
            }
            if let Some(d) = now.duration_secs {
                keys.push(MPMediaItemPropertyPlaybackDuration);
                values.push(number(NSNumber::numberWithDouble(d)));
            }
            keys.push(MPNowPlayingInfoPropertyElapsedPlaybackTime);
            values.push(number(NSNumber::numberWithDouble(now.elapsed_secs)));
            keys.push(MPNowPlayingInfoPropertyPlaybackRate);
            let rate = if now.playing { 1.0 } else { 0.0 };
            values.push(number(NSNumber::numberWithDouble(rate)));
        }
        let refs: Vec<&AnyObject> = values.iter().map(|v| &**v).collect();
        let info = NSDictionary::from_slices(&keys, &refs);
        let state = if now.playing {
            MPNowPlayingPlaybackState::Playing
        } else {
            MPNowPlayingPlaybackState::Paused
        };
        // SAFETY: a dictionary of the documented key and value types; the
        // state is a plain enum.
        unsafe {
            center.setNowPlayingInfo(Some(&info));
            center.setPlaybackState(state);
        }
    }
}

fn number(n: Retained<objc2_foundation::NSNumber>) -> Retained<AnyObject> {
    Retained::into_super(Retained::into_super(Retained::into_super(n)))
}

/// Clip the window to the union of `polygons` (logical coordinates, as in
/// `region.txt`), multiplied by `scale` for double-size mode.
///
/// softbuffer draws into a sublayer of the content view's layer and ignores
/// alpha, so per-pixel transparency is not available. Instead, the root layer
/// gets a mask: a container with one filled `CAShapeLayer` per polygon, whose
/// composite alpha is their union — the semantics `region.txt` needs and a
/// single even-odd or non-zero path cannot guarantee for overlapping
/// polygons. The window must have been created transparent, or the masked-out
/// area shows the window background instead of the desktop.
///
/// An empty list removes the mask. Returns `false` if the view is not
/// layer-backed, which winit guarantees on every supported macOS.
pub fn set_window_region(window: &Window, polygons: &[Polygon], scale: f64) -> bool {
    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return false;
    };
    // SAFETY: winit hands out a live `NSView *` for as long as the window
    // exists, and we only use it within this call.
    let view: &AnyObject = unsafe { appkit.ns_view.cast().as_ref() };
    // SAFETY: `-[NSView layer]` returns `CALayer *` or nil.
    let Some(root): Option<Retained<CALayer>> = (unsafe { msg_send![view, layer] }) else {
        return false;
    };

    if polygons.is_empty() {
        // SAFETY: nil is a valid mask, meaning "none".
        unsafe { root.setMask(None) };
    } else {
        let mask = CALayer::new();
        mask.setFrame(root.bounds());
        // winit's view is flipped (top-left origin), and so is `region.txt`;
        // make the mask agree so the polygons land where they are drawn.
        mask.setGeometryFlipped(root.isGeometryFlipped());
        for polygon in polygons {
            let shape = CAShapeLayer::new();
            shape.setFrame(root.bounds());
            shape.setPath(Some(&path(polygon, scale)));
            mask.addSublayer(&shape);
        }
        // SAFETY: `mask` is a fresh layer with no superlayer, as `setMask:`
        // requires.
        unsafe { root.setMask(Some(&mask)) };
    }

    // The drop shadow follows the window's alpha, which the mask just changed.
    // SAFETY: `-[NSView window]` returns the owning `NSWindow *`, and
    // `-invalidateShadow` takes no arguments.
    let ns_window: Option<Retained<AnyObject>> = unsafe { msg_send![view, window] };
    if let Some(ns_window) = ns_window {
        let _: () = unsafe { msg_send![&*ns_window, invalidateShadow] };
    }
    true
}

fn path(polygon: &Polygon, scale: f64) -> objc2_core_foundation::CFRetained<CGMutablePath> {
    let path = CGMutablePath::new();
    for (i, &(x, y)) in polygon.points.iter().enumerate() {
        let (x, y) = (f64::from(x) * scale, f64::from(y) * scale);
        // SAFETY: a null transform means identity.
        unsafe {
            if i == 0 {
                CGMutablePath::move_to_point(Some(&path), std::ptr::null(), x, y);
            } else {
                CGMutablePath::add_line_to_point(Some(&path), std::ptr::null(), x, y);
            }
        }
    }
    CGMutablePath::close_subpath(Some(&path));
    path
}

/// The pointer's position on screen, in points from the top-left of the
/// primary display — winit's window coordinates. Read fresh rather than
/// from a mouse event: while a window is being dragged, an event's
/// window-relative position goes stale as soon as the window moves.
pub fn cursor_position() -> Option<(f64, f64)> {
    use objc2_foundation::NSPoint;
    let class = objc2::runtime::AnyClass::get(c"NSEvent")?;
    // SAFETY: `+[NSEvent mouseLocation]` takes nothing, returns an NSPoint.
    let at: NSPoint = unsafe { msg_send![class, mouseLocation] };
    let primary_height = screen_rects(false).first()?.3;
    Some((at.x, primary_height - at.y))
}

/// Each display's usable area — without the menu bar and Dock — as
/// `(x, y, width, height)` in the same top-left coordinates.
pub fn visible_screen_rects() -> Vec<(f64, f64, f64, f64)> {
    screen_rects(true)
}

fn screen_rects(visible: bool) -> Vec<(f64, f64, f64, f64)> {
    use objc2_foundation::{NSArray, NSRect};
    let Some(class) = objc2::runtime::AnyClass::get(c"NSScreen") else {
        return Vec::new();
    };
    // SAFETY: `+[NSScreen screens]` returns an array of NSScreen; `frame`
    // and `visibleFrame` return NSRects. The first screen is the primary,
    // whose bottom-left corner is AppKit's origin.
    unsafe {
        let screens: Option<Retained<NSArray<AnyObject>>> = msg_send![class, screens];
        let Some(screens) = screens else {
            return Vec::new();
        };
        let frames: Vec<(NSRect, NSRect)> = screens
            .iter()
            .map(|s| {
                let frame: NSRect = msg_send![&*s, frame];
                let usable: NSRect = msg_send![&*s, visibleFrame];
                (frame, usable)
            })
            .collect();
        let Some(primary_height) = frames.first().map(|(f, _)| f.size.height) else {
            return Vec::new();
        };
        frames
            .into_iter()
            .map(|(frame, usable)| {
                let r = if visible { usable } else { frame };
                let top = primary_height - (r.origin.y + r.size.height);
                (r.origin.x, top, r.size.width, r.size.height)
            })
            .collect()
    }
}
