//! Sheet identities, sprite keys and their coordinates.
//!
//! Coordinates are a functional fact of the format — a skin cannot move a
//! button, only repaint it — which is what makes pixel-exact compatibility
//! achievable at all. They derive from Webamp's `skinSprites.ts` (MIT,
//! Copyright (c) 2015 Jordan Eldredge).
//!
//! `gen.bmp`, `genex.bmp`, `video.bmp`, `mb.bmp` and `avs.bmp` skin windows
//! this player does not have (media library, video, minibrowser, AVS), so
//! they are deliberately not modelled.

use crate::font;

/// A rectangle within a sheet, in the skin's own pixel space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }
}

/// The sprite sheets a classic skin may contain.
///
/// Discriminants are contiguous so [`SheetId`] can index a dense array.
/// Nearly all are optional: skins routinely ship a subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum SheetId {
    Main = 0,
    TitleBar,
    CButtons,
    /// `numbers.bmp`, or `nums_ex.bmp` where present — the latter wins,
    /// matching Webamp's CSS cascade order. [`crate::Skin::resolve`] accounts
    /// for the two sheets placing their minus sign differently.
    Numbers,
    PlayPaus,
    MonoSter,
    PosBar,
    Volume,
    Balance,
    ShufRep,
    EqMain,
    EqEx,
    PlEdit,
    Text,
}

impl SheetId {
    pub const COUNT: usize = 14;

    /// Every sheet, for iteration during parsing.
    pub const ALL: [SheetId; Self::COUNT] = [
        SheetId::Main,
        SheetId::TitleBar,
        SheetId::CButtons,
        SheetId::Numbers,
        SheetId::PlayPaus,
        SheetId::MonoSter,
        SheetId::PosBar,
        SheetId::Volume,
        SheetId::Balance,
        SheetId::ShufRep,
        SheetId::EqMain,
        SheetId::EqEx,
        SheetId::PlEdit,
        SheetId::Text,
    ];

    /// Base filename, without extension. Lookup tries `.bmp` then `.png`,
    /// case-insensitively.
    pub fn base_name(self) -> &'static str {
        match self {
            SheetId::Main => "main",
            SheetId::TitleBar => "titlebar",
            SheetId::CButtons => "cbuttons",
            SheetId::Numbers => "numbers",
            SheetId::PlayPaus => "playpaus",
            SheetId::MonoSter => "monoster",
            SheetId::PosBar => "posbar",
            SheetId::Volume => "volume",
            SheetId::Balance => "balance",
            SheetId::ShufRep => "shufrep",
            SheetId::EqMain => "eqmain",
            SheetId::EqEx => "eq_ex",
            SheetId::PlEdit => "pledit",
            SheetId::Text => "text",
        }
    }
}

/// The five letters of the clutter bar down the main window's left edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClutterButton {
    /// Options menu.
    O,
    /// Always on top.
    A,
    /// File info.
    I,
    /// Double size.
    D,
    /// Visualisation menu.
    V,
}

impl ClutterButton {
    pub const ALL: [ClutterButton; 5] = [Self::O, Self::A, Self::I, Self::D, Self::V];
}

/// The three-frame thumb used by windowshade-mode mini sliders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShadeThumb {
    Left,
    Center,
    Right,
}

impl ShadeThumb {
    pub const ALL: [ShadeThumb; 3] = [Self::Left, Self::Center, Self::Right];
}

/// Buttons in the playlist's bottom-bar pop-up menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaylistButton {
    AddUrl,
    AddDir,
    AddFile,
    RemoveAll,
    Crop,
    RemoveSelected,
    RemoveMisc,
    InvertSelection,
    SelectZero,
    SelectAll,
    SortList,
    FileInfo,
    MiscOptions,
    NewList,
    SaveList,
    LoadList,
}

