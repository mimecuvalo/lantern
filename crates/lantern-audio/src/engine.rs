//! Transport, the decoder thread and the realtime output callback.
//!
//! See the crate docs for the thread diagram. The invariant that matters: the
//! cpal callback allocates nothing, locks nothing and makes no syscalls. It
//! pops from a lock-free ring, applies gain, copies a mono mix into the tap
//! ring for the analyser, and touches a handful of atomics.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::AudioError;
use crate::eq::{EqParams, EqSettings, Equalizer};
use crate::resample::Resampler;

/// What to play: a whole file, or a stretch of one (a cue-sheet track).
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub path: PathBuf,
    /// Seconds from the start of the file.
    pub start_secs: f64,
    /// `None` plays to the end of the file.
    pub end_secs: Option<f64>,
}

impl From<PathBuf> for Source {
    fn from(path: PathBuf) -> Self {
        Self {
            path,
            start_secs: 0.0,
            end_secs: None,
        }
    }
}

impl Source {
    /// Whether `next` begins exactly where this ends, in the same file —
    /// consecutive cue-sheet tracks — so the decoder can simply keep going.
    fn continues_into(&self, next: &Source) -> bool {
        self.path == next.path
            && self
                .end_secs
                .is_some_and(|e| (e - next.start_secs).abs() < 1e-6)
    }
}

/// What the UI asks the engine to do. Crosses to the decoder thread by channel.
#[derive(Debug, Clone)]
pub enum Command {
    Open(Source),
    /// The track to play when the current one ends, without a gap: the
    /// decoder opens it the moment the current file runs dry and keeps
    /// filling the same ring, so the callback never learns a track ended.
    /// Replaces any track already queued; `Open` and `Stop` clear it.
    Enqueue(Source),
    /// Withdraw the queued track, if any: the playlist changed so that
    /// nothing should follow.
    ClearQueue,
    Play,
    Pause,
    Stop,
    /// Jump to a position in the open track, in seconds from its start (for
    /// a cue track, from the track's start, not the file's).
    Seek {
        seconds: f64,
    },
    /// 0.0..=1.0.
    SetVolume(f32),
    /// -1.0 (left) ..= 1.0 (right).
    SetBalance(f32),
    /// Crosses to the callback as precomputed coefficients, not via the
    /// decoder.
    SetEq(EqSettings),
}

/// What the engine tells the UI. Poll with [`Engine::poll_event`].
#[derive(Debug, Clone)]
pub enum Event {
    /// A track opened by [`Command::Open`] is ready.
    Opened(TrackInfo),
    /// Playback has just crossed, gaplessly, into the queued track. Sent
    /// when the *audio* crosses, not when the decoder does, which may be
    /// seconds earlier.
    Advanced(TrackInfo),
    OpenFailed {
        path: PathBuf,
        error: String,
    },
    /// The track played to its end and the ring has drained; the transport
    /// is now stopped.
    Ended,
}

/// Facts about the open track, for the main window's displays.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackInfo {
    pub path: PathBuf,
    /// Where in the file this track starts and ends, as it was opened.
    pub start_secs: f64,
    pub end_secs: Option<f64>,
    /// "Artist - Title" from tags where present, else the file stem.
    pub title: String,
    /// The file's own rate, before conversion — what the kHz display shows.
    pub sample_rate: u32,
    pub channels: u16,
    /// Average bitrate, where the format has one.
    pub kbps: Option<u32>,
    /// Of the track as played: for a cue track, its own length.
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transport {
    #[default]
    Stopped,
    Playing,
    Paused,
}

impl Transport {
    fn as_u8(self) -> u8 {
        match self {
            Transport::Stopped => 0,
            Transport::Playing => 1,
            Transport::Paused => 2,
        }
    }
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Transport::Playing,
            2 => Transport::Paused,
            _ => Transport::Stopped,
        }
    }
}

/// Mono samples for the analyser. Roughly a third of a second at 48 kHz —
/// far more than one 60fps frame needs, so a slow UI frame loses nothing.
const TAP_CAPACITY: usize = 16_384;

/// How long a flush waits for the callback before giving up. The callback
/// runs every few milliseconds whenever the device is alive; this only
/// matters if it is not.
const FLUSH_TIMEOUT: Duration = Duration::from_millis(250);

/// Length of the fades either side of a flush: about 5 ms. Long enough that
/// cutting a waveform mid-cycle does not click, short enough to be heard as
/// instant.
const DECLICK_FRAMES: usize = 256;

/// EQ updates in flight. The callback drains them every few milliseconds, so
/// this only fills if it stalls.
const EQ_QUEUE: usize = 32;

/// State shared with the realtime callback. Every field is lock-free by
/// construction; nothing here may ever become a mutex.
struct Shared {
    transport: AtomicU8,
    /// Frames handed to the device since the track opened, or since the seek
    /// target. Written by the callback, read by the UI.
    frames_played: AtomicU64,
    /// `f32` bits, since there is no `AtomicF32`.
    volume_bits: AtomicU32,
    balance_bits: AtomicU32,
    /// Output rate, so the UI can turn frames into seconds.
    output_rate: AtomicU32,
    /// Set by the decoder to ask the callback to discard everything in the
    /// ring; cleared by the callback once it has. Only the consumer can empty
    /// an SPSC ring, so this handshake is how a seek or stop drops stale audio.
    flush: AtomicBool,
}

impl Shared {
    fn load_f32(bits: &AtomicU32) -> f32 {
        f32::from_bits(bits.load(Ordering::Relaxed))
    }
}

/// Per-channel gain for a balance setting: the far side attenuates, the near
/// side stays at unity — Winamp's behaviour, not a constant-power pan.
fn balance_gains(balance: f32) -> (f32, f32) {
    let b = balance.clamp(-1.0, 1.0);
    ((1.0 - b.max(0.0)), (1.0 + b.min(0.0)))
}

