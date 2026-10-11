//! Playback: decode, resample, DSP, output.
//!
//! Headless and CLI-testable — nothing here knows about windows or skins.
//!
//! # Threading
//!
//! Three threads, described in full in the architecture doc §5:
//!
//! ```text
//!   UI ──commands──► decoder ──PCM ring──► audio callback (realtime)
//!    ▲                                            │
//!    └────── atomics: frames consumed, state ─────┘
//!    ◄────── tap ring: raw samples for the FFT ───┘
//! ```
//!
//! The audio callback allocates nothing, locks nothing and makes no syscalls.
//! EQ coefficient updates reach it through a second ring rather than a mutex,
//! and the FFT runs on the UI thread — the analyser needs 60fps, not 44,100.

pub mod engine;
pub mod eq;
pub mod eqf;
pub mod error;
pub mod resample;
pub mod spectrum;
pub mod tags;

pub use error::AudioError;