impl PlaylistButton {
    pub const ALL: [PlaylistButton; 16] = [
        Self::AddUrl,
        Self::AddDir,
        Self::AddFile,
        Self::RemoveAll,
        Self::Crop,
        Self::RemoveSelected,
        Self::RemoveMisc,
        Self::InvertSelection,
        Self::SelectZero,
        Self::SelectAll,
        Self::SortList,
        Self::FileInfo,
        Self::MiscOptions,
        Self::NewList,
        Self::SaveList,
        Self::LoadList,
    ];
}

/// The vertical bar drawn beside each playlist pop-up menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaylistMenu {
    Add,
    Remove,
    Select,
    Misc,
    List,
}

impl PlaylistMenu {
    pub const ALL: [PlaylistMenu; 5] = [
        Self::Add,
        Self::Remove,
        Self::Select,
        Self::Misc,
        Self::List,
    ];
}

/// A drawable sprite. Resolving one yields a [`SheetId`] and a [`Rect`].
///
/// "Active" means the window has focus or the toggle is on; "pressed" means
/// the mouse is down on it. Both vocabularies map onto Webamp's
/// `_SELECTED`/`_DEPRESSED`/`_ACTIVE` suffixes, which are not used
/// consistently across sheets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpriteKey {
    // main.bmp
    MainBackground,

    // titlebar.bmp
    TitleBar {
        active: bool,
    },
    EasterEggTitleBar {
        active: bool,
    },
    OptionsButton {
        pressed: bool,
    },
    MinimizeButton {
        pressed: bool,
    },
    ShadeButton {
        pressed: bool,
    },
    CloseButton {
        pressed: bool,
    },
    ClutterBar {
        enabled: bool,
    },
    ClutterBarButton(ClutterButton),
    ShadeBackground {
        active: bool,
    },
    /// The shade button as drawn while *in* windowshade mode.
    UnshadeButton {
        pressed: bool,
    },
    ShadePositionBackground,
    ShadePositionThumb(ShadeThumb),

    // cbuttons.bmp
    Previous {
        pressed: bool,
    },
    Play {
        pressed: bool,
    },
    Pause {
        pressed: bool,
    },
    Stop {
        pressed: bool,
    },
    Next {
        pressed: bool,
    },
    Eject {
        pressed: bool,
    },

    // numbers.bmp / nums_ex.bmp
    /// A time digit, 0-9. Out-of-range values clamp.
    Digit(u8),
    MinusSign,
    NoMinusSign,

    // playpaus.bmp
    PlayingIndicator,
    PausedIndicator,
    StoppedIndicator,
    NotWorkingIndicator,
    WorkingIndicator,

    // monoster.bmp
    Mono {
        active: bool,
    },
    Stereo {
        active: bool,
    },

    // posbar.bmp
    PosBarBackground,
    PosBarThumb {
        pressed: bool,
    },

    // volume.bmp
    /// Track background; 28 prerendered variants, 0-27. Clamps.
    VolumeBackground(u8),
    VolumeThumb {
        pressed: bool,
    },

    // balance.bmp
    /// Track background; 28 prerendered variants, 0-27. Clamps.
    BalanceBackground(u8),
    BalanceThumb {
        pressed: bool,
    },

    // shufrep.bmp
    Repeat {
        active: bool,
        pressed: bool,
    },
    Shuffle {
        active: bool,
        pressed: bool,
    },
    EqToggle {
        active: bool,
        pressed: bool,
    },
    PlaylistToggle {
        active: bool,
        pressed: bool,
    },

    // eqmain.bmp
    EqBackground,
    EqTitleBar {
        active: bool,
    },
    /// Band track background; 28 prerendered variants, 0-27. Clamps.
    EqSliderBackground(u8),
    EqSliderThumb {
        pressed: bool,
    },
    EqCloseButton {
        pressed: bool,
    },
    /// Not a sprite in Winamp — a patch of the title bar — but used as the
    /// maximise button when a skin has no `eq_ex.bmp`. Webamp does the same.
    EqMaximizeButtonFallback,
    EqOnButton {
        active: bool,
        pressed: bool,
    },
    EqAutoButton {
        active: bool,
        pressed: bool,
    },
    EqPresetsButton {
        pressed: bool,
    },
    EqGraphBackground,
    /// A 1x19 column: the colour of the response curve at each height.
    EqGraphLineColors,
    EqPreampLine,

    // eq_ex.bmp
    EqShadeBackground {
        active: bool,
    },
    EqShadeVolumeThumb(ShadeThumb),
    EqShadeBalanceThumb(ShadeThumb),
    EqMaximizeButton,
    EqMinimizeButton,
    EqShadeCloseButton {
        pressed: bool,
    },

    // pledit.bmp
    PlaylistTopLeftCorner {
        active: bool,
    },
    PlaylistTitleBar {
        active: bool,
    },
    PlaylistTopTile {
        active: bool,
    },
    PlaylistTopRightCorner {
        active: bool,
    },
    PlaylistLeftTile,
    PlaylistRightTile,
    PlaylistBottomTile,
    PlaylistBottomLeftCorner,
    PlaylistBottomRightCorner,
    PlaylistVisualizerBackground,
    PlaylistShadeBackground,
    PlaylistShadeBackgroundLeft,
    PlaylistShadeBackgroundRight {
        active: bool,
    },
    PlaylistScrollHandle {
        pressed: bool,
    },
    PlaylistButton {
        button: PlaylistButton,
        pressed: bool,
    },
    PlaylistMenuBar(PlaylistMenu),
    PlaylistCloseButton,
    PlaylistCollapseButton,
    PlaylistExpandButton,

    // text.bmp
    /// One 5x6 glyph of the bitmap font. Unknown characters draw as a space.
    Char(char),
}

