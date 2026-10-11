//! The main window's spectrum analyser.
//!
//! Runs on the **UI thread**, not in the audio callback. The callback only
//! copies a mono mix into a lock-free tap ring; the UI calls
//! [`Analyzer::update`] once per frame, which drains the tap, transforms the
//! most recent window and advances the bar physics. Doing this in the
//! callback would be both a realtime hazard and ~700x more transforms than
//! 60fps needs.
//!
//! The levels and look follow Webamp's `VisPainter.ts` and `FFTNullsoft.ts`
//! (MIT, Copyright (c) 2015 Jordan Eldredge), which model Winamp's own
//! analyser: magnitudes on a **linear** scale, tilted by a log10 "equalize"
//! ramp that quietens the bass, sampled at 75 points along a
//! mostly-logarithmic axis and averaged in fours into 19 "wide" bars (3px
//! with a 1px gap), clipped at 15px; "moderate" bar falloff and "slow" peak
//! falloff. Linear levels are what make Winamp's bars leap and drop to
//! nothing — an earlier dB scale here squeezed every bar to a similar height.
//! Only the FFT itself is ours (`realfft`); its output is scaled to match.

use std::sync::Arc;
use std::time::Instant;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

/// Samples per transform, giving 512 frequency bins.
pub const WINDOW_SIZE: usize = 1024;

/// Bars drawn, each 3px wide with a 1px gap: 19 * 4 = 76, the display's
/// width.
pub const BAR_COUNT: usize = 19;

/// The tallest a bar draws, in pixels: the display is 16px, and the top row
/// is left for the peak marker.
pub const HEIGHT: u8 = 15;

/// How far from linear towards logarithmic the frequency axis is. Webamp
/// settles on 0.91 to resemble later Winamp versions.
const LOG_BLEND: f32 = 0.91;

/// Points sampled along the frequency axis; each bar averages four.
const POINTS: usize = 75;

/// Winamp's input scale: a full-scale sample maps to 128/24.
const INPUT_GAIN: f32 = 128.0 / 24.0;

/// The physics runs at a fixed 60Hz step, whatever the redraw rate, so bars
/// fall at the same speed on a 30fps and a 120fps display.
const STEP_SECS: f32 = 1.0 / 60.0;

/// Pixels a bar may drop per step ("moderate": 12/16).
const BAR_FALL: f32 = 12.0 / 16.0;

/// A peak's initial fall speed in pixels per step, and its per-step
/// acceleration ("slow": 1.1x).
const PEAK_FALL_START: f32 = 3.0 / 256.0;
const PEAK_FALL_ACCEL: f32 = 1.1;

/// One bar as it should be drawn this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bar {
    /// Filled height in pixels, 0..=16, measured up from the bottom.
    pub height: u8,
    /// Height of the one-pixel peak marker, when there is one to draw.
    pub peak: Option<u8>,
}

/// Pulls samples from the tap ring and produces bar heights.
pub struct Analyzer {
    tap: rtrb::Consumer<f32>,
    /// The most recent `WINDOW_SIZE` samples, oldest first.
    history: Vec<f32>,
    fft: Arc<dyn RealToComplex<f32>>,
    input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    hann: Vec<f32>,
    /// Per-bin weights rising on a log10 curve from ~0 at DC to ~1.
    equalize: Vec<f32>,
    /// Current smoothed bar heights, in pixels.
    bars: [f32; BAR_COUNT],
    peaks: [f32; BAR_COUNT],
    peak_speed: [f32; BAR_COUNT],
    last_update: Option<Instant>,
    /// Unspent time carried between updates, below one physics step.
    pending_secs: f32,
}