/// The realtime half: everything the cpal callback owns. A plain struct
/// rather than a closure so its behaviour can be tested without a device.
struct Output {
    consumer: rtrb::Consumer<f32>,
    tap: rtrb::Producer<f32>,
    eq_updates: rtrb::Consumer<EqParams>,
    eq: Equalizer,
    shared: Arc<Shared>,
    channels: usize,
    /// Frames into the fade-in after a flush; `DECLICK_FRAMES` when done.
    fade_in: usize,
}

impl Output {
    /// Fill one device buffer. Realtime: no allocation, locks or syscalls.
    fn render(&mut self, out: &mut [f32]) {
        // Only the newest settings matter.
        while let Ok(params) = self.eq_updates.pop() {
            self.eq.set(params);
        }
        let playing =
            Transport::from_u8(self.shared.transport.load(Ordering::Relaxed)) == Transport::Playing;

        // Serviced before the transport check: a seek while paused still has
        // to empty the ring.
        if self.shared.flush.load(Ordering::Acquire) {
            // Fade out what is already playing rather than cut it mid-cycle,
            // then drop the rest.
            let faded = if playing { self.fade_out(out) } else { 0 };
            out[faded * self.channels..].fill(0.0);
            if let Ok(chunk) = self.consumer.read_chunk(self.consumer.slots()) {
                chunk.commit_all();
            }
            self.eq.reset();
            self.fade_in = 0;
            self.shared.flush.store(false, Ordering::Release);
            return;
        }

        if !playing {
            out.fill(0.0);
            return;
        }

        let mut frames = 0u64;
        for frame in out.chunks_exact_mut(self.channels) {
            // Whole frames only, so an underrun can never swap left and right.
            if self.consumer.slots() < self.channels {
                frame.fill(0.0);
                continue;
            }
            let ramp = if self.fade_in < DECLICK_FRAMES {
                self.fade_in += 1;
                self.fade_in as f32 / DECLICK_FRAMES as f32
            } else {
                1.0
            };
            self.next_frame(frame, ramp);
            frames += 1;
        }
        self.shared
            .frames_played
            .fetch_add(frames, Ordering::Relaxed);
    }

    /// Play up to `DECLICK_FRAMES` of the ring into the front of `out`,
    /// ramping to silence. Returns the frames written.
    fn fade_out(&mut self, out: &mut [f32]) -> usize {
        let available = self.consumer.slots() / self.channels;
        let n = (out.len() / self.channels)
            .min(DECLICK_FRAMES)
            .min(available);
        for (i, frame) in out.chunks_exact_mut(self.channels).take(n).enumerate() {
            self.next_frame(frame, 1.0 - (i + 1) as f32 / n as f32);
        }
        n
    }

    /// Pop, equalise, tap, and apply volume, balance and `gain` to one frame.
    fn next_frame(&mut self, frame: &mut [f32], gain: f32) {
        let volume = Shared::load_f32(&self.shared.volume_bits) * gain;
        let (left, right) = balance_gains(Shared::load_f32(&self.shared.balance_bits));
        let channels = self.channels;
        let mut mono = 0.0;
        for (c, slot) in frame.iter_mut().enumerate() {
            // EQ (with its preamp), then volume and balance.
            let s = self.eq.process(c, self.consumer.pop().unwrap_or(0.0));
            mono += s;
            let pan = match c {
                0 if channels > 1 => left,
                1 => right,
                _ => 1.0,
            };
            // Boost can exceed full scale; clip rather than hand the device
            // out-of-range samples.
            *slot = (s * volume * pan).clamp(-1.0, 1.0);
        }
        // Post-EQ but pre-volume, so the analyser shows the EQ's effect
        // without shrinking with the volume slider. A full tap drops samples.
        let _ = self.tap.push(mono / channels as f32);
    }
}

/// Owns the decoder thread and the cpal stream.
pub struct Engine {
    /// Dropping this stops the audio callback, so it must outlive the engine.
    _stream: cpal::Stream,
    cmd_tx: Sender<Command>,
    reports: Receiver<Report>,
    timeline: Timeline,
    tap: Option<rtrb::Consumer<f32>>,
    /// EQ coefficients to the callback. The mutex is only ever taken on the
    /// UI thread — the callback holds the consumer end outright.
    eq_tx: Mutex<rtrb::Producer<EqParams>>,
    shared: Arc<Shared>,
}