/// Where a sprite lives: which sheet, and the rectangle within it.
///
/// For [`SpriteKey::MinusSign`] and [`SpriteKey::NoMinusSign`] this gives the
/// `numbers.bmp` position; use [`crate::Skin::resolve`], which knows whether
/// the skin supplied `nums_ex.bmp` instead.
pub fn resolve(key: SpriteKey) -> (SheetId, Rect) {
    use SheetId as S;
    use SpriteKey as K;
    let r = Rect::new;
    match key {
        K::MainBackground => (S::Main, r(0, 0, 275, 116)),

        // Title bar variants are 275 wide, starting after a 27px column of
        // small buttons; active (focused) rows sit above inactive ones.
        K::TitleBar { active } => (S::TitleBar, r(27, pick(active, 0, 15), 275, 14)),
        K::EasterEggTitleBar { active } => (S::TitleBar, r(27, pick(active, 57, 72), 275, 14)),
        K::OptionsButton { pressed } => (S::TitleBar, r(0, pick(pressed, 9, 0), 9, 9)),
        K::MinimizeButton { pressed } => (S::TitleBar, r(9, pick(pressed, 9, 0), 9, 9)),
        // Shade's pressed frame is beside it, not below — unlike its neighbours.
        K::ShadeButton { pressed } => (S::TitleBar, r(pick(pressed, 9, 0), 18, 9, 9)),
        K::CloseButton { pressed } => (S::TitleBar, r(18, pick(pressed, 9, 0), 9, 9)),
        K::ClutterBar { enabled } => (S::TitleBar, r(pick(enabled, 304, 312), 0, 8, 43)),
        K::ClutterBarButton(b) => {
            let (rect_x, y, h) = match b {
                ClutterButton::O => (304, 47, 8),
                ClutterButton::A => (312, 55, 7),
                ClutterButton::I => (320, 62, 7),
                ClutterButton::D => (328, 69, 8),
                ClutterButton::V => (336, 77, 7),
            };
            (S::TitleBar, r(rect_x, y, 8, h))
        }
        K::ShadeBackground { active } => (S::TitleBar, r(27, pick(active, 29, 42), 275, 14)),
        K::UnshadeButton { pressed } => (S::TitleBar, r(pick(pressed, 9, 0), 27, 9, 9)),
        K::ShadePositionBackground => (S::TitleBar, r(0, 36, 17, 7)),
        K::ShadePositionThumb(t) => (S::TitleBar, r(shade_thumb_x(t, 17), 36, 3, 7)),

        // cbuttons.bmp is 136x36: six buttons across, normal row over pressed
        // row. Eject is shorter (16px) than the other five (18px), so its
        // pressed variant sits at y=16 rather than y=18. Next is 22 wide in
        // the window; Webamp's 23px unpressed frame only overlaps Eject.
        K::Previous { pressed } => (S::CButtons, r(0, row(pressed, 18), 23, 18)),
        K::Play { pressed } => (S::CButtons, r(23, row(pressed, 18), 23, 18)),
        K::Pause { pressed } => (S::CButtons, r(46, row(pressed, 18), 23, 18)),
        K::Stop { pressed } => (S::CButtons, r(69, row(pressed, 18), 23, 18)),
        K::Next { pressed } => (S::CButtons, r(92, row(pressed, 18), 22, 18)),
        K::Eject { pressed } => (S::CButtons, r(114, row(pressed, 16), 22, 16)),

        K::Digit(n) => (S::Numbers, r(u32::from(n.min(9)) * 9, 0, 9, 13)),
        // In numbers.bmp the minus is a 5x1 dash cut from the middle of a
        // cell; nums_ex.bmp gives it two full cells of its own.
        K::MinusSign => (S::Numbers, r(20, 6, 5, 1)),
        K::NoMinusSign => (S::Numbers, r(9, 6, 5, 1)),

        K::PlayingIndicator => (S::PlayPaus, r(0, 0, 9, 9)),
        K::PausedIndicator => (S::PlayPaus, r(9, 0, 9, 9)),
        K::StoppedIndicator => (S::PlayPaus, r(18, 0, 9, 9)),
        // 3px slivers drawn beside the 9px state icon. Webamp declares them
        // 9 wide and lets the element clip; 3 keeps them inside the 42px sheet.
        K::NotWorkingIndicator => (S::PlayPaus, r(36, 0, 3, 9)),
        K::WorkingIndicator => (S::PlayPaus, r(39, 0, 3, 9)),

        K::Stereo { active } => (S::MonoSter, r(0, pick(active, 0, 12), 29, 12)),
        K::Mono { active } => (S::MonoSter, r(29, pick(active, 0, 12), 27, 12)),

        K::PosBarBackground => (S::PosBar, r(0, 0, 248, 10)),
        K::PosBarThumb { pressed } => (S::PosBar, r(pick(pressed, 278, 248), 0, 29, 10)),

        // 28 track frames stacked 15px apart, each 13 tall, thumbs below.
        K::VolumeBackground(p) => (S::Volume, r(0, u32::from(p.min(27)) * 15, 68, 13)),
        K::VolumeThumb { pressed } => (S::Volume, r(pick(pressed, 0, 15), 422, 14, 11)),
        K::BalanceBackground(p) => (S::Balance, r(9, u32::from(p.min(27)) * 15, 38, 13)),
        K::BalanceThumb { pressed } => (S::Balance, r(pick(pressed, 0, 15), 422, 14, 11)),

        K::Repeat { active, pressed } => (S::ShufRep, r(0, shufrep_y(active, pressed), 28, 15)),
        K::Shuffle { active, pressed } => (S::ShufRep, r(28, shufrep_y(active, pressed), 47, 15)),
        K::EqToggle { active, pressed } => (
            S::ShufRep,
            r(pick(pressed, 46, 0), pick(active, 73, 61), 23, 12),
        ),
        K::PlaylistToggle { active, pressed } => (
            S::ShufRep,
            r(pick(pressed, 69, 23), pick(active, 73, 61), 23, 12),
        ),

        K::EqBackground => (S::EqMain, r(0, 0, 275, 116)),
        K::EqTitleBar { active } => (S::EqMain, r(0, pick(active, 134, 149), 275, 14)),
        // A 14x2 grid of 14x63 frames starting at (13, 164), 15px apart
        // horizontally and 65px vertically.
        K::EqSliderBackground(p) => {
            let p = u32::from(p.min(27));
            (
                S::EqMain,
                r(13 + (p % 14) * 15, 164 + (p / 14) * 65, 14, 63),
            )
        }
        K::EqSliderThumb { pressed } => (S::EqMain, r(0, pick(pressed, 176, 164), 11, 11)),
        K::EqCloseButton { pressed } => (S::EqMain, r(0, pick(pressed, 125, 116), 9, 9)),
        K::EqMaximizeButtonFallback => (S::EqMain, r(254, 152, 9, 9)),
        K::EqOnButton { active, pressed } => {
            let x = match (active, pressed) {
                (false, false) => 10,
                (true, false) => 69,
                (false, true) => 128,
                (true, true) => 187,
            };
            (S::EqMain, r(x, 119, 26, 12))
        }
        K::EqAutoButton { active, pressed } => {
            let x = match (active, pressed) {
                (false, false) => 36,
                (true, false) => 95,
                (false, true) => 154,
                (true, true) => 213,
            };
            (S::EqMain, r(x, 119, 32, 12))
        }
        K::EqPresetsButton { pressed } => (S::EqMain, r(224, pick(pressed, 176, 164), 44, 12)),
        K::EqGraphBackground => (S::EqMain, r(0, 294, 113, 19)),
        K::EqGraphLineColors => (S::EqMain, r(115, 294, 1, 19)),
        K::EqPreampLine => (S::EqMain, r(0, 314, 113, 1)),

        K::EqShadeBackground { active } => (S::EqEx, r(0, pick(active, 0, 15), 275, 14)),
        K::EqShadeVolumeThumb(t) => (S::EqEx, r(shade_thumb_x(t, 1), 30, 3, 7)),
        K::EqShadeBalanceThumb(t) => (S::EqEx, r(shade_thumb_x(t, 11), 30, 3, 7)),
        K::EqMaximizeButton => (S::EqEx, r(1, 38, 9, 9)),
        K::EqMinimizeButton => (S::EqEx, r(1, 47, 9, 9)),
        K::EqShadeCloseButton { pressed } => (S::EqEx, r(11, pick(pressed, 47, 38), 9, 9)),

        K::PlaylistTopLeftCorner { active } => (S::PlEdit, r(0, pick(active, 0, 21), 25, 20)),
        K::PlaylistTitleBar { active } => (S::PlEdit, r(26, pick(active, 0, 21), 100, 20)),
        K::PlaylistTopTile { active } => (S::PlEdit, r(127, pick(active, 0, 21), 25, 20)),
        K::PlaylistTopRightCorner { active } => (S::PlEdit, r(153, pick(active, 0, 21), 25, 20)),
        K::PlaylistLeftTile => (S::PlEdit, r(0, 42, 12, 29)),
        K::PlaylistRightTile => (S::PlEdit, r(31, 42, 20, 29)),
        K::PlaylistBottomTile => (S::PlEdit, r(179, 0, 25, 38)),
        K::PlaylistBottomLeftCorner => (S::PlEdit, r(0, 72, 125, 38)),
        K::PlaylistBottomRightCorner => (S::PlEdit, r(126, 72, 150, 38)),
        K::PlaylistVisualizerBackground => (S::PlEdit, r(205, 0, 75, 38)),
        K::PlaylistShadeBackground => (S::PlEdit, r(72, 57, 25, 14)),
        K::PlaylistShadeBackgroundLeft => (S::PlEdit, r(72, 42, 25, 14)),
        K::PlaylistShadeBackgroundRight { active } => {
            (S::PlEdit, r(99, pick(active, 42, 57), 50, 14))
        }
        K::PlaylistScrollHandle { pressed } => (S::PlEdit, r(pick(pressed, 61, 52), 53, 8, 18)),
        K::PlaylistButton { button, pressed } => {
            // Four menu columns of buttons, 19px apart vertically; each
            // column's pressed frames sit 23px to the right.
            use PlaylistButton as B;
            let (x, y) = match button {
                B::AddUrl => (0, 111),
                B::AddDir => (0, 130),
                B::AddFile => (0, 149),
                B::RemoveAll => (54, 111),
                B::Crop => (54, 130),
                B::RemoveSelected => (54, 149),
                B::RemoveMisc => (54, 168),
                B::InvertSelection => (104, 111),
                B::SelectZero => (104, 130),
                B::SelectAll => (104, 149),
                B::SortList => (154, 111),
                B::FileInfo => (154, 130),
                B::MiscOptions => (154, 149),
                B::NewList => (204, 111),
                B::SaveList => (204, 130),
                B::LoadList => (204, 149),
            };
            (S::PlEdit, r(x + row(pressed, 23), y, 22, 18))
        }
        K::PlaylistMenuBar(menu) => {
            let (x, h) = match menu {
                PlaylistMenu::Add => (48, 54),
                PlaylistMenu::Remove => (100, 72),
                PlaylistMenu::Select => (150, 54),
                PlaylistMenu::Misc => (200, 54),
                PlaylistMenu::List => (250, 54),
            };
            (S::PlEdit, r(x, 111, 3, h))
        }
        K::PlaylistCloseButton => (S::PlEdit, r(52, 42, 9, 9)),
        K::PlaylistCollapseButton => (S::PlEdit, r(62, 42, 9, 9)),
        K::PlaylistExpandButton => (S::PlEdit, r(150, 42, 9, 9)),

        K::Char(c) => {
            let (row, col) = font::cell(c);
            (
                S::Text,
                r(
                    u32::from(col) * font::GLYPH_WIDTH,
                    u32::from(row) * font::GLYPH_HEIGHT,
                    font::GLYPH_WIDTH,
                    font::GLYPH_HEIGHT,
                ),
            )
        }
    }
}

