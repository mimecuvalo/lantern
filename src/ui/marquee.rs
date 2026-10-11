//! The scrolling song-title display.
//!
//! Behaviour follows Webamp's `Marquee.tsx` (MIT, Copyright (c) 2015 Jordan
//! Eldredge): text that fits sits still; text of 31 characters or more loops
//! with a `  ***  ` separator, advancing one 5px character every 220ms.
//! While a slider is dragged, the marquee shows what the drag is doing
//! instead.

use std::time::{Duration, Instant};

use lantern_skin::font::GLYPH_WIDTH;

const SEPARATOR: &str = "  ***  ";
const MAX_STILL_LEN: usize = 31;
const STEP_INTERVAL: Duration = Duration::from_millis(220);

#[derive(Debug)]
pub struct Marquee {
    step: u32,
    last_step: Instant,
}

impl Marquee {
    pub fn new(now: Instant) -> Self {
        Self {
            step: 0,
            last_step: now,
        }
    }

    /// Advance the scroll to `now`. Returns true if the text moved.
    pub fn tick(&mut self, now: Instant) -> bool {
        let mut moved = false;
        while now.duration_since(self.last_step) >= STEP_INTERVAL {
            self.last_step += STEP_INTERVAL;
            self.step = self.step.wrapping_add(1);
            moved = true;
        }
        moved
    }

    /// Start from the beginning, for a new track.
    pub fn reset(&mut self, now: Instant) {
        self.step = 0;
        self.last_step = now;
    }

    /// The string to draw and how many pixels to shift it left.
    pub fn layout(&self, text: &str) -> (String, u32) {
        let chars = text.chars().count();
        if chars < MAX_STILL_LEN {
            return (text.to_string(), 0);
        }
        let period = (chars + SEPARATOR.len()) as u32 * GLYPH_WIDTH;
        let offset = (self.step * GLYPH_WIDTH) % period;
        (format!("{text}{SEPARATOR}{text}"), offset)
    }

    /// Whether `text` scrolls, and so needs animation frames.
    pub fn scrolls(text: &str) -> bool {
        text.chars().count() >= MAX_STILL_LEN
    }
}

/// `m:ss`, as the marquee writes durations.
pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// "3. Artist - Title (3:45)" — Winamp's form: playlist position, title,
/// length.
pub fn track_text(number: usize, title: &str, duration: Option<f64>) -> String {
    match duration {
        Some(d) => format!("{number}. {title} ({})", clock(d)),
        None => format!("{number}. {title}"),
    }
}

pub fn volume_text(volume: f32) -> String {
    format!("VOLUME: {}%", (volume * 100.0).round() as u32)
}

pub fn balance_text(balance: f32) -> String {
    let pct = (balance.abs() * 100.0).round() as u32;
    match pct {
        0 => "BALANCE: CENTER".to_string(),
        _ if balance < 0.0 => format!("BALANCE: {pct}% LEFT"),
        _ => format!("BALANCE: {pct}% RIGHT"),
    }
}

pub fn seek_text(fraction: f32, duration: f64) -> String {
    let target = f64::from(fraction) * duration;
    format!(
        "SEEK TO: {}/{} ({}%)",
        clock(target),
        clock(duration),
        (fraction * 100.0).round() as u32
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_stands_still() {
        let m = Marquee::new(Instant::now());
        assert_eq!(m.layout("1. SHORT (1:00)"), ("1. SHORT (1:00)".into(), 0));
        assert!(!Marquee::scrolls("1. SHORT (1:00)"));
    }

    #[test]
    fn long_text_loops_with_a_separator() {
        let t0 = Instant::now();
        let mut m = Marquee::new(t0);
        let text = "1. A Very Long Artist Name - A Very Long Song Title (12:34)";
        let (drawn, offset) = m.layout(text);
        assert_eq!(drawn, format!("{text}  ***  {text}"));
        assert_eq!(offset, 0);

        assert!(m.tick(t0 + Duration::from_millis(450)), "two steps elapsed");
        assert_eq!(m.layout(text).1, 2 * GLYPH_WIDTH);

        // One full lap wraps back to the start.
        let lap = (text.len() + SEPARATOR.len()) as u32;
        m.tick(t0 + STEP_INTERVAL * lap);
        assert_eq!(m.layout(text).1, 0);
    }

    #[test]
    fn slider_messages() {
        assert_eq!(volume_text(0.7), "VOLUME: 70%");
        assert_eq!(balance_text(0.0), "BALANCE: CENTER");
        assert_eq!(balance_text(-0.3), "BALANCE: 30% LEFT");
        assert_eq!(balance_text(1.0), "BALANCE: 100% RIGHT");
        assert_eq!(seek_text(0.5, 200.0), "SEEK TO: 1:40/3:20 (50%)");
    }

    #[test]
    fn track_text_includes_duration() {
        assert_eq!(track_text(1, "X - Y", Some(225.4)), "1. X - Y (3:45)");
        assert_eq!(track_text(12, "X - Y", None), "12. X - Y");
    }
}
