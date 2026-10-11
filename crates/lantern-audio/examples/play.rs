//! Headless playback smoke test.
//!
//! `cargo run -p lantern-audio --example play -- <file> [seconds]`
//!
//! Verifies the decode -> ring -> callback path without any UI: reports the
//! position the engine believes it is at against wall-clock elapsed time. The
//! two tracking each other is the evidence that audio is genuinely flowing at
//! the right rate, which is checkable without being able to hear it.

use std::time::{Duration, Instant};

use lantern_audio::engine::{Command, Engine, Event};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: play <file> [seconds]")?;
    let secs: f64 = args.next().unwrap_or_else(|| "5".into()).parse()?;

    let mut engine = Engine::new()?;
    let mut tap = engine.take_tap().expect("tap is available once");
    println!("output rate: {} Hz", engine.output_rate());

    engine.send(Command::Open(std::path::PathBuf::from(&path).into()))?;
    engine.send(Command::SetVolume(0.6))?;
    engine.send(Command::Play)?;

    let info = loop {
        match engine.poll_event() {
            Some(Event::Opened(info)) => break info,
            Some(Event::OpenFailed { error, .. }) => return Err(error.into()),
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    println!(
        "opened: {:?}  {} Hz  {} ch  {:?} kbps  {:?} s",
        info.title, info.sample_rate, info.channels, info.kbps, info.duration_secs
    );

    let start = Instant::now();
    let mut last = 0.0;
    while start.elapsed().as_secs_f64() < secs {
        std::thread::sleep(Duration::from_millis(500));
        let wall = start.elapsed().as_secs_f64();
        let pos = engine.position_secs();
        println!(
            "wall {wall:5.2}s   engine {pos:5.2}s   drift {:+.3}s   {}",
            pos - wall,
            if pos > last { "advancing" } else { "STALLED" }
        );
        last = pos;
    }

    engine.send(Command::Pause)?;
    let paused_at = engine.position_secs();
    std::thread::sleep(Duration::from_millis(600));
    let after = engine.position_secs();
    println!("\npause held position: {paused_at:.2}s -> {after:.2}s");
    assert!(
        (after - paused_at).abs() < 0.01,
        "pause did not stop the callback consuming"
    );

    engine.send(Command::Play)?;
    std::thread::sleep(Duration::from_millis(600));
    println!("resumed to: {:.2}s", engine.position_secs());

    // A heavy EQ must not stall or silence the callback.
    engine.send(Command::SetEq(lantern_audio::eq::EqSettings {
        enabled: true,
        preamp_db: -6.0,
        band_gains_db: [9.0, 6.0, 3.0, 0.0, -3.0, -3.0, 0.0, 3.0, 6.0, 9.0],
    }))?;
    let before_eq = engine.position_secs();
    std::thread::sleep(Duration::from_millis(500));
    let after_eq = engine.position_secs();
    println!("with EQ on: {before_eq:.2}s -> {after_eq:.2}s");
    assert!(
        after_eq - before_eq > 0.4,
        "playback stalled with the EQ on"
    );

    // The tap only fills while playing; a live track must have fed it.
    let tapped = tap.slots();
    println!("tap holds {tapped} samples");
    assert!(tapped > 0, "callback pushed nothing into the analyser tap");
    while tap.pop().is_ok() {}

    // Seek forwards: position must jump to the target and keep advancing.
    let target = info.duration_secs.map_or(30.0, |d| (d / 2.0).min(60.0));
    engine.send(Command::Seek { seconds: target })?;
    std::thread::sleep(Duration::from_millis(400));
    let after_seek = engine.position_secs();
    println!("seek to {target:.2}s -> {after_seek:.2}s after 0.4s");
    assert!(
        (after_seek - target - 0.4).abs() < 0.3,
        "seek landed at {after_seek:.2}s, wanted about {:.2}s",
        target + 0.4
    );

    // Seek near the end: the engine must report the track finished.
    if let Some(d) = info.duration_secs {
        engine.send(Command::Seek {
            seconds: (d - 1.0).max(0.0),
        })?;
        let start = Instant::now();
        let ended = loop {
            if let Some(Event::Ended) = engine.poll_event() {
                break true;
            }
            if start.elapsed() > Duration::from_secs(5) {
                break false;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        println!(
            "end of track reported: {ended}  transport: {:?}",
            engine.transport()
        );
        assert!(
            ended,
            "no Ended event within 5s of seeking to the last second"
        );
    }
    println!("\nOK");
    Ok(())
}