/// `nums_ex.bmp` coordinates for the keys whose position differs from
/// `numbers.bmp`, or `None` when the key is laid out identically.
pub(crate) fn resolve_nums_ex(key: SpriteKey) -> Option<Rect> {
    match key {
        SpriteKey::NoMinusSign => Some(Rect::new(90, 0, 9, 13)),
        SpriteKey::MinusSign => Some(Rect::new(99, 0, 9, 13)),
        _ => None,
    }
}

/// Every distinct key, for exhaustive tests and fixture generation.
pub fn all_keys() -> Vec<SpriteKey> {
    use SpriteKey as K;
    let mut keys = vec![
        K::MainBackground,
        K::ShadePositionBackground,
        K::MinusSign,
        K::NoMinusSign,
        K::PlayingIndicator,
        K::PausedIndicator,
        K::StoppedIndicator,
        K::NotWorkingIndicator,
        K::WorkingIndicator,
        K::PosBarBackground,
        K::EqBackground,
        K::EqMaximizeButtonFallback,
        K::EqGraphBackground,
        K::EqGraphLineColors,
        K::EqPreampLine,
        K::EqMaximizeButton,
        K::EqMinimizeButton,
        K::PlaylistLeftTile,
        K::PlaylistRightTile,
        K::PlaylistBottomTile,
        K::PlaylistBottomLeftCorner,
        K::PlaylistBottomRightCorner,
        K::PlaylistVisualizerBackground,
        K::PlaylistShadeBackground,
        K::PlaylistShadeBackgroundLeft,
        K::PlaylistCloseButton,
        K::PlaylistCollapseButton,
        K::PlaylistExpandButton,
    ];
    for b in [false, true] {
        keys.extend([
            K::TitleBar { active: b },
            K::EasterEggTitleBar { active: b },
            K::OptionsButton { pressed: b },
            K::MinimizeButton { pressed: b },
            K::ShadeButton { pressed: b },
            K::CloseButton { pressed: b },
            K::ClutterBar { enabled: b },
            K::ShadeBackground { active: b },
            K::UnshadeButton { pressed: b },
            K::Previous { pressed: b },
            K::Play { pressed: b },
            K::Pause { pressed: b },
            K::Stop { pressed: b },
            K::Next { pressed: b },
            K::Eject { pressed: b },
            K::Mono { active: b },
            K::Stereo { active: b },
            K::PosBarThumb { pressed: b },
            K::VolumeThumb { pressed: b },
            K::BalanceThumb { pressed: b },
            K::EqTitleBar { active: b },
            K::EqSliderThumb { pressed: b },
            K::EqCloseButton { pressed: b },
            K::EqPresetsButton { pressed: b },
            K::EqShadeBackground { active: b },
            K::EqShadeCloseButton { pressed: b },
            K::PlaylistTopLeftCorner { active: b },
            K::PlaylistTitleBar { active: b },
            K::PlaylistTopTile { active: b },
            K::PlaylistTopRightCorner { active: b },
            K::PlaylistShadeBackgroundRight { active: b },
            K::PlaylistScrollHandle { pressed: b },
        ]);
        for p in [false, true] {
            keys.extend([
                K::Repeat {
                    active: b,
                    pressed: p,
                },
                K::Shuffle {
                    active: b,
                    pressed: p,
                },
                K::EqToggle {
                    active: b,
                    pressed: p,
                },
                K::PlaylistToggle {
                    active: b,
                    pressed: p,
                },
                K::EqOnButton {
                    active: b,
                    pressed: p,
                },
                K::EqAutoButton {
                    active: b,
                    pressed: p,
                },
            ]);
        }
        for button in PlaylistButton::ALL {
            keys.push(K::PlaylistButton { button, pressed: b });
        }
    }
    keys.extend(ClutterButton::ALL.map(K::ClutterBarButton));
    for t in ShadeThumb::ALL {
        keys.extend([
            K::ShadePositionThumb(t),
            K::EqShadeVolumeThumb(t),
            K::EqShadeBalanceThumb(t),
        ]);
    }
    keys.extend(PlaylistMenu::ALL.map(K::PlaylistMenuBar));
    keys.extend((0..10).map(K::Digit));
    for p in 0..28 {
        keys.extend([
            K::VolumeBackground(p),
            K::BalanceBackground(p),
            K::EqSliderBackground(p),
        ]);
    }
    keys.extend(font::CHARSET.chars().map(K::Char));
    keys
}

