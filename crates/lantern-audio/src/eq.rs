//! Ten-band equaliser and preamp.
//!
//! Filters are the standard biquads from Robert Bristow-Johnson's "Audio EQ
//! Cookbook" (public domain formulas): peaking bands one octave wide, as the
//! Swift implementation used, with shelves at either end.
//!
//! Coefficients are computed on the UI thread ([`EqParams::new`]) and cross to
//! the audio callback through a lock-free ring. The callback only runs
//! [`Equalizer::process`], which neither allocates nor branches on anything
//! but plain data.

/// Winamp 2's actual band centres, in Hz.
///
/// Deliberately **not** the ISO octave series (32/64/125/250/...). A fork of
/// the Swift implementation "corrected" these and the change was rejected:
/// matching Winamp is the entire point, and saved EQ presets from the era
/// assume these centres.
pub const BAND_FREQUENCIES: [f32; 10] = [
    70.0, 180.0, 320.0, 600.0, 1000.0, 3000.0, 6000.0, 12000.0, 14000.0, 16000.0,
];

/// Per-band gain limit, in dB.
pub const BAND_GAIN_LIMIT_DB: f32 = 12.0;

/// Preamp limit, in dB.
pub const PREAMP_LIMIT_DB: f32 = 12.0;

/// Filter shape for a band.
///
/// The outer bands are **shelves, not peaking** — a parametric filter at the
/// edge of the spectrum leaves the actual extremes untouched, so dragging the
/// 70 Hz or 16 kHz slider appeared to do nothing. Fixed once already in the
/// Swift implementation; do not regress it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandShape {
    LowShelf,
    Peaking,
    HighShelf,
}

/// The shape band `index` uses.
pub fn band_shape(index: usize) -> BandShape {
    match index {
        0 => BandShape::LowShelf,
        i if i == BAND_FREQUENCIES.len() - 1 => BandShape::HighShelf,
        _ => BandShape::Peaking,
    }
}

/// Equaliser settings, as the UI holds them.
///
/// `preamp_db` is a **gain stage applied to the EQ block**, not a multiplier
/// folded into output volume. Folding it into volume means any volume or mute
/// change silently wipes the preamp — the second bug fixed in the Swift
/// implementation, and equally easy to reintroduce here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqSettings {
    pub enabled: bool,
    pub preamp_db: f32,
    pub band_gains_db: [f32; 10],
}

impl Default for EqSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            preamp_db: 0.0,
            band_gains_db: [0.0; 10],
        }
    }
}

/// Peaking bands span one octave, matching the Swift implementation's
/// `AVAudioUnitEQ` bandwidth of 1.0.
const PEAK_BANDWIDTH_OCTAVES: f64 = 1.0;

/// The most channels the equaliser keeps filter state for. Beyond that,
/// channels pass through unequalised; no classic source has more.
pub const MAX_CHANNELS: usize = 8;