impl Engine {
    /// Open the default output device and start the decoder thread.
    ///
    /// The stream is built once, at the device's preferred rate, and lives for
    /// the process. Rate conversion is the decoder thread's job — rebuilding
    /// the stream per file would make gapless across differing rates
    /// impossible.
    pub fn new() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoOutputDevice)?;
        let config = device
            .default_output_config()
            .map_err(|e| AudioError::Stream(e.to_string()))?;

        let output_rate = config.sample_rate();
        let channels = config.channels() as usize;

        let shared = Arc::new(Shared {
            transport: AtomicU8::new(Transport::Stopped.as_u8()),
            frames_played: AtomicU64::new(0),
            volume_bits: AtomicU32::new(1.0f32.to_bits()),
            balance_bits: AtomicU32::new(0.0f32.to_bits()),
            output_rate: AtomicU32::new(output_rate),
            flush: AtomicBool::new(false),
        });

        // Two seconds of slack. Large enough to ride out a slow disk read,
        // small enough that a seek does not have to discard much.
        let capacity = output_rate as usize * channels * 2;
        let (producer, consumer) = rtrb::RingBuffer::<f32>::new(capacity);
        let (tap_tx, tap_rx) = rtrb::RingBuffer::<f32>::new(TAP_CAPACITY);
        let (eq_tx, eq_rx) = rtrb::RingBuffer::<EqParams>::new(EQ_QUEUE);

        let (cmd_tx, cmd_rx) = channel::<Command>();
        let (report_tx, reports) = channel::<Report>();

        let mut output = Output {
            consumer,
            tap: tap_tx,
            eq_updates: eq_rx,
            eq: Equalizer::default(),
            shared: Arc::clone(&shared),
            channels,
            fade_in: DECLICK_FRAMES,
        };
        let err_fn = |e| eprintln!("audio output error: {e}");

        // The realtime callback. Nothing in here may allocate or block.
        let stream = device
            .build_output_stream(
                config.config(),
                move |out: &mut [f32], _: &cpal::OutputCallbackInfo| output.render(out),
                err_fn,
                None,
            )
            .map_err(|e| AudioError::Stream(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioError::Stream(e.to_string()))?;

        let dec_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("lantern-decoder".into())
            .spawn(move || {
                Decoder {
                    producer,
                    capacity,
                    output_rate,
                    out_channels: channels,
                    shared: dec_shared,
                    reports: report_tx,
                    next: None,
                }
                .run(cmd_rx);
            })
            .map_err(|e| AudioError::Stream(e.to_string()))?;

        Ok(Self {
            _stream: stream,
            cmd_tx,
            reports,
            timeline: Timeline::default(),
            tap: Some(tap_rx),
            eq_tx: Mutex::new(eq_tx),
            shared,
        })
    }

    pub fn send(&self, command: Command) -> Result<(), AudioError> {
        // Volume, balance and EQ go straight to the callback, skipping the
        // decoder.
        match command {
            Command::SetVolume(v) => {
                let v = v.clamp(0.0, 1.0);
                self.shared
                    .volume_bits
                    .store(v.to_bits(), Ordering::Relaxed);
                return Ok(());
            }
            Command::SetBalance(b) => {
                let b = b.clamp(-1.0, 1.0);
                self.shared
                    .balance_bits
                    .store(b.to_bits(), Ordering::Relaxed);
                return Ok(());
            }
            Command::SetEq(settings) => {
                let params = EqParams::new(&settings, self.output_rate());
                let mut tx = self
                    .eq_tx
                    .lock()
                    .map_err(|_| AudioError::Stream("EQ queue poisoned".into()))?;
                // Full only if the callback has stalled; the next change
                // gets through once it resumes.
                return tx
                    .push(params)
                    .map_err(|_| AudioError::Stream("EQ queue full".into()));
            }
            Command::Play | Command::Pause | Command::Stop => {
                let t = match command {
                    Command::Play => Transport::Playing,
                    Command::Pause => Transport::Paused,
                    _ => Transport::Stopped,
                };
                self.shared.transport.store(t.as_u8(), Ordering::Relaxed);
            }
            _ => {}
        }
        self.cmd_tx
            .send(command)
            .map_err(|_| AudioError::Stream("decoder thread has stopped".into()))
    }

    /// The next pending event, if any. Never blocks. Call regularly while
    /// playing: gapless crossings are detected here.
    pub fn poll_event(&mut self) -> Option<Event> {
        while let Ok(report) = self.reports.try_recv() {
            self.timeline.absorb(report);
        }
        self.timeline.next_event(self.frames_played())
    }

    /// The analyser's sample feed: mono, at [`Engine::output_rate`], only
    /// while playing. Can be taken once.
    pub fn take_tap(&mut self) -> Option<rtrb::Consumer<f32>> {
        self.tap.take()
    }

    /// Frames delivered to the device so far. Never blocks.
    pub fn frames_played(&self) -> u64 {
        self.shared.frames_played.load(Ordering::Relaxed)
    }

    pub fn transport(&self) -> Transport {
        Transport::from_u8(self.shared.transport.load(Ordering::Relaxed))
    }

    pub fn output_rate(&self) -> u32 {
        self.shared.output_rate.load(Ordering::Relaxed)
    }

    /// Elapsed position in the current track, in seconds.
    pub fn position_secs(&self) -> f64 {
        let frames = self.timeline.track_frames(self.frames_played());
        frames as f64 / self.output_rate().max(1) as f64
    }
}

/// Turns decoder reports into UI events, holding each gapless crossing back
/// until the callback's frame counter actually reaches it — the decoder runs
/// up to two seconds ahead of what is audible.
#[derive(Default)]
struct Timeline {
    /// Seams the decoder has passed but the audio has not: `(frame, track)`,
    /// in `frames_played` terms.
    seams: std::collections::VecDeque<(u64, TrackInfo)>,
    ready: std::collections::VecDeque<Event>,
    /// `frames_played` where the current track began; the counter itself
    /// runs on across a gapless seam.
    track_origin: u64,
}

impl Timeline {
    fn absorb(&mut self, report: Report) {
        match report {
            Report::Opened(info) => {
                self.seams.clear();
                self.track_origin = 0;
                self.ready.push_back(Event::Opened(info));
            }
            Report::Seeked => {
                self.seams.clear();
                self.track_origin = 0;
            }
            Report::Queued { at_frame, info } => self.seams.push_back((at_frame, info)),
            Report::OpenFailed { path, error } => {
                self.ready.push_back(Event::OpenFailed { path, error });
            }
            Report::Ended => {
                // The ring has drained, so every seam has been crossed.
                while let Some((at, info)) = self.seams.pop_front() {
                    self.cross(at, info);
                }
                self.ready.push_back(Event::Ended);
            }
        }
    }

    fn next_event(&mut self, played: u64) -> Option<Event> {
        while let Some((at, _)) = self.seams.front()
            && played >= *at
        {
            if let Some((at, info)) = self.seams.pop_front() {
                self.cross(at, info);
            }
        }
        self.ready.pop_front()
    }

    fn cross(&mut self, at: u64, info: TrackInfo) {
        self.track_origin = at;
        self.ready.push_back(Event::Advanced(info));
    }

    /// Frames into the current track.
    fn track_frames(&self, played: u64) -> u64 {
        played.saturating_sub(self.track_origin)
    }
}

// ---------------------------------------------------------------------------
// Decoder thread
// ---------------------------------------------------------------------------

