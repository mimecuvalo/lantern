//! Platform shell.
//!
//! The core is platform-neutral by construction; only integration lives here.
//! We build and ship macOS alone for now, but nothing above this module knows
//! that, so a Linux or Windows shell is an afternoon rather than a rewrite.

#[cfg(target_os = "macos")]
pub mod apple_music;
#[cfg(target_os = "macos")]
pub mod macos;
