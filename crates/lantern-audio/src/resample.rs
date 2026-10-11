//! Sample-rate conversion on the decoder thread.
//!
//! Wraps `rubato`'s FFT resampler — band-limited, so downsampling does not
//! alias the way M1's linear stopgap did. Lives on the decoder thread, never in
//! the audio callback, where allocation and latency are both acceptable.
//!
//! Gapless playback puts two requirements on it beyond quality:
//!
//! - **No leading silence.** The filter's startup delay is trimmed from the
//!   front of each track's output.
//! - **No lost tail.** [`Resampler::finish`] flushes the input still buffered
//!   and the filter's delay line, and cuts the output at exactly
//!   `input_frames * ratio`. Without it every track would end a few
//!   milliseconds short, which is audible as a click at a gapless seam.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler as _};

/// Input frames per FFT chunk. Larger is cheaper per frame but adds delay,
/// which only matters for how much `finish` has to flush.
const CHUNK_FRAMES: usize = 1024;

/// Interleaved resampler that carries state across calls.
pub struct Resampler {
    /// `None` when the rates match and samples pass straight through.
    inner: Option<Fft<f32>>,
    channels: usize,
    ratio: f64,
    /// Input awaiting a full chunk.
    pending: Vec<f32>,
    /// Scratch output for one chunk.
    chunk_out: Vec<f32>,
    /// Output frames still to discard from the front: the filter's delay.
    delay_left: usize,
    frames_in: u64,
    frames_out: u64,
}

impl Resampler {
    pub fn new(src_rate: u32, dst_rate: u32, channels: usize) -> Self {
        let channels = channels.max(1);
        let inner = (src_rate != dst_rate)
            .then(|| {
                Fft::<f32>::new(
                    src_rate as usize,
                    dst_rate as usize,
                    CHUNK_FRAMES,
                    channels,
                    FixedSync::Input,
                )
                .ok()
            })
            .flatten();
        let delay_left = inner.as_ref().map_or(0, |r| r.output_delay());
        let chunk_out = vec![0.0; inner.as_ref().map_or(0, |r| r.output_frames_max()) * channels];
        Self {
            inner,
            channels,
            ratio: f64::from(dst_rate) / f64::from(src_rate.max(1)),
            pending: Vec::new(),
            chunk_out,
            delay_left,
            frames_in: 0,
            frames_out: 0,
        }
    }

    /// True when input and output rates match, so the work is skipped.
    pub fn is_passthrough(&self) -> bool {
        self.inner.is_none()
    }

    /// Forget all state, for a seek: filtering across the discontinuity
    /// would smear old audio into new.
    pub fn reset(&mut self) {
        if let Some(r) = &mut self.inner {
            r.reset();
            self.delay_left = r.output_delay();
        }
        self.pending.clear();
        self.frames_in = 0;
        self.frames_out = 0;
    }

    /// Output frames the input already given will still produce. Exact,
    /// because output is aligned to input: it is whatever `finish` would
    /// still owe. Used to place a gapless seam.
    pub fn latency_frames(&self) -> u64 {
        if self.inner.is_none() {
            return 0;
        }
        let owed = (self.frames_in as f64 * self.ratio).round() as u64;
        owed.saturating_sub(self.frames_out)
    }

    /// Resample interleaved `src`, appending whatever output is ready.
    /// Some input is held back until a full chunk arrives.
    pub fn process(&mut self, src: &[f32], out: &mut Vec<f32>) {
        if self.inner.is_none() {
            out.extend_from_slice(src);
            return;
        }
        self.frames_in += (src.len() / self.channels) as u64;
        self.pending.extend_from_slice(src);
        while self.pending.len() >= CHUNK_FRAMES * self.channels {
            self.run_chunk(CHUNK_FRAMES, out);
        }
    }