/// What the decoder tells the engine. The engine turns these into
/// [`Event`]s, timing gapless crossings against the callback's progress.
enum Report {
    Opened(TrackInfo),
    OpenFailed {
        path: PathBuf,
        error: String,
    },
    /// The decoder has moved on to the queued track. Its first frame will
    /// reach the device when `frames_played` reaches `at_frame`.
    Queued {
        at_frame: u64,
        info: TrackInfo,
    },
    /// The frame counter was moved; any pending seam is void.
    Seeked,
    Ended,
}

/// A file being decoded, plus the conversion state that must survive packets.
struct Active {
    reader: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::audio::AudioDecoder>,
    track_id: u32,
    time_base: Option<symphonia::core::units::TimeBase>,
    src_rate: u32,
    src_channels: usize,
    resampler: Resampler,
    /// What is playing, and the file's facts to describe it with.
    source: Source,
    info: TrackInfo,
    /// Length of the whole file, for a segment that plays to the end.
    file_secs: Option<f64>,
    /// Source frame the next decoded frame will be, from the file's start.
    pos: u64,
    /// Decoded frames before this are discarded: the segment start, or the
    /// exact seek target when the reader landed a little early.
    skip_until: u64,
    /// Decoding stops here, for a segment with an end.
    end_frame: Option<u64>,
    /// Interleaved output samples not yet accepted by the ring.
    pending: std::collections::VecDeque<f32>,
    /// The reader is exhausted and the resampler flushed; once `pending`
    /// empties, the track is done.
    ended: bool,
}

impl Active {
    fn frame_at(&self, secs: f64) -> u64 {
        (secs.max(0.0) * f64::from(self.src_rate)).round() as u64
    }

    /// Point at `source` within the already-open file: describe it and set
    /// its end. Does not move the reader.
    fn adopt(&mut self, source: Source) {
        self.end_frame = source.end_secs.map(|e| self.frame_at(e));
        self.info.start_secs = source.start_secs;
        self.info.end_secs = source.end_secs;
        self.info.duration_secs = source
            .end_secs
            .or(self.file_secs)
            .map(|end| (end - source.start_secs).max(0.0));
        self.source = source;
    }

    /// Move the reader to `secs` into the file, sample-accurately: symphonia
    /// lands at or before the target, and the difference is decoded and
    /// discarded. Returns false if the format cannot seek.
    fn seek_to(&mut self, secs: f64) -> bool {
        use symphonia::core::formats::{SeekMode, SeekTo};
        use symphonia::core::units::Time;

        let Some(time) = Time::try_from_secs_f64(secs.max(0.0)) else {
            return false;
        };
        let to = match self.reader.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time,
                track_id: Some(self.track_id),
            },
        ) {
            Ok(to) => to,
            Err(e) => {
                eprintln!("seek failed: {e}");
                return false;
            }
        };
        let landed = self
            .time_base
            .and_then(|tb| tb.calc_time(to.actual_ts))
            .map_or(secs, |t| t.as_secs_f64());
        self.decoder.reset();
        self.resampler.reset();
        self.pending.clear();
        self.ended = false;
        self.pos = self.frame_at(landed);
        self.skip_until = self.frame_at(secs);
        true
    }

    /// No more input: flush the resampler's tail into `pending`.
    fn end(&mut self, scratch: &mut Vec<f32>) {
        scratch.clear();
        self.resampler.finish(scratch);
        self.pending.extend(scratch.iter().copied());
        self.ended = true;
    }
}

struct Decoder {
    producer: rtrb::Producer<f32>,
    capacity: usize,
    output_rate: u32,
    out_channels: usize,
    shared: Arc<Shared>,
    reports: Sender<Report>,
    /// The queued track, opened when the current one runs dry.
    next: Option<Source>,
}