/// One biquad's coefficients, normalised so `a0 == 1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Coeffs {
    pub const IDENTITY: Coeffs = Coeffs {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    /// Design one band. Returns identity at 0 dB, and for a centre at or
    /// above Nyquist, where the filter would be unstable.
    pub fn design(shape: BandShape, freq: f32, gain_db: f32, sample_rate: u32) -> Self {
        let fs = f64::from(sample_rate);
        let f0 = f64::from(freq);
        if gain_db == 0.0 || f0 >= fs / 2.0 {
            return Self::IDENTITY;
        }
        let a = 10f64.powf(f64::from(gain_db) / 40.0);
        let w0 = std::f64::consts::TAU * f0 / fs;
        let (sin, cos) = w0.sin_cos();

        let (b0, b1, b2, a0, a1, a2) = match shape {
            BandShape::Peaking => {
                let alpha =
                    sin * (std::f64::consts::LN_2 / 2.0 * PEAK_BANDWIDTH_OCTAVES * w0 / sin).sinh();
                (
                    1.0 + alpha * a,
                    -2.0 * cos,
                    1.0 - alpha * a,
                    1.0 + alpha / a,
                    -2.0 * cos,
                    1.0 - alpha / a,
                )
            }
            // Shelf slope S = 1: the steepest without overshoot.
            BandShape::LowShelf | BandShape::HighShelf => {
                let alpha = sin / 2.0 * std::f64::consts::SQRT_2;
                let k = 2.0 * a.sqrt() * alpha;
                let (p, m) = (a + 1.0, a - 1.0);
                if shape == BandShape::LowShelf {
                    (
                        a * (p - m * cos + k),
                        2.0 * a * (m - p * cos),
                        a * (p - m * cos - k),
                        p + m * cos + k,
                        -2.0 * (m + p * cos),
                        p + m * cos - k,
                    )
                } else {
                    (
                        a * (p + m * cos + k),
                        -2.0 * a * (m + p * cos),
                        a * (p + m * cos - k),
                        p - m * cos + k,
                        2.0 * (m - p * cos),
                        p - m * cos - k,
                    )
                }
            }
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Gain in dB at `freq`, from the transfer function. For tests and for
    /// drawing the EQ window's response curve.
    pub fn response_db(&self, freq: f32, sample_rate: u32) -> f64 {
        let w = std::f64::consts::TAU * f64::from(freq) / f64::from(sample_rate);
        let (c1, s1) = (w.cos(), -w.sin());
        let (c2, s2) = ((2.0 * w).cos(), -(2.0 * w).sin());
        let num = (
            self.b0 + self.b1 * c1 + self.b2 * c2,
            self.b1 * s1 + self.b2 * s2,
        );
        let den = (
            1.0 + self.a1 * c1 + self.a2 * c2,
            self.a1 * s1 + self.a2 * s2,
        );
        let mag = (num.0.hypot(num.1)) / (den.0.hypot(den.1));
        20.0 * mag.log10()
    }
}

/// Everything the callback needs to equalise, precomputed. `Copy`, so it
/// crosses the ring without allocating.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqParams {
    pub enabled: bool,
    /// Linear gain applied ahead of the bands, only while enabled.
    pub preamp: f32,
    pub bands: [Coeffs; 10],
}

impl EqParams {
    /// Unity: disabled, flat.
    pub const FLAT: EqParams = EqParams {
        enabled: false,
        preamp: 1.0,
        bands: [Coeffs::IDENTITY; 10],
    };

    pub fn new(settings: &EqSettings, sample_rate: u32) -> Self {
        let clamp = |db: f32, limit: f32| db.clamp(-limit, limit);
        Self {
            enabled: settings.enabled,
            preamp: 10f32.powf(clamp(settings.preamp_db, PREAMP_LIMIT_DB) / 20.0),
            bands: std::array::from_fn(|i| {
                Coeffs::design(
                    band_shape(i),
                    BAND_FREQUENCIES[i],
                    clamp(settings.band_gains_db[i], BAND_GAIN_LIMIT_DB),
                    sample_rate,
                )
            }),
        }
    }
}

/// The realtime half: filter state plus the current parameters.
pub struct Equalizer {
    params: EqParams,
    /// Transposed direct form II state, `[z1, z2]` per channel per band.
    state: [[[f64; 2]; 10]; MAX_CHANNELS],
}

impl Default for Equalizer {
    fn default() -> Self {
        Self {
            params: EqParams::FLAT,
            state: [[[0.0; 2]; 10]; MAX_CHANNELS],
        }
    }
}

impl Equalizer {
    /// Adopt new parameters. Filter state carries over, so moving a slider
    /// changes the sound without a click.
    pub fn set(&mut self, params: EqParams) {
        self.params = params;
    }

    /// Clear filter memory, for a seek: otherwise the filters ring audibly
    /// across the discontinuity.
    pub fn reset(&mut self) {
        self.state = [[[0.0; 2]; 10]; MAX_CHANNELS];
    }

    /// Equalise one sample of `channel`. Realtime-safe.
    #[inline]
    pub fn process(&mut self, channel: usize, x: f32) -> f32 {
        let p = &self.params;
        if !p.enabled || channel >= MAX_CHANNELS {
            return x;
        }
        let mut y = f64::from(x * p.preamp);
        for (c, z) in p.bands.iter().zip(self.state[channel].iter_mut()) {
            if *c == Coeffs::IDENTITY {
                continue;
            }
            let out = c.b0 * y + z[0];
            z[0] = c.b1 * y - c.a1 * out + z[1];
            z[1] = c.b2 * y - c.a2 * out;
            y = out;
        }
        y as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn assert_db(actual: f64, expected: f64, tolerance: f64, what: &str) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{what}: {actual:.2} dB, expected {expected} ± {tolerance}"
        );
    }

    #[test]
    fn zero_gain_is_identity() {
        assert_eq!(
            Coeffs::design(BandShape::Peaking, 1000.0, 0.0, RATE),
            Coeffs::IDENTITY
        );
    }

    #[test]
    fn peaking_band_hits_its_gain_at_centre_only() {
        let c = Coeffs::design(BandShape::Peaking, 1000.0, 12.0, RATE);
        assert_db(c.response_db(1000.0, RATE), 12.0, 0.01, "at centre");
        assert_db(c.response_db(100.0, RATE), 0.0, 0.5, "a decade below");
        assert_db(c.response_db(10_000.0, RATE), 0.0, 0.5, "a decade above");
        // One octave wide: about half the gain (in dB) half an octave out.
        let edge = c.response_db(1000.0 * 2f32.sqrt(), RATE);
        assert!((4.0..9.0).contains(&edge), "band edge {edge:.2} dB");
    }

    #[test]
    fn outer_shelves_reach_the_extremes() {
        // The Swift-era bug: a peaking filter at 70 Hz left 20 Hz untouched.
        let low = Coeffs::design(BandShape::LowShelf, 70.0, 12.0, RATE);
        assert_db(low.response_db(20.0, RATE), 12.0, 1.5, "low shelf at 20 Hz");
        assert_db(
            low.response_db(2000.0, RATE),
            0.0,
            0.3,
            "low shelf at 2 kHz",
        );

        let high = Coeffs::design(BandShape::HighShelf, 16_000.0, -12.0, RATE);
        assert_db(
            high.response_db(22_000.0, RATE),
            -12.0,
            2.0,
            "high shelf near Nyquist",
        );
        assert_db(
            high.response_db(1000.0, RATE),
            0.0,
            0.3,
            "high shelf at 1 kHz",
        );
    }

    #[test]
    fn bands_above_nyquist_are_skipped() {
        // A 16 kHz shelf on a 22.05 kHz device would be unstable.
        assert_eq!(
            Coeffs::design(BandShape::HighShelf, 16_000.0, 12.0, 22_050),
            Coeffs::IDENTITY
        );
    }

    #[test]
    fn disabled_is_bit_exact_passthrough() {
        let mut eq = Equalizer::default();
        let settings = EqSettings {
            enabled: false,
            preamp_db: 12.0,
            band_gains_db: [12.0; 10],
        };
        eq.set(EqParams::new(&settings, RATE));
        assert_eq!(eq.process(0, 0.123), 0.123);
    }

    #[test]
    fn preamp_is_a_gain_stage() {
        let mut eq = Equalizer::default();
        let settings = EqSettings {
            enabled: true,
            preamp_db: 6.0,
            band_gains_db: [0.0; 10],
        };
        eq.set(EqParams::new(&settings, RATE));
        assert!((eq.process(0, 0.5) - 0.5 * 1.9953).abs() < 1e-3);
    }

    #[test]
    fn a_boosted_sine_comes_out_louder_by_the_gain() {
        let mut eq = Equalizer::default();
        let mut gains = [0.0; 10];
        gains[4] = 12.0; // 1 kHz
        eq.set(EqParams::new(
            &EqSettings {
                enabled: true,
                preamp_db: 0.0,
                band_gains_db: gains,
            },
            RATE,
        ));
        let n = RATE as usize;
        let out: Vec<f32> = (0..n)
            .map(|i| {
                let x = (std::f32::consts::TAU * 1000.0 * i as f32 / RATE as f32).sin() * 0.1;
                eq.process(0, x)
            })
            .collect();
        // Skip the first 100ms of filter settling.
        let settled = &out[n / 10..];
        let rms = (settled.iter().map(|s| s * s).sum::<f32>() / settled.len() as f32).sqrt();
        let gain_db = 20.0 * (rms / (0.1 / 2f32.sqrt())).log10();
        assert!((gain_db - 12.0).abs() < 0.3, "{gain_db:.2} dB");
    }

    #[test]
    fn reset_clears_ringing() {
        let mut eq = Equalizer::default();
        eq.set(EqParams::new(
            &EqSettings {
                enabled: true,
                preamp_db: 0.0,
                band_gains_db: [12.0; 10],
            },
            RATE,
        ));
        eq.process(0, 1.0);
        eq.reset();
        assert_eq!(eq.process(0, 0.0), 0.0);
    }

    #[test]
    fn outer_bands_are_shelves() {
        assert_eq!(band_shape(0), BandShape::LowShelf);
        assert_eq!(band_shape(9), BandShape::HighShelf);
    }

    #[test]
    fn inner_bands_are_peaking() {
        for i in 1..9 {
            assert_eq!(band_shape(i), BandShape::Peaking, "band {i}");
        }
    }

    #[test]
    fn frequencies_are_winamps_not_iso() {
        // Guards against a well-meaning "fix" to the ISO octave series.
        assert_eq!(BAND_FREQUENCIES[0], 70.0);
        assert_eq!(BAND_FREQUENCIES[9], 16000.0);
    }
}