    /// End of input: flush everything buffered, so the output is exactly as
    /// long as the input implies. Call once per track, before the next.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        if self.inner.is_none() {
            return;
        }
        let expected = (self.frames_in as f64 * self.ratio).round() as u64;
        // Bounded: a few chunks of silence always flush the delay line.
        for _ in 0..8 {
            if self.frames_out >= expected {
                break;
            }
            let partial = (self.pending.len() / self.channels).min(CHUNK_FRAMES);
            self.run_chunk(partial, out);
        }
        // Cut the silence the flush appended past the true end.
        let excess = self.frames_out.saturating_sub(expected) as usize * self.channels;
        out.truncate(out.len().saturating_sub(excess));
        self.frames_out = expected;
    }

    /// Resample one chunk from the front of `pending`, of which `frames` are
    /// real input and the rest silence.
    fn run_chunk(&mut self, frames: usize, out: &mut Vec<f32>) {
        let Some(r) = &mut self.inner else { return };
        let ch = self.channels;
        let need = r.input_frames_next();
        // Pad a short final chunk with silence, so the input is always the
        // size the resampler expects.
        let real = frames * ch;
        self.pending.resize(self.pending.len().max(need * ch), 0.0);
        let (Ok(input), Ok(mut output)) = (
            InterleavedSlice::new(&self.pending[..need * ch], ch, need),
            InterleavedSlice::new_mut(&mut self.chunk_out, ch, r.output_frames_max()),
        ) else {
            self.pending.clear();
            return;
        };
        let indexing = Indexing {
            input_offset: 0,
            output_offset: 0,
            partial_len: None,
            active_channels_mask: None,
        };
        let produced = r
            .process_into_buffer(&input, &mut output, Some(&indexing))
            .map_or(0, |(_, produced)| produced);
        // Drop the consumed input, and any padding that stood in for it.
        self.pending.drain(..need * ch);
        if real < need * ch {
            self.pending.clear();
        }

        let skip = self.delay_left.min(produced);
        self.delay_left -= skip;
        out.extend_from_slice(&self.chunk_out[skip * ch..produced * ch]);
        self.frames_out += (produced - skip) as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::Resampler;

    fn sine(hz: f32, rate: u32, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|n| {
                let s = (std::f32::consts::TAU * hz * n as f32 / rate as f32).sin() * 0.5;
                [s, s]
            })
            .collect()
    }

    /// Feed in uneven pieces, as packets arrive, then finish.
    fn run(r: &mut Resampler, input: &[f32]) -> Vec<f32> {
        let mut out = Vec::new();
        for piece in input.chunks(1152 * 2) {
            r.process(piece, &mut out);
        }
        r.finish(&mut out);
        out
    }

    #[test]
    fn matching_rates_pass_through() {
        let mut r = Resampler::new(48_000, 48_000, 2);
        assert!(r.is_passthrough());
        let input = sine(440.0, 48_000, 100);
        assert_eq!(run(&mut r, &input), input);
    }

    #[test]
    fn output_length_is_exactly_the_ratio() {
        // Gapless depends on this: no frames lost, none invented.
        for frames in [100, 1024, 44_100, 50_001] {
            let mut r = Resampler::new(44_100, 48_000, 2);
            let out = run(&mut r, &sine(440.0, 44_100, frames));
            let expected = (frames as f64 * 48_000.0 / 44_100.0).round() as usize;
            assert_eq!(out.len() / 2, expected, "{frames} input frames");
        }
    }

    #[test]
    fn no_leading_silence() {
        let mut r = Resampler::new(44_100, 48_000, 2);
        let out = run(&mut r, &vec![0.5; 44_100 * 2]);
        // DC in must be DC out from the first frames, not after a filter
        // delay's worth of zeros.
        assert!(out[20].abs() > 0.3, "output starts {:?}", &out[..8]);
    }

    #[test]
    fn a_sine_keeps_its_pitch() {
        let mut r = Resampler::new(44_100, 48_000, 2);
        let out = run(&mut r, &sine(1_000.0, 44_100, 44_100));
        // Rising zero crossings of the left channel over one second.
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let crossings = left
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        assert!((995..=1005).contains(&crossings), "{crossings} Hz");
    }

    #[test]
    fn downsampling_does_not_alias() {
        // 22 kHz is above the 16 kHz Nyquist of 32 kHz output. Linear
        // interpolation folds it back as a loud 10 kHz tone; a band-limited
        // resampler removes it.
        let mut r = Resampler::new(48_000, 32_000, 2);
        let out = run(&mut r, &sine(22_000.0, 48_000, 48_000));
        let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
        assert!(rms < 0.01, "aliased energy {rms}");
    }

    #[test]
    fn latency_is_what_finish_still_owes() {
        let mut r = Resampler::new(44_100, 48_000, 2);
        let mut out = Vec::new();
        for piece in sine(440.0, 44_100, 20_000).chunks(1152 * 2) {
            r.process(piece, &mut out);
        }
        let (emitted, owed) = (out.len() as u64 / 2, r.latency_frames());
        r.finish(&mut out);
        assert_eq!(emitted + owed, out.len() as u64 / 2);
    }

    #[test]
    fn reset_starts_clean() {
        let mut r = Resampler::new(44_100, 48_000, 2);
        let _ = run(&mut r, &sine(440.0, 44_100, 5_000));
        r.reset();
        let out = run(&mut r, &sine(440.0, 44_100, 2_000));
        let expected = (2_000.0f64 * 48_000.0 / 44_100.0).round() as usize;
        assert_eq!(out.len() / 2, expected);
    }
}