impl Decoder {
    fn run(mut self, cmd_rx: Receiver<Command>) {
        let mut active: Option<Active> = None;
        // End of file reached; waiting for the callback to play out the ring
        // before reporting the track finished.
        let mut draining = false;
        // Reused across packets so the steady state does not allocate.
        let mut decoded: Vec<f32> = Vec::new();
        let mut converted: Vec<f32> = Vec::new();
        let mut resampled: Vec<f32> = Vec::new();

        loop {
            // Block only when there is nothing to do; otherwise poll so
            // decoding continues.
            let cmd = if active.is_some() || draining {
                match cmd_rx.try_recv() {
                    Ok(c) => Some(c),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => return,
                }
            } else {
                match cmd_rx.recv() {
                    Ok(c) => Some(c),
                    Err(_) => return,
                }
            };

            if let Some(cmd) = cmd {
                match cmd {
                    Command::Open(source) => {
                        self.reset_position(0);
                        self.next = None;
                        draining = false;
                        active = match open_track(&source, self.output_rate, self.out_channels) {
                            Ok(a) => {
                                let _ = self.reports.send(Report::Opened(a.info.clone()));
                                Some(a)
                            }
                            Err(e) => {
                                self.report_failure(source.path, &e);
                                None
                            }
                        };
                    }
                    Command::Stop => {
                        self.reset_position(0);
                        self.next = None;
                        active = None;
                        draining = false;
                    }
                    Command::Seek { seconds } => {
                        if let Some(a) = active.as_mut() {
                            self.seek(a, seconds);
                        }
                    }
                    Command::Enqueue(source) => self.next = Some(source),
                    Command::ClearQueue => self.next = None,
                    // Handled on the UI side or by the callback directly.
                    Command::Play
                    | Command::Pause
                    | Command::SetVolume(_)
                    | Command::SetBalance(_)
                    | Command::SetEq(_) => {}
                }
            }

            if draining {
                // A track queued late — after the last one ran dry but while
                // its tail is still playing — still joins without a gap.
                if let Some(a) = self.open_next(None, &mut resampled) {
                    active = Some(a);
                    draining = false;
                } else if self.producer.slots() == self.capacity {
                    draining = false;
                    self.shared
                        .transport
                        .store(Transport::Stopped.as_u8(), Ordering::Relaxed);
                    let _ = self.reports.send(Report::Ended);
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                }
                continue;
            }

            let Some(a) = active.as_mut() else { continue };

            // Drain whatever the ring will take before decoding more.
            while let Some(&s) = a.pending.front() {
                if self.producer.push(s).is_err() {
                    break;
                }
                a.pending.pop_front();
            }
            if !a.pending.is_empty() {
                // Ring is full; let the callback catch up rather than spin.
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            if a.ended {
                active = None;
                draining = true;
                continue;
            }

            let exhausted = match a.reader.next_packet() {
                Ok(Some(packet)) => {
                    if packet.track_id != a.track_id {
                        continue;
                    }
                    match a.decoder.decode(&packet) {
                        Ok(buf) => {
                            decoded.clear();
                            buf.copy_to_vec_interleaved(&mut decoded);
                            let ch = a.src_channels.max(1);
                            let frames = (decoded.len() / ch) as u64;
                            let first = a.pos;
                            a.pos += frames;
                            // A segment ending inside this packet may run
                            // straight on into the queued one.
                            if a.end_frame.is_some_and(|e| a.pos >= e) {
                                self.continue_segment(a, first);
                            }
                            // Keep only frames inside [skip_until, end).
                            let lo = a.skip_until.saturating_sub(first).min(frames) as usize;
                            let hi = a
                                .end_frame
                                .map_or(frames, |e| e.saturating_sub(first).min(frames))
                                as usize;
                            if hi > lo {
                                converted.clear();
                                map_channels(
                                    &decoded[lo * ch..hi * ch],
                                    ch,
                                    self.out_channels,
                                    &mut converted,
                                );
                                resampled.clear();
                                a.resampler.process(&converted, &mut resampled);
                                a.pending.extend(resampled.iter().copied());
                            }
                            a.end_frame.is_some_and(|e| a.pos >= e)
                        }
                        // A bad packet mid-file is common in the wild; skip it.
                        Err(symphonia::core::errors::Error::DecodeError(_)) => false,
                        Err(e) => {
                            eprintln!("decode failed: {e}");
                            true
                        }
                    }
                }
                Ok(None) => true,
                Err(e) => {
                    eprintln!("read failed: {e}");
                    true
                }
            };
            if exhausted {
                match self.open_next(Some(&mut *a), &mut resampled) {
                    Some(next) => active = Some(next),
                    None => a.end(&mut resampled),
                }
            }
        }
    }

    /// The current segment is about to end inside the packet starting at
    /// source frame `first`. If the queued source continues it in the same
    /// file (consecutive cue tracks), adopt it without reopening anything —
    /// the decoder, resampler and reader just keep going — and report the
    /// seam.
    fn continue_segment(&mut self, a: &mut Active, first: u64) {
        let Some(next) = self.next.take_if(|n| a.source.continues_into(n)) else {
            return;
        };
        let seam = a.end_frame.unwrap_or(a.pos);
        // Output frames still to come before the seam: in the ring, in
        // `pending`, inside the resampler, and in this packet.
        let ratio = f64::from(self.output_rate) / f64::from(a.src_rate);
        let in_packet =
            (seam.saturating_sub(first.max(a.skip_until)) as f64 * ratio).round() as u64;
        let at_frame = self.shared.frames_played.load(Ordering::Relaxed)
            + self.unplayed_frames()
            + a.pending.len() as u64 / self.out_channels as u64
            + a.resampler.latency_frames()
            + in_packet;
        a.adopt(next);
        let _ = self.reports.send(Report::Queued {
            at_frame,
            info: a.info.clone(),
        });
    }

    /// Open the queued source, if any, to continue from `previous` without a
    /// gap. `previous` is the track that just ran dry, or `None` if it has
    /// already been flushed into the ring.
    ///
    /// When both tracks share a rate and channel count — an album, nearly
    /// always — the resampler carries straight over, so there is no filter
    /// restart at the seam either. Otherwise the old resampler's tail is
    /// flushed first, which is as seamless as a format change allows.
    fn open_next(
        &mut self,
        previous: Option<&mut Active>,
        scratch: &mut Vec<f32>,
    ) -> Option<Active> {
        let source = self.next.take()?;
        let mut next = match open_track(&source, self.output_rate, self.out_channels) {
            Ok(opened) => opened,
            Err(e) => {
                self.report_failure(source.path, &e);
                return None;
            }
        };
        let ch = self.out_channels as u64;
        // Frames of the old track still ahead of the seam, beyond the ring.
        let mut in_flight = 0;
        if let Some(prev) = previous {
            if prev.src_rate == next.src_rate && prev.src_channels == next.src_channels {
                std::mem::swap(&mut next.resampler, &mut prev.resampler);
                in_flight += next.resampler.latency_frames();
            } else {
                prev.end(scratch);
            }
            in_flight += prev.pending.len() as u64 / ch;
            next.pending = std::mem::take(&mut prev.pending);
        }
        let at_frame =
            self.shared.frames_played.load(Ordering::Relaxed) + self.unplayed_frames() + in_flight;
        let _ = self.reports.send(Report::Queued {
            at_frame,
            info: next.info.clone(),
        });
        Some(next)
    }

    /// Frames written to the ring that the callback has not yet played.
    fn unplayed_frames(&self) -> u64 {
        (self.capacity - self.producer.slots()) as u64 / self.out_channels as u64
    }

    fn report_failure(&self, path: PathBuf, error: &AudioError) {
        let _ = self.reports.send(Report::OpenFailed {
            path,
            error: error.to_string(),
        });
    }

    /// Empty the ring and restart the frame counter at `frames`.
    fn reset_position(&mut self, frames: u64) {
        self.flush();
        self.shared.frames_played.store(frames, Ordering::Relaxed);
    }

    /// Ask the callback to discard the ring, and wait until it has.
    fn flush(&self) {
        self.shared.flush.store(true, Ordering::Release);
        let start = Instant::now();
        while self.shared.flush.load(Ordering::Acquire) && start.elapsed() < FLUSH_TIMEOUT {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Seek = flush the ring, seek the reader sample-accurately, reset
    /// decoder and resampler state, and set the frame counter to the target.
    /// `seconds` is from the start of the current track.
    fn seek(&mut self, a: &mut Active, seconds: f64) {
        let mut target = a.source.start_secs + seconds.max(0.0);
        if let Some(end) = a.source.end_secs {
            target = target.min(end);
        }
        self.flush();
        if !a.seek_to(target) {
            return;
        }
        let frames = ((target - a.source.start_secs) * f64::from(self.output_rate)) as u64;
        self.reset_position(frames);
        let _ = self.reports.send(Report::Seeked);
    }
}

fn open_track(
    source: &Source,
    output_rate: u32,
    out_channels: usize,
) -> Result<Active, AudioError> {
    let path = source.path.as_path();
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let file = std::fs::File::open(path).map_err(|e| AudioError::Decode(e.to_string()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let reader = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| AudioError::UnsupportedFormat(e.to_string()))?;

    let track = reader
        .default_track(TrackType::Audio)
        .ok_or_else(|| AudioError::UnsupportedFormat("no audio track".into()))?;
    let track_id = track.id;
    let time_base = track.time_base;
    let duration_secs = track
        .duration
        .zip(time_base)
        .and_then(|(d, tb)| tb.calc_duration(d))
        .map(|t| t.as_secs_f64());

    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| AudioError::UnsupportedFormat("no audio codec parameters".into()))?;

    let src_rate = params
        .sample_rate
        .ok_or_else(|| AudioError::UnsupportedFormat("unknown sample rate".into()))?;
    let src_channels = params.channels.as_ref().map_or(2, |c| c.count());

    // Gapless trimming removes encoder delay and padding. Default is already
    // true in symphonia 0.6; stated explicitly so the intent is not silently
    // lost if that default ever changes.
    let dec_opts = AudioDecoderOptions::default().gapless(true);
    let decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &dec_opts)
        .map_err(|e| AudioError::UnsupportedFormat(e.to_string()))?;

    let tags = crate::tags::read(path);
    let file_secs = duration_secs.or(tags.duration_secs);
    let info = TrackInfo {
        path: path.to_path_buf(),
        start_secs: 0.0,
        end_secs: None,
        title: tags.title.unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        }),
        sample_rate: src_rate,
        channels: src_channels as u16,
        kbps: tags.kbps,
        duration_secs: file_secs,
    };

