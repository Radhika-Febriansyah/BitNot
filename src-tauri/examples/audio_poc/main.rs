//! BitNot audio pipeline proof of concept (Phase 3).
//!
//! Standalone: not part of the application and not wired into it. It answers
//! one question with measurements: can the current rodio 0.23 pipeline carry
//! gapless playback, a 10-band EQ, crossfade, a visualizer tap, and a limiter
//! with acceptable accuracy and CPU cost, or is a custom Symphonia + cpal
//! pipeline justified?
//!
//! Run from `src-tauri/`:
//!   cargo run --release --example audio_poc -- all
//!   cargo run --release --example audio_poc -- gapless|eq|crossfade|capture|cpu|device|cpal
//!
//! Device runs are silent by default (the audio is measured, then multiplied
//! by zero before the speakers). Set BITNOT_POC_AUDIBLE=1 to hear them.
//! Test files are generated into target/audio-poc/.

// Index loops read more clearly in this DSP code.
#![allow(clippy::needless_range_loop)]

mod analysis;
mod dsp;
mod experiments;
mod signals;

use std::time::Instant;

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    let dir = std::path::PathBuf::from("target/audio-poc");
    std::fs::create_dir_all(&dir).expect("create output folder");
    let started = Instant::now();
    experiments::environment();
    let all = which == "all";
    if all || which == "gapless" {
        experiments::gapless(&dir);
    }
    if all || which == "eq" {
        experiments::equalizer();
    }
    if all || which == "crossfade" {
        experiments::crossfade(&dir);
    }
    if all || which == "capture" {
        experiments::capture();
    }
    if all || which == "cpu" {
        experiments::cpu(&dir);
    }
    if all || which == "device" {
        experiments::device(&dir);
    }
    if all || which == "cpal" {
        experiments::raw_cpal(&dir);
    }
    println!("\nFinished in {:.1} s.", started.elapsed().as_secs_f64());
}