impl Analyzer {
    pub fn new(tap: rtrb::Consumer<f32>) -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(WINDOW_SIZE);
        let hann = (0..WINDOW_SIZE)
            .map(|i| {
                let t = i as f32 / WINDOW_SIZE as f32;
                0.5 - 0.5 * (std::f32::consts::TAU * t).cos()
            })
            .collect();
        let bins = WINDOW_SIZE / 2;
        let mut bias = 0.04f32;
        let equalize = (0..bins)
            .map(|i| {
                let step = (9.0 - bias) / bins as f32;
                let weight = (1.0 + bias + (i + 1) as f32 * step).log10();
                bias /= 1.0025;
                weight
            })
            .collect();
        Self {
            tap,
            history: vec![0.0; WINDOW_SIZE],
            input: fft.make_input_vec(),
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            hann,
            equalize,
            bars: [0.0; BAR_COUNT],
            peaks: [0.0; BAR_COUNT],
            peak_speed: [0.0; BAR_COUNT],
            last_update: None,
            pending_secs: 0.0,
        }
    }

    /// Drain the tap and advance the display to `now`. Call once per frame.
    ///
    /// When the tap has nothing new — paused, stopped, between tracks — the
    /// input is treated as silence, so the bars fall away rather than freeze.
    pub fn update(&mut self, now: Instant) {
        let fresh = self.drain_tap();
        if !fresh {
            self.history.fill(0.0);
        }
        let targets = self.targets();

        let elapsed = self
            .last_update
            .map_or(STEP_SECS, |t| now.duration_since(t).as_secs_f32());
        self.last_update = Some(now);
        // Cap catch-up so a long stall (window hidden) does not run hundreds
        // of steps at once.
        self.pending_secs = (self.pending_secs + elapsed).min(STEP_SECS * 10.0);
        while self.pending_secs >= STEP_SECS {
            self.pending_secs -= STEP_SECS;
            self.step(&targets);
        }
    }

    /// The bars to draw, left (bass) to right (treble).
    pub fn bars(&self) -> [Bar; BAR_COUNT] {
        std::array::from_fn(|i| {
            let height = self.bars[i].round().clamp(0.0, f32::from(HEIGHT)) as u8;
            let peak = self.peaks[i].round().clamp(0.0, f32::from(HEIGHT)) as u8;
            Bar {
                height,
                // A peak resting on the floor is not drawn.
                peak: (peak >= 1).then_some(peak),
            }
        })
    }

    /// Move everything in the tap into `history`. True if anything arrived.
    fn drain_tap(&mut self) -> bool {
        let available = self.tap.slots();
        if available == 0 {
            return false;
        }
        let Ok(chunk) = self.tap.read_chunk(available) else {
            return false;
        };
        // Only the newest WINDOW_SIZE samples matter.
        let take = available.min(WINDOW_SIZE);
        self.history.rotate_left(take);
        let (a, b) = chunk.as_slices();
        let newest = a.iter().chain(b).skip(available - take);
        for (dst, &s) in self.history[WINDOW_SIZE - take..].iter_mut().zip(newest) {
            *dst = s;
        }
        chunk.commit_all();
        true
    }

    /// This frame's target height for every bar, from a fresh transform.
    fn targets(&mut self) -> [f32; BAR_COUNT] {
        for ((dst, &s), &w) in self.input.iter_mut().zip(&self.history).zip(&self.hann) {
            *dst = s * INPUT_GAIN * w;
        }
        if self
            .fft
            .process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch)
            .is_err()
        {
            return [0.0; BAR_COUNT];
        }
        let bins = WINDOW_SIZE / 2;
        let level = |i: usize| {
            let i = i.min(bins - 1);
            self.spectrum[i].norm() * self.equalize[i]
        };

        // The axis: each point blends a linear and a logarithmic position.
        let log_max = (bins as f32).log10();
        let position = |x: f32| {
            let t = x / (POINTS - 1) as f32;
            let linear = t * (bins - 1) as f32;
            let log = 10f32.powf(t * log_max);
            ((1.0 - LOG_BLEND) * linear + LOG_BLEND * log).clamp(0.0, (bins - 1) as f32)
        };
        let points: [f32; POINTS] = std::array::from_fn(|x| {
            // Where the axis is finer than the bins, interpolate, as
            // Webamp does. Where it is coarser, average the bins halfway to
            // each neighbour instead of sampling one: the same level on
            // music, but a pure tone cannot fall between two points.
            let (lo, hi) = (position(x as f32 - 0.5), position(x as f32 + 0.5));
            if hi - lo < 2.0 {
                let at = position(x as f32);
                let (i, frac) = (at.floor() as usize, at.fract());
                level(i) * (1.0 - frac) + level(i + 1) * frac
            } else {
                let (first, last) = (lo.round() as usize, hi.round() as usize);
                (first..last).map(level).sum::<f32>() / (last - first) as f32
            }
        });
        std::array::from_fn(|bar| {
            // 75 points make 18 fours and a three.
            let group = &points[bar * 4..(bar * 4 + 4).min(POINTS)];
            let mean = group.iter().sum::<f32>() / group.len() as f32;
            mean.min(f32::from(HEIGHT))
        })
    }

    /// One fixed physics step: bars jump up and fall at a constant rate;
    /// peaks ride the bars up, then fall with acceleration.
    fn step(&mut self, targets: &[f32; BAR_COUNT]) {
        let bars = self.bars.iter_mut().zip(targets);
        let peaks = self.peaks.iter_mut().zip(self.peak_speed.iter_mut());
        for ((bar, &target), (peak, speed)) in bars.zip(peaks) {
            *bar = (*bar - BAR_FALL).max(target);
            if *bar >= *peak {
                *peak = *bar;
                *speed = PEAK_FALL_START;
            } else {
                *peak = (*peak - *speed).max(0.0);
                *speed *= PEAK_FALL_ACCEL;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const RATE: f32 = 48_000.0;

    fn analyzer() -> (rtrb::Producer<f32>, Analyzer) {
        let (tx, rx) = rtrb::RingBuffer::new(16_384);
        (tx, Analyzer::new(rx))
    }

    /// Feed one 60fps frame's worth of a sine, then update.
    fn feed_sine(tx: &mut rtrb::Producer<f32>, a: &mut Analyzer, hz: f32, t: &mut Instant) {
        for n in 0..WINDOW_SIZE {
            let _ = tx.push((std::f32::consts::TAU * hz * n as f32 / RATE).sin());
        }
        *t += Duration::from_micros(16_667);
        a.update(*t);
    }

    fn heights(a: &Analyzer) -> Vec<u8> {
        a.bars().iter().map(|b| b.height).collect()
    }

    #[test]
    fn bass_lights_the_left_and_treble_the_right() {
        let (mut tx, mut a) = analyzer();
        let mut t = Instant::now();
        for _ in 0..5 {
            feed_sine(&mut tx, &mut a, 100.0, &mut t);
        }
        let low = heights(&a);
        assert!(low[..4].iter().any(|&h| h >= 14), "100 Hz: {low:?}");
        assert!(low[14..].iter().all(|&h| h < 8), "100 Hz: {low:?}");

        let (mut tx, mut a) = analyzer();
        for _ in 0..5 {
            feed_sine(&mut tx, &mut a, 12_000.0, &mut t);
        }
        let high = heights(&a);
        assert!(high[14..].iter().any(|&h| h >= 14), "12 kHz: {high:?}");
        assert!(high[..4].iter().all(|&h| h < 8), "12 kHz: {high:?}");
    }

    #[test]
    fn silence_draws_nothing() {
        let (_tx, mut a) = analyzer();
        let mut t = Instant::now();
        for _ in 0..3 {
            t += Duration::from_millis(16);
            a.update(t);
        }
        assert!(a.bars().iter().all(|b| *b == Bar::default()));
    }

    #[test]
    fn bars_fall_gradually_and_peaks_linger() {
        let (mut tx, mut a) = analyzer();
        let mut t = Instant::now();
        for _ in 0..5 {
            feed_sine(&mut tx, &mut a, 1_000.0, &mut t);
        }
        let loud = a.bars();
        let top = (0..BAR_COUNT).max_by_key(|&i| loud[i].height).unwrap();
        assert!(loud[top].height >= 14);

        // One silent frame: the bar drops by about one step, not to zero.
        t += Duration::from_micros(16_667);
        a.update(t);
        let after = a.bars()[top];
        assert!(after.height >= loud[top].height - 2, "{after:?}");

        // Half a second of silence: the bar has fallen well below the peak,
        // which is still in the air.
        for _ in 0..30 {
            t += Duration::from_micros(16_667);
            a.update(t);
        }
        let later = a.bars()[top];
        let peak = later.peak.expect("peak should still be visible");
        assert!(peak > later.height, "{later:?}");
    }

    #[test]
    fn physics_is_frame_rate_independent() {
        let run = |frame: Duration| {
            let (mut tx, mut a) = analyzer();
            let mut t = Instant::now();
            for _ in 0..5 {
                feed_sine(&mut tx, &mut a, 1_000.0, &mut t);
            }
            let end = t + Duration::from_millis(200);
            while t < end {
                t += frame;
                a.update(t);
            }
            heights(&a)
        };
        let at_30 = run(Duration::from_micros(33_333));
        let at_120 = run(Duration::from_micros(8_333));
        for (a, b) in at_30.iter().zip(&at_120) {
            assert!(a.abs_diff(*b) <= 1, "30fps {at_30:?} vs 120fps {at_120:?}");
        }
    }
}