/// The size a sheet has in a conforming skin.
///
/// Real skins deviate — larger is common, smaller happens — so this is for
/// tests and fixture generation, never for rejecting a skin.
pub fn standard_size(id: SheetId) -> (u32, u32) {
    match id {
        SheetId::Main => (275, 116),
        SheetId::TitleBar => (344, 87),
        SheetId::CButtons => (136, 36),
        SheetId::Numbers => (99, 13),
        SheetId::PlayPaus => (42, 9),
        SheetId::MonoSter => (56, 24),
        SheetId::PosBar => (307, 10),
        SheetId::Volume => (68, 433),
        SheetId::Balance => (47, 433),
        SheetId::ShufRep => (92, 85),
        SheetId::EqMain => (275, 315),
        SheetId::EqEx => (275, 56),
        SheetId::PlEdit => (280, 186),
        SheetId::Text => (155, 18),
    }
}

/// Pressed variants sit one row down, by the button's own height.
const fn row(pressed: bool, height: u32) -> u32 {
    if pressed { height } else { 0 }
}

const fn pick(flag: bool, yes: u32, no: u32) -> u32 {
    if flag { yes } else { no }
}

/// Repeat and shuffle: off/off-pressed/on/on-pressed, stacked 15px apart.
const fn shufrep_y(active: bool, pressed: bool) -> u32 {
    pick(active, 30, 0) + pick(pressed, 15, 0)
}