    let mut active = Active {
        reader,
        decoder,
        track_id,
        time_base,
        src_rate,
        src_channels,
        resampler: Resampler::new(src_rate, output_rate, out_channels),
        source: source.clone(),
        info,
        file_secs,
        pos: 0,
        skip_until: 0,
        end_frame: None,
        pending: std::collections::VecDeque::new(),
        ended: false,
    };
    active.adopt(source.clone());
    if source.start_secs > 0.0 && !active.seek_to(source.start_secs) {
        return Err(AudioError::Decode(format!(
            "cannot seek to {:.2}s",
            source.start_secs
        )));
    }
    Ok(active)
}

/// Fit `src`'s channel count to the device's.
///
/// Mono fans out to every output channel; extra source channels are dropped.
/// A proper surround downmix needs the source's channel layout, and is
/// deferred until a multichannel file turns up in a real library (design doc,
/// risk 5).
fn map_channels(src: &[f32], src_ch: usize, dst_ch: usize, out: &mut Vec<f32>) {
    if src_ch == dst_ch {
        out.extend_from_slice(src);
        return;
    }
    if src_ch == 0 {
        return;
    }
    for frame in src.chunks_exact(src_ch) {
        for c in 0..dst_ch {
            out.push(if src_ch == 1 {
                frame[0]
            } else {
                frame[c % src_ch]
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 16-bit stereo PCM WAV of `frames` of a sine, starting at frame
    /// `offset` of the waveform.
    fn write_wav(path: &Path, rate: u32, hz: f32, offset: usize, frames: usize) {
        let data_len = (frames * 4) as u32;
        let mut b = Vec::with_capacity(44 + data_len as usize);
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes()); // PCM
        b.extend_from_slice(&2u16.to_le_bytes()); // channels
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 4).to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for n in offset..offset + frames {
            let s = (std::f32::consts::TAU * hz * n as f32 / rate as f32).sin() * 0.5;
            let v = (s * 32767.0) as i16;
            b.extend_from_slice(&v.to_le_bytes());
            b.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }

    /// Run a decoder thread with this test standing in for the audio
    /// callback: drain the ring, honour flushes, advance the frame counter.
    /// Returns everything played and every report, up to `Ended`.
    fn play_through(commands: Vec<Command>, output_rate: u32) -> (Vec<f32>, Vec<Report>) {
        let shared = Arc::new(Shared {
            transport: AtomicU8::new(Transport::Playing.as_u8()),
            frames_played: AtomicU64::new(0),
            volume_bits: AtomicU32::new(1.0f32.to_bits()),
            balance_bits: AtomicU32::new(0.0f32.to_bits()),
            output_rate: AtomicU32::new(output_rate),
            flush: AtomicBool::new(false),
        });
        let capacity = output_rate as usize * 2;
        let (producer, mut consumer) = rtrb::RingBuffer::<f32>::new(capacity);
        let (cmd_tx, cmd_rx) = channel();
        let (report_tx, reports) = channel();
        let decoder = Decoder {
            producer,
            capacity,
            output_rate,
            out_channels: 2,
            shared: Arc::clone(&shared),
            reports: report_tx,
            next: None,
        };
        std::thread::spawn(move || decoder.run(cmd_rx));
        for c in commands {
            cmd_tx.send(c).unwrap();
        }

        let (mut out, mut seen) = (Vec::new(), Vec::new());
        let deadline = Instant::now() + Duration::from_secs(20);
        'run: while Instant::now() < deadline {
            if shared.flush.load(Ordering::Acquire) {
                while consumer.pop().is_ok() {}
                shared.flush.store(false, Ordering::Release);
            }
            // Count frame by frame, so the decoder never sees samples gone
            // from the ring but not yet counted as played.
            while consumer.slots() >= 2 {
                out.push(consumer.pop().unwrap());
                out.push(consumer.pop().unwrap());
                shared.frames_played.fetch_add(1, Ordering::Relaxed);
            }
            while let Ok(r) = reports.try_recv() {
                let ended = matches!(r, Report::Ended);
                seen.push(r);
                if ended {
                    break 'run;
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            matches!(seen.last(), Some(Report::Ended)),
            "decoder never reported the end"
        );
        (out, seen)
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lantern-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn gapless_seam_is_sample_continuous() {
        // One continuous sine, cut into two files at an arbitrary frame, then
        // played back to back through a 44.1k -> 48k conversion. A gap, a
        // click or a filter restart at the seam shows as a jump.
        let dir = temp_dir("gapless");
        let (a, b) = (dir.join("a.wav"), dir.join("b.wav"));
        let (split, total) = (32_157, 88_200);
        write_wav(&a, 44_100, 440.0, 0, split);
        write_wav(&b, 44_100, 440.0, split, total - split);

        let (out, reports) = play_through(
            vec![
                Command::Open(a.clone().into()),
                Command::Enqueue(b.clone().into()),
            ],
            48_000,
        );
        std::fs::remove_dir_all(&dir).ok();

        let expected = (total as f64 * 48_000.0 / 44_100.0).round() as i64;
        let frames = out.len() as i64 / 2;
        assert!(
            (frames - expected).abs() <= 2,
            "{frames} frames, expected {expected}"
        );

        // A 440 Hz sine at 0.5 moves at most 0.029 per 48 kHz sample.
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let (worst, at) = left
            .windows(2)
            .enumerate()
            .map(|(i, w)| ((w[1] - w[0]).abs(), i))
            .fold((0.0f32, 0), |m, x| if x.0 > m.0 { x } else { m });
        assert!(worst < 0.035, "discontinuity {worst:.3} at frame {at}");

        let seam = reports.iter().find_map(|r| match r {
            Report::Queued { at_frame, info } => Some((*at_frame, info.path.clone())),
            _ => None,
        });
        let (at_frame, path) = seam.expect("no seam reported");
        assert_eq!(path, b);
        let true_seam = (split as f64 * 48_000.0 / 44_100.0) as i64;
        assert!(
            (at_frame as i64 - true_seam).abs() <= 2,
            "seam placed at {at_frame}, audio crosses at {true_seam}"
        );
    }

    fn info(name: &str) -> TrackInfo {
        TrackInfo {
            path: PathBuf::from(name),
            start_secs: 0.0,
            end_secs: None,
            title: name.into(),
            sample_rate: 44_100,
            channels: 2,
            kbps: None,
            duration_secs: None,
        }
    }

    /// An `Output` over a ring of `samples`, as the callback would own it.
    fn output(samples: &[f32], transport: Transport) -> (Output, rtrb::Producer<f32>) {
        let (mut producer, consumer) = rtrb::RingBuffer::new(16_384);
        for &s in samples {
            producer.push(s).unwrap();
        }
        let (tap, _) = rtrb::RingBuffer::new(16_384);
        let (_, eq_updates) = rtrb::RingBuffer::new(4);
        let shared = Arc::new(Shared {
            transport: AtomicU8::new(transport.as_u8()),
            frames_played: AtomicU64::new(0),
            volume_bits: AtomicU32::new(1.0f32.to_bits()),
            balance_bits: AtomicU32::new(0.0f32.to_bits()),
            output_rate: AtomicU32::new(48_000),
            flush: AtomicBool::new(false),
        });
        let out = Output {
            consumer,
            tap,
            eq_updates,
            eq: Equalizer::default(),
            shared,
            channels: 2,
            fade_in: DECLICK_FRAMES,
        };
        (out, producer)
    }

    /// Largest jump between consecutive left-channel samples.
    fn worst_step(samples: &[f32]) -> f32 {
        let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
        left.windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max)
    }

    #[test]
    fn a_flush_fades_out_then_in_instead_of_clicking() {
        // Full-scale DC is the worst case: a hard cut is a full-scale step.
        let (mut out, mut producer) = output(&[1.0; 4_096], Transport::Playing);
        let mut buf = vec![0.0; 1_024];
        out.render(&mut buf);
        assert_eq!(buf[0], 1.0);

        out.shared.flush.store(true, Ordering::Release);
        let mut flushed = vec![0.0; 1_024];
        out.render(&mut flushed);
        assert!(
            !out.shared.flush.load(Ordering::Acquire),
            "flush acknowledged"
        );
        assert_eq!(out.consumer.slots(), 0, "ring emptied");

        for _ in 0..2_048 {
            producer.push(-1.0).unwrap();
        }
        let mut after = vec![0.0; 1_024];
        out.render(&mut after);

        let joined: Vec<f32> = [buf, flushed, after].concat();
        let step = worst_step(&joined);
        assert!(step < 0.01, "largest step {step}: a click");
        assert_eq!(
            *joined.last().unwrap(),
            -1.0,
            "new audio reaches full level"
        );
    }

    #[test]
    fn a_flush_while_paused_just_empties_the_ring() {
        let (mut out, _producer) = output(&[1.0; 512], Transport::Paused);
        out.shared.flush.store(true, Ordering::Release);
        let mut buf = vec![0.5; 256];
        out.render(&mut buf);
        assert!(buf.iter().all(|&s| s == 0.0));
        assert_eq!(out.consumer.slots(), 0);
    }

    #[test]
    fn underrun_outputs_silence_and_counts_no_frames() {
        let (mut out, _producer) = output(&[0.5; 3], Transport::Playing);
        let mut buf = vec![9.0; 8];
        out.render(&mut buf);
        assert_eq!(&buf[..2], &[0.5, 0.5]);
        assert!(buf[2..].iter().all(|&s| s == 0.0), "{buf:?}");
        assert_eq!(out.shared.frames_played.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn timeline_holds_a_crossing_until_the_audio_reaches_it() {
        let mut t = Timeline::default();
        t.absorb(Report::Opened(info("a")));
        assert!(matches!(t.next_event(0), Some(Event::Opened(_))));
        t.absorb(Report::Queued {
            at_frame: 1_000,
            info: info("b"),
        });
        assert!(t.next_event(999).is_none(), "decoder is ahead of the audio");
        assert_eq!(t.track_frames(999), 999);
        match t.next_event(1_000) {
            Some(Event::Advanced(i)) => assert_eq!(i.title, "b"),
            other => panic!("expected Advanced, got {other:?}"),
        }
        assert_eq!(t.track_frames(1_480), 480, "position restarts at the seam");
    }

    #[test]
    fn timeline_seek_voids_a_pending_seam() {
        let mut t = Timeline::default();
        t.absorb(Report::Queued {
            at_frame: 1_000,
            info: info("b"),
        });
        t.absorb(Report::Seeked);
        assert!(t.next_event(5_000).is_none());
        assert_eq!(t.track_frames(5_000), 5_000);
    }

    #[test]
    fn timeline_end_flushes_crossings_first() {
        let mut t = Timeline::default();
        t.absorb(Report::Queued {
            at_frame: 1_000,
            info: info("b"),
        });
        t.absorb(Report::Ended);
        assert!(matches!(t.next_event(0), Some(Event::Advanced(_))));
        assert!(matches!(t.next_event(0), Some(Event::Ended)));
    }

    fn segment(path: &Path, start: f64, end: Option<f64>) -> Source {
        Source {
            path: path.to_path_buf(),
            start_secs: start,
            end_secs: end,
        }
    }

    /// The sample a 16-bit 440 Hz test sine has at `frame`, as written.
    fn sine_at(frame: usize, rate: u32) -> f32 {
        let s = (std::f32::consts::TAU * 440.0 * frame as f32 / rate as f32).sin() * 0.5;
        f32::from((s * 32767.0) as i16) / 32768.0
    }

    #[test]
    fn a_segment_plays_exactly_its_frames() {
        let dir = temp_dir("segment");
        let a = dir.join("a.wav");
        write_wav(&a, 48_000, 440.0, 0, 48_000);
        let (out, reports) =
            play_through(vec![Command::Open(segment(&a, 0.25, Some(0.5)))], 48_000);
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(out.len() / 2, 12_000, "a quarter second, to the frame");
        // Sample-accurate start: the first frame is the file's frame 12000.
        assert!((out[0] - sine_at(12_000, 48_000)).abs() < 1e-3);
        assert!((out[out.len() - 2] - sine_at(23_999, 48_000)).abs() < 1e-3);
        let Some(Report::Opened(info)) = reports.first() else {
            panic!("no Opened report");
        };
        assert_eq!(info.duration_secs, Some(0.25), "duration is the segment's");
    }

    #[test]
    fn consecutive_cue_tracks_continue_without_reopening() {
        let dir = temp_dir("cue");
        let a = dir.join("a.wav");
        write_wav(&a, 44_100, 440.0, 0, 44_100);
        let (out, reports) = play_through(
            vec![
                Command::Open(segment(&a, 0.0, Some(0.4))),
                Command::Enqueue(segment(&a, 0.4, None)),
            ],
            48_000,
        );
        std::fs::remove_dir_all(&dir).ok();

        let frames = out.len() as i64 / 2;
        assert!((frames - 48_000).abs() <= 2, "{frames} frames");
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let worst = left
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        assert!(worst < 0.035, "discontinuity {worst}");
        let seam = reports.iter().find_map(|r| match r {
            Report::Queued { at_frame, info } => Some((*at_frame, info.clone())),
            _ => None,
        });
        let (at_frame, info) = seam.expect("no seam reported");
        assert!((at_frame as i64 - 19_200).abs() <= 2, "seam at {at_frame}");
        assert_eq!(info.start_secs, 0.4);
        assert!((info.duration_secs.unwrap() - 0.6).abs() < 1e-6);
    }

    #[test]
    fn a_rate_change_at_the_seam_still_plays_everything() {
        let dir = temp_dir("ratechange");
        let (a, b) = (dir.join("a.wav"), dir.join("b.wav"));
        write_wav(&a, 44_100, 440.0, 0, 22_050);
        write_wav(&b, 48_000, 440.0, 0, 24_000);
        let (out, _) = play_through(
            vec![Command::Open(a.into()), Command::Enqueue(b.into())],
            48_000,
        );
        std::fs::remove_dir_all(&dir).ok();
        // Half a second each, whatever the source rates.
        let frames = out.len() as i64 / 2;
        assert!((frames - 48_000).abs() <= 2, "{frames} frames");
    }

    #[test]
    fn without_a_queued_track_playback_ends() {
        let dir = temp_dir("single");
        let a = dir.join("a.wav");
        write_wav(&a, 48_000, 440.0, 0, 4_800);
        let (out, reports) = play_through(vec![Command::Open(a.into())], 48_000);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(out.len(), 4_800 * 2, "passthrough plays exactly the file");
        assert!(!reports.iter().any(|r| matches!(r, Report::Queued { .. })));
    }

    #[test]
    fn mono_fans_out_to_stereo() {
        let mut out = Vec::new();
        map_channels(&[0.5, -0.5], 1, 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5, -0.5, -0.5]);
    }

    #[test]
    fn matching_channel_counts_pass_through() {
        let mut out = Vec::new();
        map_channels(&[0.1, 0.2], 2, 2, &mut out);
        assert_eq!(out, vec![0.1, 0.2]);
    }

    #[test]
    fn balance_attenuates_only_the_far_side() {
        assert_eq!(balance_gains(0.0), (1.0, 1.0));
        assert_eq!(balance_gains(1.0), (0.0, 1.0));
        assert_eq!(balance_gains(-1.0), (1.0, 0.0));
        assert_eq!(balance_gains(0.5), (0.5, 1.0));
        assert_eq!(balance_gains(9.0), (0.0, 1.0), "clamps");
    }
}