/// Shade thumbs are three adjacent 3px frames: left, centre, right.
const fn shade_thumb_x(t: ShadeThumb, base: u32) -> u32 {
    base + match t {
        ShadeThumb::Left => 0,
        ShadeThumb::Center => 3,
        ShadeThumb::Right => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_buttons_tile_the_sheet_without_overlap() {
        let keys = [
            SpriteKey::Previous { pressed: false },
            SpriteKey::Play { pressed: false },
            SpriteKey::Pause { pressed: false },
            SpriteKey::Stop { pressed: false },
            SpriteKey::Next { pressed: false },
        ];
        let mut x = 0;
        for k in keys {
            let (sheet, r) = resolve(k);
            assert_eq!(sheet, SheetId::CButtons);
            assert_eq!(r.x, x, "{k:?} should start where the previous ended");
            x += r.w;
        }
        // Eject follows next, and the row ends at the sheet width.
        let (_, eject) = resolve(SpriteKey::Eject { pressed: false });
        assert_eq!(eject.x, x);
        assert_eq!(eject.x + eject.w, 136, "row should fill cbuttons.bmp");
    }

    #[test]
    fn pressed_variants_sit_below_by_their_own_height() {
        let (_, up) = resolve(SpriteKey::Play { pressed: false });
        let (_, down) = resolve(SpriteKey::Play { pressed: true });
        assert_eq!(down.y, up.y + up.h);

        // Eject is 16px tall, not 18 — a real trap in this sheet.
        let (_, e_up) = resolve(SpriteKey::Eject { pressed: false });
        let (_, e_down) = resolve(SpriteKey::Eject { pressed: true });
        assert_eq!(e_up.h, 16);
        assert_eq!(e_down.y, 16);
    }

    #[test]
    fn every_key_fits_its_sheet_at_standard_size() {
        for key in all_keys() {
            let (sheet, r) = resolve(key);
            let (w, h) = standard_size(sheet);
            assert!(
                r.w > 0 && r.h > 0 && r.x + r.w <= w && r.y + r.h <= h,
                "{key:?} -> {r:?} overflows {}.bmp ({w}x{h})",
                sheet.base_name()
            );
        }
    }

    #[test]
    fn nums_ex_minus_signs_fit_nums_ex() {
        for key in [SpriteKey::MinusSign, SpriteKey::NoMinusSign] {
            let r = resolve_nums_ex(key).unwrap();
            assert!(r.x + r.w <= 108 && r.y + r.h <= 13);
        }
        assert_eq!(resolve_nums_ex(SpriteKey::Digit(3)), None);
    }

    #[test]
    fn indexed_variants_clamp_rather_than_overflow() {
        assert_eq!(resolve(SpriteKey::Digit(200)), resolve(SpriteKey::Digit(9)));
        assert_eq!(
            resolve(SpriteKey::VolumeBackground(99)),
            resolve(SpriteKey::VolumeBackground(27))
        );
        assert_eq!(
            resolve(SpriteKey::EqSliderBackground(99)),
            resolve(SpriteKey::EqSliderBackground(27))
        );
    }

    #[test]
    fn eq_slider_frames_wrap_onto_a_second_row() {
        let (_, last_top) = resolve(SpriteKey::EqSliderBackground(13));
        let (_, first_bottom) = resolve(SpriteKey::EqSliderBackground(14));
        assert_eq!((last_top.x, last_top.y), (13 + 13 * 15, 164));
        assert_eq!((first_bottom.x, first_bottom.y), (13, 164 + 65));
    }

    #[test]
    fn shade_button_presses_sideways() {
        let (_, up) = resolve(SpriteKey::ShadeButton { pressed: false });
        let (_, down) = resolve(SpriteKey::ShadeButton { pressed: true });
        assert_eq!(up.y, down.y);
        assert_eq!(down.x, up.x + 9);
    }

    #[test]
    fn keys_are_distinct() {
        let keys = all_keys();
        let set: std::collections::HashSet<_> = keys.iter().collect();
        assert_eq!(set.len(), keys.len(), "all_keys() lists something twice");
    }
}
