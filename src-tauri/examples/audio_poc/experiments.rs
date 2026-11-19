//! The experiments. Each prints its method, conditions, and measured results.

use std::fs::File;
use std::io::BufReader;
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::source::{LimitSettings, UniformSourceIterator};
use rodio::{Decoder, Player, Source};

use crate::analysis::{
    channel, click_peak, db, rms, spectrum, thd_n_db, tone_amplitude, windowed_rms_db,
};
use crate::dsp::{
    Biquad, Curve, Equalizer, Fade, Filter, Limiter, Processed, Ring, SincResampler, Svf,
    TimingTap, BANDS, SINC_TAPS,
};
use crate::signals::{
    as_decoded, noise, stereo_sine, two_tones, write_aiff16, write_flac, write_wav16, Frames,
};

const TWO: NonZero<u16> = NonZero::new(2).unwrap();

fn nz_rate(rate: u32) -> NonZero<u32> {
    NonZero::new(rate).unwrap()
}

fn audible() -> bool {
    std::env::var("BITNOT_POC_AUDIBLE").as_deref() == Ok("1")
}

fn decoder(path: &Path) -> Decoder<BufReader<File>> {
    let file = File::open(path).expect("open test file");
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut builder = Decoder::builder()
        .with_data(BufReader::new(file))
        .with_byte_len(len)
        .with_seekable(true);
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        builder = builder.with_hint(ext);
    }
    builder.build().expect("decode test file")
}

fn decode_all(path: &Path) -> Vec<f32> {
    decoder(path).collect()
}

fn pull<S: Iterator<Item = f32>>(source: &mut S, samples: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(samples);
    for _ in 0..samples {
        match source.next() {
            Some(s) => out.push(s),
            None => break,
        }
    }
    out
}

fn heading(title: &str) {
    println!("\n## {title}\n");
}

/// Continuous stereo sine across files of possibly different rates: the phase
/// follows real time, so a gap or overlap shows as a phase break.
fn sine_from_time(freq: f64, amp: f64, rate: u32, t0: f64, frames: usize) -> Frames {
    (0..frames)
        .map(|n| {
            let phase = std::f64::consts::TAU * freq * (t0 + n as f64 / rate as f64);
            [amp * phase.sin(), amp * phase.cos()]
        })
        .collect()
}

// ── Environment ────────────────────────────────────────────────────────────

pub fn environment() {
    heading("Environment");
    println!("- rodio 0.23.0 (symphonia-all, playback), cpal 0.18.2 (via rodio), symphonia 0.5.5");
    println!("- Build: release profile of the example (opt-level 3)");
    let host = rodio::cpal::default_host();
    println!("- Audio host: {:?}", host.id());
    match host.default_output_device() {
        Some(device) => {
            let name = device
                .description()
                .map(|d| d.name().to_string())
                .unwrap_or_else(|_| "unknown".into());
            println!("- Default output device: {name}");
            if let Ok(config) = device.default_output_config() {
                println!(
                    "- Default config (shared mode): {} Hz, {} channels, {:?}, buffer {:?}",
                    config.sample_rate(),
                    config.channels(),
                    config.sample_format(),
                    config.buffer_size()
                );
            }
            if let Ok(configs) = device.supported_output_configs() {
                let configs: Vec<_> = configs.collect();
                let mut rates: Vec<u32> = configs.iter().map(|c| c.min_sample_rate()).collect();
                rates.sort_unstable();
                rates.dedup();
                let mut formats: Vec<String> = configs
                    .iter()
                    .map(|c| format!("{:?}", c.sample_format()))
                    .collect();
                formats.sort();
                formats.dedup();
                println!(
                    "- Shared-mode configs cpal reports: {} entries; rates {:?} Hz; formats {}",
                    configs.len(),
                    rates,
                    formats.join(", ")
                );
            }
        }
        None => println!("- No output device"),
    }
}

// ── A. Gapless ─────────────────────────────────────────────────────────────

struct Pair {
    name: &'static str,
    a: PathBuf,
    b: PathBuf,
    rate_a: u32,
    rate_b: u32,
}

fn make_pairs(dir: &Path) -> Vec<Pair> {
    let freq = 997.0;
    let amp = 0.5;
    let mut pairs = Vec::new();
    let mut write = |name: &'static str, rate_a: u32, rate_b: u32, fmt_a: &str, fmt_b: &str| {
        // Odd frame counts so the boundary falls mid-cycle and off any block size.
        let frames_a = (rate_a as f64 * 1.5) as usize + 37;
        let a = sine_from_time(freq, amp, rate_a, 0.0, frames_a);
        let t_b = frames_a as f64 / rate_a as f64;
        let b = sine_from_time(freq, amp, rate_b, t_b, rate_b as usize + 11);
        let pa = dir.join(format!("gapless-{name}-a.{fmt_a}"));
        let pb = dir.join(format!("gapless-{name}-b.{fmt_b}"));
        for (path, frames, rate, fmt) in [(&pa, &a, rate_a, fmt_a), (&pb, &b, rate_b, fmt_b)] {
            match fmt {
                "wav" => write_wav16(path, rate, frames),
                "aiff" => write_aiff16(path, rate, frames),
                "flac" if rate == 48_000 => write_flac(path, rate, 24, frames),
                "flac" => write_flac(path, rate, 16, frames),
                _ => unreachable!(),
            }
        }
        pairs.push(Pair {
            name,
            a: pa,
            b: pb,
            rate_a,
            rate_b,
        });
    };
    write("wav16-44k", 44_100, 44_100, "wav", "wav");
    write("flac16-44k", 44_100, 44_100, "flac", "flac");
    write("flac-to-wav-44k", 44_100, 44_100, "flac", "wav");
    write("aiff16-48k", 48_000, 48_000, "aiff", "aiff");
    write("flac24-48k", 48_000, 48_000, "flac", "flac");
    write("wav44k-to-flac48k", 44_100, 48_000, "wav", "flac");
    pairs
}

/// Index of the first sample where `got` differs from `want`, if any.
fn first_difference(got: &[f32], want: &[f32]) -> Option<usize> {
    got.iter()
        .zip(want)
        .position(|(g, w)| g.to_bits() != w.to_bits())
}

/// Longest run of exactly-zero frames in `x` (interleaved stereo).
fn longest_zero_run(x: &[f32]) -> usize {
    let (mut best, mut run) = (0, 0);
    for f in x.chunks(2) {
        if f.iter().all(|&s| s == 0.0) {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    best
}

pub fn gapless(dir: &Path) {
    heading("A. Gapless playback");
    println!("Method: a continuous 997 Hz stereo sine (L = sin, R = cos, -6 dBFS) is split into two files at an");
    println!("odd frame count, so the join falls mid-cycle. File B continues A's phase exactly. Both files are");
    println!("appended to a rodio `Player` before playback, and the output is compared with the two files decoded");
    println!("separately and concatenated (bit-exact comparison), and checked for zero runs and second-difference");
    println!("spikes at the join.\n");
    let pairs = make_pairs(dir);

    println!("### A1. rodio Player queue, both files appended before playback (queue output, no resampling)\n");
    println!("| Pair | Frames A | Output = A ++ B (bit-exact)? | First differing sample | Longest zero run at join | Join spike / body |");
    println!("|---|---|---|---|---|---|");
    for pair in &pairs {
        let want: Vec<f32> = decode_all(&pair.a)
            .into_iter()
            .chain(decode_all(&pair.b))
            .collect();
        let (player, mut out) = Player::new();
        player.append(decoder(&pair.a));
        player.append(decoder(&pair.b));
        let got = pull(&mut out, want.len());
        let frames_a = decode_all(&pair.a).len() / 2;
        let diff = first_difference(&got, &want);
        let around = &got[(frames_a - 64) * 2..(frames_a + 64) * 2];
        let l = channel(&got, 0);
        let body = click_peak(&l, 1000..frames_a - 1000);
        let join = click_peak(&l, frames_a - 4..frames_a + 4);
        println!(
            "| {} | {} | {} | {} | {} | {:.3} |",
            pair.name,
            frames_a,
            if diff.is_none() && got.len() == want.len() {
                "yes"
            } else {
                "no"
            },
            diff.map(|d| d.to_string()).unwrap_or_else(|| "none".into()),
            longest_zero_run(around),
            join / body
        );
    }

    println!("\n### A2. Same queue, B appended only after A has finished (late append)\n");
    println!("| Pair | Silence inserted at the join |");
    println!("|---|---|");
    for pair in pairs.iter().take(2) {
        let (player, mut out) = Player::new();
        player.append(decoder(&pair.a));
        let a_len = decode_all(&pair.a).len();
        let mut got = pull(&mut out, a_len);
        // A is exhausted; the queue (keep-alive) fills with silence until B arrives.
        got.extend(pull(&mut out, 1));
        player.append(decoder(&pair.b));
        got.extend(pull(&mut out, 4096));
        let zeros = got[a_len..].iter().take_while(|&&s| s == 0.0).count();
        println!(
            "| {} | {} samples = {} frames = {:.2} ms |",
            pair.name,
            zeros,
            zeros / 2,
            zeros as f64 / 2.0 / pair.rate_b as f64 * 1000.0
        );
    }

    println!("\n### A3. Through a rodio Mixer at 48 kHz (as on the device)\n");
    println!("The queue is added to the mixer after both files are appended (see A5 for why). For pairs at 48 kHz");
    println!("the output is compared bit for bit. For 44.1 kHz pairs, which rodio resamples, the 997 Hz tone's phase");
    println!("is measured per 10 ms block: a gap would show as a large phase step at the join.\n");
    println!("| Pair | Resampled? | Bit-exact vs A ++ B | Longest zero run near join | Phase step at join | Typical step elsewhere (count) | Output length vs ideal |");
    println!("|---|---|---|---|---|---|---|");
    for pair in &pairs {
        let (mixer, mut mixed) = rodio::mixer::mixer(TWO, nz_rate(48_000));
        let (player, queue) = Player::new();
        player.append(decoder(&pair.a));
        player.append(decoder(&pair.b));
        mixer.add(queue);
        let frames_a = decode_all(&pair.a).len() / 2;
        let frames_b = decode_all(&pair.b).len() / 2;
        let dur_a = frames_a as f64 / pair.rate_a as f64;
        let ideal_frames =
            ((dur_a + frames_b as f64 / pair.rate_b as f64) * 48_000.0).round() as usize;
        // Pull past the end of B: the keep-alive queue then gives exact zeros, trimmed here.
        let mut got = pull(&mut mixed, (ideal_frames + 2_000) * 2);
        while got.len() > 2 && got[got.len() - 2..].iter().all(|&s| s == 0.0) {
            got.truncate(got.len() - 2);
        }
        let out_frames = got.len() / 2;
        let join = (dur_a * 48_000.0).round() as usize;
        let near = 96; // 2 ms
        let resampled = pair.rate_a != 48_000 || pair.rate_b != 48_000;
        let exact = if resampled {
            "n/a".to_string()
        } else {
            let want: Vec<f32> = decode_all(&pair.a)
                .into_iter()
                .chain(decode_all(&pair.b))
                .collect();
            (if first_difference(&got, &want).is_none() && got.len() == want.len() {
                "yes"
            } else {
                "no"
            })
            .to_string()
        };
        let l = channel(&got, 0);
        let steps = phase_steps(&l, 48_000.0, 997.0, 480);
        let at_join = steps
            .iter()
            .filter(|s| s.0 + 960 >= join && s.0 <= join + 960)
            .map(|s| s.1.abs())
            .fold(0.0, f64::max);
        let mut others: Vec<f64> = steps
            .iter()
            .filter(|s| s.0 + 960 < join || s.0 > join + 960)
            .map(|s| s.1.abs())
            .collect();
        others.sort_by(f64::total_cmp);
        let typical = if others.is_empty() {
            "none".to_string()
        } else {
            format!("{:.1} µs ({})", others[others.len() / 2], others.len())
        };
        println!(
            "| {} | {} | {} | {} | {} | {} | {} frames ({:+}) |",
            pair.name,
            if resampled { "yes" } else { "no" },
            exact,
            longest_zero_run(&got[(join - near) * 2..(join + near) * 2]),
            if at_join > 0.0 {
                format!("{at_join:.1} µs")
            } else {
                "none".into()
            },
            typical,
            out_frames,
            out_frames as i64 - ideal_frames as i64
        );
    }

    println!("\n### A4. rodio's sample-rate conversion (44.1 kHz into a 48 kHz mixer)\n");
    println!("rodio converts by linear interpolation (src/conversions/sample_rate.rs), and its UniformSourceIterator");
    println!("starts a new converter at every span boundary of the source (each decoder packet: 2304 samples for");
    println!("these WAV files). Two cases: the same 2 s tone as one continuous buffer (one span: interpolation");
    println!("only), and decoded from the file (spans: interpolation plus the restarts). THD+N is measured per");
    println!("100 ms block (fit of the tone in each block), so it is not affected by slow timing drift.\n");
    println!("| Tone | File itself | One span: worst block THD+N | Decoder: worst / median block THD+N | Decoder: timing steps, total drift | Decoder: output length vs ideal | Sinc wrapper (PoC): worst block THD+N, steps |");
    println!("|---|---|---|---|---|---|---|");
    for freq in [997.0, 4_999.0, 9_997.0, 15_997.0] {
        let frames = stereo_sine(freq, 0.5, 44_100, 0, 44_100 * 2);
        let path = dir.join(format!("resample-{}.wav", freq as u32));
        write_wav16(&path, 44_100, &frames);
        let decoded = decode_all(&path);
        let file_thd = block_thd(&channel(&decoded, 0), 44_100.0, freq, 4_410);
        let (mixer, mut mixed) = rodio::mixer::mixer(TWO, nz_rate(48_000));
        mixer.add(rodio::buffer::SamplesBuffer::new(
            TWO,
            nz_rate(44_100),
            decoded.clone(),
        ));
        let one_span = channel(&pull(&mut mixed, 96_000 * 2), 0);
        let (mixer, mixed) = rodio::mixer::mixer(TWO, nz_rate(48_000));
        mixer.add(decoder(&path));
        let mut out = Vec::new();
        for s in mixed {
            out.push(s);
        }
        let l = channel(&out, 0);
        let thd = block_thd(&l, 48_000.0, freq, 4_800);
        let steps = phase_steps(&l, 48_000.0, freq, 480);
        let drift: f64 = steps.iter().map(|s| s.1).sum();
        // The same file through the windowed-sinc wrapper, then the 48 kHz mixer.
        let (mixer, mixed) = rodio::mixer::mixer(TWO, nz_rate(48_000));
        mixer.add(SincResampler::new(decoder(&path), 48_000));
        let mut sinc_out = Vec::new();
        for s in mixed {
            sinc_out.push(s);
        }
        let sl = channel(&sinc_out, 0);
        let sinc_thd = block_thd(&sl, 48_000.0, freq, 4_800);
        let sinc_steps = phase_steps(&sl, 48_000.0, freq, 480).len();
        println!(
            "| {:.0} Hz | {:.1} dB | {:.1} dB | {:.1} / {:.1} dB | {} steps, {:+.0} µs | {} frames ({:+}) | {:.1} dB, {} steps |",
            freq,
            file_thd.0,
            block_thd(&one_span, 48_000.0, freq, 4_800).0,
            thd.0,
            thd.1,
            steps.len(),
            drift,
            out.len() / 2,
            (out.len() / 2) as i64 - 96_000,
            sinc_thd.0,
            sinc_steps
        );
    }
    println!("
The sinc wrapper is {SINC_TAPS} taps per phase (Kaiser, β = 9, pass band to 20 kHz), written for this test only.");
    {
        // A 48 kHz file into the 48 kHz mixer: is the path transparent?
        let path = &pairs[3].a;
        let want = decode_all(path);
        let (mixer, mut mixed) = rodio::mixer::mixer(TWO, nz_rate(48_000));
        mixer.add(decoder(path));
        let got = pull(&mut mixed, want.len());
        println!(
            "\n- A 48 kHz file into the 48 kHz mixer: bit-exact = {} (no conversion when the rates match).",
            first_difference(&got, &want).is_none() && got.len() == want.len()
        );
    }

    println!("\n### A5. A Player connected to the mixer before anything is queued (BitNot's current setup)\n");
    {
        let path = &pairs[3].a; // 48 kHz stereo
        let want = decode_all(path);
        let (mixer, mut mixed) = rodio::mixer::mixer(TWO, nz_rate(48_000));
        let player = Player::connect_new(&mixer);
        player.append(decoder(path));
        let got = pull(&mut mixed, want.len());
        // The lag at which the output matches the file exactly.
        let lag = (0..2_000usize)
            .find(|&k| got[(k + 2_000) * 2..(k + 2_100) * 2] == want[2_000 * 2..2_100 * 2]);
        let misread = got
            .chunks(2)
            .take(600)
            .filter(|f| f[0] == f[1] && f[0] != 0.0)
            .count();
        println!("- When the Player connects, its queue is empty, so the mixer's UniformSourceIterator takes the empty");
        println!("  queue's format (mono) for the first span. Measured on a 48 kHz stereo file: {misread} of the first");
        println!("  600 output frames have identical left and right (the stereo samples read as mono, at half speed),");
        println!(
            "  and from then on the output equals the file {} frames late ({:.1} ms).",
            lag.map(|l| l.to_string()).unwrap_or_else(|| "?".into()),
            lag.unwrap_or(0) as f64 / 48.0
        );
        println!("- Appending before connecting (A3) or adding the queue to the mixer after the first append avoids it.");
        drop(player);
    }

    println!("\nThe current production engine does not append ahead: it detects the end on its 250 ms ticker and");
    println!(
        "then opens the next file. Its gap is therefore up to one tick plus the file-open time:"
    );
    for pair in pairs.iter().take(2) {
        let t = Instant::now();
        for _ in 0..20 {
            let _ = decoder(&pair.b);
        }
        println!(
            "- {}: opening a decoder took {:.2} ms on average (20 runs)",
            pair.name,
            t.elapsed().as_secs_f64() * 1000.0 / 20.0
        );
    }
}

/// Phase of `freq` per block of `block` frames; returns (frame, step in µs)
/// for every block-to-block phase change larger than 0.01 rad.
fn phase_steps(x: &[f64], rate: f64, freq: f64, block: usize) -> Vec<(usize, f64)> {
    let w = std::f64::consts::TAU * freq / rate;
    let blocks = x.len() / block;
    let phases: Vec<f64> = (1..blocks.saturating_sub(1))
        .map(|b| {
            let (mut s, mut c) = (0.0, 0.0);
            for i in b * block..(b + 1) * block {
                s += x[i] * (w * i as f64).sin();
                c += x[i] * (w * i as f64).cos();
            }
            c.atan2(s)
        })
        .collect();
    let mut steps = Vec::new();
    for k in 1..phases.len() {
        let mut d = phases[k] - phases[k - 1];
        while d > std::f64::consts::PI {
            d -= std::f64::consts::TAU;
        }
        while d < -std::f64::consts::PI {
            d += std::f64::consts::TAU;
        }
        if d.abs() > 0.01 {
            steps.push(((k + 1) * block, d / std::f64::consts::TAU / freq * 1e6));
        }
    }
    steps
}

/// THD+N per block (skipping the first and last block): (worst, median) in dB.
fn block_thd(x: &[f64], rate: f64, freq: f64, block: usize) -> (f64, f64) {
    let mut v: Vec<f64> = x
        .chunks(block)
        .skip(1)
        .filter(|c| c.len() == block)
        .map(|c| thd_n_db(c, rate, freq))
        .collect();
    v.pop();
    v.sort_by(f64::total_cmp);
    (v[v.len() - 1], v[v.len() / 2])
}

// ── B. Equaliser ───────────────────────────────────────────────────────────

fn run_eq<F: Filter>(
    input: &Frames,
    rate: f64,
    glide_ms: f64,
    changes: &[(usize, [f64; 10])],
) -> Vec<f32> {
    let mut eq: Equalizer<F> = Equalizer::new(rate, glide_ms);
    let mut out = Vec::with_capacity(input.len() * 2);
    let mut next = 0;
    for (n, f) in input.iter().enumerate() {
        while next < changes.len() && changes[next].0 == n {
            eq.set_gains(changes[next].1);
            next += 1;
        }
        let mut frame = [f[0] as f32, f[1] as f32];
        eq.process(&mut frame);
        out.extend_from_slice(&frame);
    }
    out
}

fn gains_at(band: usize, db: f64) -> [f64; 10] {
    let mut g = [0.0; 10];
    g[band] = db;
    g
}

/// 4th-order high-pass (two RBJ biquads, Q = 0.707).
fn high_passed(x: &[f64], rate: f64, cutoff: f64) -> Vec<f64> {
    let w0 = std::f64::consts::TAU * cutoff / rate;
    let (sin, cos) = w0.sin_cos();
    let alpha = sin / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
    let a0 = 1.0 + alpha;
    let (b0, b1, b2) = (
        (1.0 + cos) / 2.0 / a0,
        -(1.0 + cos) / a0,
        (1.0 + cos) / 2.0 / a0,
    );
    let (a1, a2) = (-2.0 * cos / a0, (1.0 - alpha) / a0);
    let mut y = x.to_vec();
    for _ in 0..2 {
        let (mut s1, mut s2) = (0.0, 0.0);
        for v in y.iter_mut() {
            let input = *v;
            let out = b0 * input + s1;
            s1 = b1 * input - a1 * out + s2;
            s2 = b2 * input - a2 * out;
            *v = out;
        }
    }
    y
}

fn peak_in(x: &[f64], range: std::ops::Range<usize>) -> f64 {
    x[range.start.min(x.len())..range.end.min(x.len())]
        .iter()
        .fold(0.0f64, |m, v| m.max(v.abs()))
}

pub fn equalizer() {
    heading("B. 10-band equalizer");
    println!("Bands (Hz): {:?}. Band 1 is a low shelf, band 10 a high shelf, the rest peaking filters, Q = √2", BANDS);
    println!("(one octave). Two implementations: RBJ cookbook biquad (transposed direct form II, f64 state) and");
    println!("Simper's linear trapezoidal SVF. A band at exactly 0 dB is bypassed. Glide: gains move at most");
    println!("24 dB per 30 ms, with coefficients recomputed every 16 frames. All at 48 kHz.\n");
    let rate = 48_000.0;

    println!(
        "### B1. Magnitude response (sine probes at the band centres, measured after settling)\n"
    );
    let configs: [(&str, [f64; 10]); 4] = [
        (
            "Bass +6 (bands 1-3)",
            [6.0, 6.0, 6.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        ),
        (
            "V shape",
            [6.0, 4.0, 2.0, 0.0, -2.0, -2.0, 0.0, 2.0, 4.0, 6.0],
        ),
        ("1 kHz +12", gains_at(5, 12.0)),
        ("All -12", [-12.0; 10]),
    ];
    println!("| Configuration | Filter | Measured gain at each centre (dB) |");
    println!("|---|---|---|");
    for (name, gains) in &configs {
        for filter in ["biquad", "svf"] {
            let measured: Vec<String> = BANDS
                .iter()
                .map(|&f| {
                    let probe = stereo_sine(f, 0.1, 48_000, 0, 48_000);
                    let changes = [(0, *gains)];
                    let out = if filter == "biquad" {
                        run_eq::<Biquad>(&probe, rate, 0.0, &changes)
                    } else {
                        run_eq::<Svf>(&probe, rate, 0.0, &changes)
                    };
                    let l = channel(&out, 0);
                    format!("{:+.2}", db(tone_amplitude(&l[24_000..], rate, f) / 0.1))
                })
                .collect();
            println!("| {name} | {filter} | {} |", measured.join(" "));
        }
    }

    println!("\n### B2. Transparency when flat\n");
    let program = noise(7, 0.3, 48_000 * 2);
    let input: Vec<f32> = program
        .iter()
        .flat_map(|f| [f[0] as f32, f[1] as f32])
        .collect();
    let flat = run_eq::<Biquad>(&program, rate, 30.0, &[]);
    let changed = flat
        .iter()
        .zip(&input)
        .filter(|(a, b)| a.to_bits() != b.to_bits())
        .count();
    println!(
        "- Flat EQ (all bands bypassed): {changed} of {} samples changed (0 = bit-exact).",
        input.len()
    );
    for filter in ["biquad", "svf"] {
        let mut changed = 0;
        let mut worst = 0.0f64;
        let (mut b, mut s): (Equalizer<Biquad>, Equalizer<Svf>) =
            (Equalizer::new(rate, 0.0), Equalizer::new(rate, 0.0));
        for f in &program {
            let mut frame = [f[0] as f32, f[1] as f32];
            let original = frame;
            if filter == "biquad" {
                b.process_unbypassed(&mut frame)
            } else {
                s.process_unbypassed(&mut frame)
            }
            for ch in 0..2 {
                if frame[ch].to_bits() != original[ch].to_bits() {
                    changed += 1;
                    worst = worst.max((frame[ch] - original[ch]).abs() as f64);
                }
            }
        }
        println!("- Flat EQ with all 10 {filter} filters running at 0 dB: {changed} samples changed, largest change {worst:.2e}.");
    }
    let back = run_eq::<Biquad>(
        &program,
        rate,
        30.0,
        &[(10_000, gains_at(5, 6.0)), (40_000, [0.0; 10])],
    );
    let after = back[(40_000 + 48_000 / 10) * 2..]
        .iter()
        .zip(&input[(40_000 + 48_000 / 10) * 2..])
        .filter(|(a, b)| a.to_bits() != b.to_bits())
        .count();
    println!("- After gliding a band up and back to 0 dB, 100 ms later: {after} samples changed (bypass re-engages).");

    println!("\n### B3. Clicks when a gain changes during playback\n");
    println!("Method: a steady sine at -12 dBFS; one band jumps 0 → +12 dB and back. A click is broadband energy,");
    println!("so the output is high-passed (4th order at 4× the tone, at least 2 kHz) and its peak is reported in");
    println!("dB relative to the tone's amplitude: during the changes, and in the steady state for reference (the");
    println!("tone's own leakage through the high-pass). Lower is cleaner.\n");
    println!("| Signal | Filter | Steady (reference) | Immediate change | 30 ms glide |");
    println!("|---|---|---|---|---|");
    for (label, freq, band) in [
        ("1 kHz sine, 1 kHz band", 1000.0, 5usize),
        ("100 Hz sine, 125 Hz band", 100.0, 2usize),
    ] {
        let signal = stereo_sine(freq, 0.25, 48_000, 0, 72_000);
        let changes = [(24_000, gains_at(band, 12.0)), (48_000, [0.0; 10])];
        for filter in ["biquad", "svf"] {
            let mut cells = Vec::new();
            let mut steady = 0.0f64;
            for glide in [0.0, 30.0] {
                let out = if filter == "biquad" {
                    run_eq::<Biquad>(&signal, rate, glide, &changes)
                } else {
                    run_eq::<Svf>(&signal, rate, glide, &changes)
                };
                let hp = high_passed(&channel(&out, 0), rate, (4.0 * freq).max(2_000.0));
                let w = 960 + (glide * 48.0) as usize;
                steady = steady
                    .max(peak_in(&hp, 6_000..23_000))
                    .max(peak_in(&hp, 36_000..47_000));
                cells.push(peak_in(&hp, 24_000..24_000 + w).max(peak_in(&hp, 48_000..48_000 + w)));
            }
            println!(
                "| {label} | {filter} | {:.1} dB | {:.1} dB | {:.1} dB |",
                db(steady / 0.25),
                db(cells[0] / 0.25),
                db(cells[1] / 0.25)
            );
        }
    }
    // Rapid "slider drag": a new gain every 16 ms for one second.
    let signal = stereo_sine(1000.0, 0.25, 48_000, 0, 72_000);
    let mut changes = Vec::new();
    let mut seed = 99u64;
    for k in 0..60 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let g = ((seed >> 33) as f64 / (1u64 << 31) as f64) * 24.0 - 12.0;
        changes.push((12_000 + k * 768, gains_at(5, (g * 2.0).round() / 2.0)));
    }
    for filter in ["biquad", "svf"] {
        let mut row = Vec::new();
        let mut steady = 0.0f64;
        for glide in [0.0, 30.0] {
            let out = if filter == "biquad" {
                run_eq::<Biquad>(&signal, rate, glide, &changes)
            } else {
                run_eq::<Svf>(&signal, rate, glide, &changes)
            };
            let hp = high_passed(&channel(&out, 0), rate, 4_000.0);
            steady = steady.max(peak_in(&hp, 4_000..11_000));
            row.push(peak_in(&hp, 12_000..12_000 + 60 * 768));
        }
        println!("| 1 kHz sine, gain changed every 16 ms (±12 dB) | {filter} | {:.1} dB | {:.1} dB | {:.1} dB |", db(steady / 0.25), db(row[0] / 0.25), db(row[1] / 0.25));
    }

    println!("\nGlide variants on the hardest case (100 Hz sine, 125 Hz band, 0 → +12 → 0 dB):\n");
    println!("| Filter | Glide | Coefficient update | Click level (dB re tone) |");
    println!("|---|---|---|---|");
    let signal = stereo_sine(100.0, 0.25, 48_000, 0, 96_000);
    let changes = [(24_000usize, gains_at(2, 12.0)), (60_000, [0.0; 10])];
    for (filter, glide, block) in [
        ("biquad", 30.0, 16),
        ("biquad", 30.0, 1),
        ("biquad", 100.0, 16),
        ("biquad", 100.0, 1),
        ("svf", 30.0, 16),
        ("svf", 30.0, 1),
        ("svf", 100.0, 16),
        ("svf", 100.0, 1),
    ] {
        let mut out = Vec::new();
        let (mut b, mut v): (Equalizer<Biquad>, Equalizer<Svf>) = (
            Equalizer::with_block(rate, glide, block),
            Equalizer::with_block(rate, glide, block),
        );
        for (n, f) in signal.iter().enumerate() {
            for (at, g) in &changes {
                if *at == n {
                    if filter == "biquad" {
                        b.set_gains(*g)
                    } else {
                        v.set_gains(*g)
                    }
                }
            }
            let mut frame = [f[0] as f32, f[1] as f32];
            if filter == "biquad" {
                b.process(&mut frame)
            } else {
                v.process(&mut frame)
            }
            out.extend_from_slice(&frame);
        }
        let hp = high_passed(&channel(&out, 0), rate, 2_000.0);
        let w = 960 + (glide * 48.0) as usize;
        let click = peak_in(&hp, 24_000..24_000 + w).max(peak_in(&hp, 60_000..60_000 + w));
        println!(
            "| {filter} | {glide:.0} ms | every {block} frame{} | {:.1} dB |",
            if block == 1 { "" } else { "s" },
            db(click / 0.25)
        );
    }

    println!("\n### B4. Clipping, pre-amp, and the limiter\n");
    let raw = noise(11, 0.3, 48_000 * 3);
    let raw_peak = raw
        .iter()
        .flat_map(|f| [f[0].abs(), f[1].abs()])
        .fold(0.0, f64::max);
    // Normalised to a -3 dBFS peak, like mastered music.
    let scale = 10f64.powf(-3.0 / 20.0) / raw_peak;
    let program: Frames = raw.iter().map(|f| [f[0] * scale, f[1] * scale]).collect();
    let peak_in = program
        .iter()
        .flat_map(|f| [f[0].abs(), f[1].abs()])
        .fold(0.0, f64::max);
    let boost = [12.0, 12.0, 6.0, 0.0, 0.0, 0.0, 0.0, 3.0, 12.0, 12.0];
    let boosted = run_eq::<Biquad>(&program, rate, 0.0, &[(0, boost)]);
    let peak = |x: &[f32]| x.iter().fold(0.0f32, |m, s| m.max(s.abs())) as f64;
    let overs = |x: &[f32]| x.iter().filter(|s| s.abs() > 1.0).count();
    println!(
        "- Program: noise with a music-like tilt, peak {:.1} dBFS. EQ: bass and treble +12 dB.",
        db(peak_in)
    );
    println!(
        "- Without protection: peak {:.1} dBFS, {} samples above full scale (would clip).",
        db(peak(&boosted)),
        overs(&boosted)
    );
    let preamp: Vec<f32> = boosted
        .iter()
        .map(|s| s * 10f32.powf(-12.0 / 20.0))
        .collect();
    println!(
        "- With a -12 dB pre-amp (the largest boost): peak {:.1} dBFS, {} over.",
        db(peak(&preamp)),
        overs(&preamp)
    );
    let mut limiter = Limiter::new(-1.0, 50.0, rate);
    let mut limited = boosted.clone();
    for f in limited.chunks_mut(2) {
        let mut frame = [f[0], f[1]];
        limiter.process(&mut frame);
        f.copy_from_slice(&frame);
    }
    println!(
        "- With the peak limiter (threshold -1 dBFS, 50 ms release): peak {:.2} dBFS, {} over. Gain reduced by more than 0.1 dB on {:.1}% of frames, by more than 1 dB on {:.2}%; deepest reduction {:.1} dB.",
        db(peak(&limited)),
        overs(&limited),
        limiter.over_tenth_db as f64 * 100.0 / program.len() as f64,
        limiter.over_one_db as f64 * 100.0 / program.len() as f64,
        db(limiter.min_gain)
    );
    let quiet: Vec<f32> = program
        .iter()
        .flat_map(|f| [f[0] as f32, f[1] as f32])
        .collect();
    let mut limiter = Limiter::new(-1.0, 50.0, rate);
    let mut through = quiet.clone();
    for f in through.chunks_mut(2) {
        let mut frame = [f[0], f[1]];
        limiter.process(&mut frame);
        f.copy_from_slice(&frame);
    }
    let changed = through
        .iter()
        .zip(&quiet)
        .filter(|(a, b)| a.to_bits() != b.to_bits())
        .count();
    println!("- Same limiter on audio that never reaches the threshold: {changed} samples changed (0 = untouched).");
    let buffer = rodio::buffer::SamplesBuffer::new(TWO, nz_rate(48_000), quiet.clone());
    let rodio_limited: Vec<f32> = buffer.limit(LimitSettings::default()).collect();
    let changed = rodio_limited
        .iter()
        .zip(&quiet)
        .filter(|(a, b)| a.to_bits() != b.to_bits())
        .count();
    let worst = rodio_limited
        .iter()
        .zip(&quiet)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    println!("- rodio's built-in `limit(LimitSettings::default())` on the same quiet audio: {changed} samples changed, largest change {:.1} dB below full scale.", db(worst as f64));
}

// ── C. Crossfade ───────────────────────────────────────────────────────────

pub fn crossfade(dir: &Path) {
    heading("C. Crossfade");
    println!("Method: a rodio Mixer at 48 kHz (as on the device). Deck A plays from 0 s; deck B is added with");
    println!("`delay(4 s)`. A falls and B rises over 4 s (4 s to 8 s). Gains are applied per frame by a wrapper");
    println!("around each decoder. Loudness is the stereo RMS per 50 ms window; the steady level is the mean of");
    println!("the 0.5 to 3.5 s windows.\n");
    let rate = 48_000u32;
    let fade = 4 * rate as u64;
    let cases: [(&str, Frames, Frames); 2] = [
        (
            "Uncorrelated (two noise programs)",
            noise(21, 0.25, 12 * rate as usize),
            noise(22, 0.25, 12 * rate as usize),
        ),
        (
            "Correlated (identical sine on both decks)",
            stereo_sine(440.0, 0.25, rate, 0, 12 * rate as usize),
            stereo_sine(440.0, 0.25, rate, 4 * rate as usize, 12 * rate as usize),
        ),
    ];
    println!("| Signal | Curve | Lowest level during fade | Highest level during fade | Spike at start / mid / end ÷ body |");
    println!("|---|---|---|---|---|");
    for (index, (label, a, b)) in cases.iter().enumerate() {
        let pa = dir.join(format!("fade-{index}-a.wav"));
        let pb = dir.join(format!("fade-{index}-b.wav"));
        write_wav16(&pa, rate, a);
        write_wav16(&pb, rate, b);
        for curve in [Curve::Linear, Curve::EqualPower] {
            let (mixer, mut mixed) = rodio::mixer::mixer(TWO, nz_rate(rate));
            let mut out_fade = Fade::new(curve, false, fade, fade);
            mixer.add(Processed::new(decoder(&pa), move |f: &mut [f32; 2]| {
                out_fade.process(f)
            }));
            let mut in_fade = Fade::new(curve, true, 0, fade);
            mixer.add(
                Processed::new(decoder(&pb), move |f: &mut [f32; 2]| in_fade.process(f))
                    .delay(Duration::from_secs(4)),
            );
            let got = pull(&mut mixed, 10 * rate as usize * 2);
            let levels = windowed_rms_db(&got, rate as usize / 20);
            let steady: f64 = levels[10..70].iter().sum::<f64>() / 60.0;
            let during = &levels[80..160];
            let lo = during.iter().cloned().fold(f64::MAX, f64::min) - steady;
            let hi = during.iter().cloned().fold(f64::MIN, f64::max) - steady;
            let l = channel(&got, 0);
            let body = click_peak(&l, 10_000..150_000);
            let s = |at: u64| click_peak(&l, at as usize - 48..at as usize + 48) / body;
            println!(
                "| {label} | {:?} | {:+.2} dB | {:+.2} dB | {:.2} / {:.2} / {:.2} |",
                curve,
                lo,
                hi,
                s(fade),
                s(fade + fade / 2),
                s(2 * fade)
            );
        }
    }

    println!("\n### C2. Position of the incoming deck and the handover point\n");
    let pa = dir.join("fade-0-a.wav");
    let pb = dir.join("fade-0-b.wav");
    let (mixer, mut mixed) = rodio::mixer::mixer(TWO, nz_rate(rate));
    mixer.add(decoder(&pa));
    let counted = Arc::new(AtomicU64::new(0));
    let reported = Arc::new(AtomicU64::new(0));
    let (c, r) = (counted.clone(), reported.clone());
    let deck_b = Processed::new(decoder(&pb), move |_: &mut [f32; 2]| {
        c.fetch_add(1, Ordering::Relaxed);
    })
    .track_position()
    .periodic_access(Duration::from_millis(5), move |src| {
        r.store(src.get_pos().as_micros() as u64, Ordering::Relaxed);
    })
    .delay(Duration::from_secs(4));
    mixer.add(deck_b);
    let midpoint = 6 * rate as usize;
    let _ = pull(&mut mixed, midpoint * 2);
    let frames_b = counted.load(Ordering::Relaxed);
    println!("- At the fade midpoint (6.000 s of mixer output), deck B had produced {frames_b} frames = {:.6} s (expected 2.000000 s).", frames_b as f64 / rate as f64);
    println!(
        "- rodio's `track_position()` for deck B, read through `periodic_access(5 ms)` as Player does: {:.3} s (updates every 5 ms).",
        reported.load(Ordering::Relaxed) as f64 / 1e6
    );
    println!("- A frame counter in the deck's own wrapper is exact to the frame; this is where \"current track\" can switch.");

    println!("\n### C3. rodio's built-in `take_crossfade_with`\n");
    let a = rodio::buffer::SamplesBuffer::new(
        TWO,
        nz_rate(rate),
        as_decoded(&noise(21, 0.25, 6 * rate as usize), 32),
    );
    let b = rodio::buffer::SamplesBuffer::new(
        TWO,
        nz_rate(rate),
        as_decoded(&noise(22, 0.25, 6 * rate as usize), 32),
    );
    let fade_out: Vec<f32> = a.take_crossfade_with(b, Duration::from_secs(4)).collect();
    let levels = windowed_rms_db(&fade_out, rate as usize / 20);
    let steady = levels[0];
    let lo = levels[..80].iter().cloned().fold(f64::MAX, f64::min) - steady;
    println!("- Uncorrelated noise, 4 s: lowest level {lo:+.2} dB against the start, i.e. a linear crossfade (it also ends both sources at the fade end).");
}

// ── D. Visualizer capture ──────────────────────────────────────────────────

pub fn capture() {
    heading("D. Visualizer sample capture");
    println!("Method: a wrapper copies each frame into a lock-free single-producer/single-consumer ring of");
    println!("2^16 samples. The audio side never waits: if the reader falls behind, new samples are dropped and");
    println!("counted. A reader thread drains it at 60 Hz, as a visualizer would.\n");
    let rate = 48_000u32;
    let tones = two_tones(1_000.0, 5_000.0, 0.5, rate, rate as usize);
    let data: Vec<f32> = tones
        .iter()
        .flat_map(|f| [f[0] as f32, f[1] as f32])
        .collect();
    let origin = Instant::now();
    let ring = Ring::new(1 << 16, origin);
    let writer = ring.clone();
    let mut source = Processed::new(
        rodio::buffer::SamplesBuffer::new(TWO, nz_rate(rate), data.clone()),
        move |f: &mut [f32; 2]| writer.push_frame(f),
    );
    let played = pull(&mut source, 8192);
    let mut captured = Vec::new();
    ring.drain(&mut captured);
    let identical = captured
        .iter()
        .zip(&played)
        .all(|(a, b)| a.to_bits() == b.to_bits())
        && captured.len() == played.len();
    println!(
        "- Captured samples identical to the samples played: {identical} ({} samples).",
        captured.len()
    );
    let l = channel(&captured, 0);
    let spec = spectrum(&l, 2048);
    let mut peaks: Vec<(usize, f64)> = spec.iter().cloned().enumerate().collect();
    peaks.sort_by(|a, b| b.1.total_cmp(&a.1));
    let bin_hz = rate as f64 / 2048.0;
    let mut found: Vec<f64> = peaks
        .iter()
        .take(8)
        .filter(|p| p.1 > 0.05)
        .map(|p| p.0 as f64 * bin_hz)
        .collect();
    found.sort_by(f64::total_cmp);
    found.dedup_by(|a, b| (*a - *b).abs() < 3.0 * bin_hz);
    println!("- 2048-point FFT of the capture finds peaks near {:?} Hz (bin width {:.1} Hz): enough for frequency bars.", found.iter().map(|f| f.round()).collect::<Vec<_>>(), bin_hz);
    let waveform: Vec<(f32, f32)> = captured
        .chunks(1024)
        .take(4)
        .map(|c| {
            c.iter()
                .fold((f32::MAX, f32::MIN), |(lo, hi), &s| (lo.min(s), hi.max(s)))
        })
        .collect();
    println!(
        "- Waveform min/max per 512 frames (first four): {:?}",
        waveform
            .iter()
            .map(|(a, b)| format!("{a:.2}..{b:.2}"))
            .collect::<Vec<_>>()
    );

    // A stalled reader must not hold up the audio side.
    let ring = Ring::new(1 << 12, origin);
    let writer = ring.clone();
    let mut source = Processed::new(
        rodio::buffer::SamplesBuffer::new(TWO, nz_rate(rate), data),
        move |f: &mut [f32; 2]| writer.push_frame(f),
    );
    let t = Instant::now();
    let n = pull(&mut source, rate as usize * 2).len();
    let elapsed = t.elapsed();
    println!(
        "- With no reader at all (ring full after 2048 frames): {} samples produced in {:.2} ms, {} dropped, none waited.",
        n,
        elapsed.as_secs_f64() * 1000.0,
        ring.dropped.load(Ordering::Relaxed)
    );
    println!("- Latency and the device thread are measured in the device run (section F).");
}

// ── E. CPU ─────────────────────────────────────────────────────────────────

const CPU_SECONDS: usize = 60;

fn cpu_files(dir: &Path) -> (PathBuf, PathBuf) {
    let a = dir.join("cpu-a.flac");
    let b = dir.join("cpu-b.wav");
    if !a.exists() || !b.exists() {
        write_flac(&a, 44_100, 16, &noise(31, 0.25, 44_100 * CPU_SECONDS));
        write_wav16(&b, 44_100, &noise(32, 0.25, 44_100 * CPU_SECONDS));
    }
    (a, b)
}

fn v_shape() -> [f64; 10] {
    [6.0, 4.0, 2.0, 1.0, -2.0, -2.0, 1.0, 2.0, 4.0, 6.0]
}

fn eq_stage<F: Filter + 'static>(rate: u32) -> impl FnMut(&mut [f32; 2]) + Send {
    let mut eq: Equalizer<F> = Equalizer::new(rate as f64, 30.0);
    eq.set_gains(v_shape());
    move |f: &mut [f32; 2]| eq.process(f)
}

/// Builds a configuration on a 48 kHz mixer and returns its output.
fn build(
    config: &str,
    a: &Path,
    b: &Path,
    ring: &Arc<Ring>,
) -> Box<dyn Iterator<Item = f32> + Send> {
    let rate = 48_000u32;
    let (mixer, mixed) = rodio::mixer::mixer(TWO, nz_rate(rate));
    let player = Player::connect_new(&mixer);
    let fade = 3 * 44_100u64;
    match config {
        "Playback (decode + Player + 44.1→48 kHz)" => player.append(decoder(a)),
        "+ EQ, 10 biquads" => player.append(Processed::new(decoder(a), eq_stage::<Biquad>(44_100))),
        "+ EQ, 10 SVFs" => player.append(Processed::new(decoder(a), eq_stage::<Svf>(44_100))),
        "+ sinc resampler to 48 kHz (replaces rodio's conversion)" => {
            player.append(SincResampler::new(decoder(a), 48_000))
        }
        "+ EQ, 10 SVFs gliding all the time, coefficients every frame (worst case)" => {
            let mut eq: Equalizer<Svf> = Equalizer::with_block(44_100.0, 30.0, 1);
            let mut n = 0u64;
            player.append(Processed::new(decoder(a), move |f: &mut [f32; 2]| {
                // A new target every 20 ms keeps every band moving.
                if n.is_multiple_of(882) {
                    let sign = if (n / 882).is_multiple_of(2) {
                        1.0
                    } else {
                        -1.0
                    };
                    eq.set_gains(v_shape().map(|g| g * sign + sign * 3.0));
                }
                n += 1;
                eq.process(f)
            }));
        }
        "Crossfade, two decoders" | "All: crossfade + EQ on both + limiter + capture" => {
            let with_eq = config.starts_with("All");
            let mut out_fade = Fade::new(Curve::EqualPower, false, 0, fade);
            let mut in_fade = Fade::new(Curve::EqualPower, true, 0, fade);
            let mut eq_a = eq_stage::<Biquad>(44_100);
            let mut eq_b = eq_stage::<Biquad>(44_100);
            player.append(Processed::new(decoder(a), move |f: &mut [f32; 2]| {
                if with_eq {
                    eq_a(f)
                }
                out_fade.process(f)
            }));
            mixer.add(Processed::new(decoder(b), move |f: &mut [f32; 2]| {
                if with_eq {
                    eq_b(f)
                }
                in_fade.process(f)
            }));
        }
        "+ visualizer capture" => {
            let writer = ring.clone();
            player.append(Processed::new(decoder(a), move |f: &mut [f32; 2]| {
                writer.push_frame(f)
            }));
        }
        _ => unreachable!(),
    }
    std::mem::forget(player); // keep the queue alive while it is pulled
    if config.starts_with("All") {
        let writer = ring.clone();
        let mut limiter = Limiter::new(-1.0, 50.0, rate as f64);
        Box::new(Processed::new(mixed, move |f: &mut [f32; 2]| {
            limiter.process(f);
            writer.push_frame(f);
        }))
    } else {
        Box::new(mixed)
    }
}

const CONFIGS: [&str; 8] = [
    "Playback (decode + Player + 44.1→48 kHz)",
    "+ sinc resampler to 48 kHz (replaces rodio's conversion)",
    "+ EQ, 10 biquads",
    "+ EQ, 10 SVFs",
    "+ EQ, 10 SVFs gliding all the time, coefficients every frame (worst case)",
    "Crossfade, two decoders",
    "+ visualizer capture",
    "All: crossfade + EQ on both + limiter + capture",
];

pub fn cpu(dir: &Path) {
    heading("E. CPU cost (offline, one thread, as fast as possible)");
    let (a, b) = cpu_files(dir);
    println!("Method: {CPU_SECONDS} s of 44.1 kHz stereo (FLAC deck A, WAV deck B) pulled through a 48 kHz rodio Mixer");
    println!("as fast as possible on one thread; 5 runs, median shown. EQ: all ten bands active (V shape).");
    println!(
        "\"Cost\" is processing time per second of audio; 1000 ms would be exactly real time.\n"
    );
    println!(
        "| Configuration | Cost (ms per s of audio) | Faster than real time | Spread of 5 runs |"
    );
    println!("|---|---|---|---|");
    for config in CONFIGS {
        let mut times = Vec::new();
        for _ in 0..5 {
            let ring = Ring::new(1 << 16, Instant::now());
            let stop = Arc::new(AtomicBool::new(false));
            let (r, s) = (ring.clone(), stop.clone());
            let reader = std::thread::spawn(move || {
                let mut sink = Vec::with_capacity(1 << 16);
                while !s.load(Ordering::Relaxed) {
                    sink.clear();
                    r.drain(&mut sink);
                    std::thread::sleep(Duration::from_millis(16));
                }
            });
            let mut source = build(config, &a, &b, &ring);
            let samples = (CPU_SECONDS - 4) * 48_000 * 2;
            let t = Instant::now();
            let mut sum = 0.0f32;
            for _ in 0..samples {
                sum += source.next().unwrap_or(0.0);
            }
            times.push(t.elapsed().as_secs_f64() / (CPU_SECONDS - 4) as f64 * 1000.0);
            std::hint::black_box(sum);
            stop.store(true, Ordering::Relaxed);
            reader.join().ok();
        }
        times.sort_by(f64::total_cmp);
        let median = times[2];
        println!(
            "| {config} | {median:.2} | {:.0}× | {:.2}–{:.2} |",
            1000.0 / median,
            times[0],
            times[4]
        );
    }
}

// ── F. On the device (rodio) ───────────────────────────────────────────────

fn print_stats(
    label: &str,
    stats: &Arc<Mutex<crate::dsp::CallbackStats>>,
    rate: u32,
    channels: u16,
) {
    let s = stats.lock().unwrap();
    let samples_per_ms = rate as f64 * channels as f64 / 1000.0;
    let mean_busy = s.total_busy_ns as f64 / s.callbacks.max(1) as f64 / 1e6;
    let mean_burst_ms = s.samples as f64 / s.callbacks.max(1) as f64 / samples_per_ms;
    println!(
        "| {label} | {} | {:.2} ms | {:.3} ms | {:.3} ms | {:.1}–{:.1} ms | {} |",
        s.callbacks,
        mean_burst_ms,
        mean_busy,
        s.max_busy_ns as f64 / 1e6,
        s.min_gap_ns as f64 / 1e6,
        s.max_gap_ns as f64 / 1e6,
        s.missed
    );
}

pub fn device(dir: &Path) {
    heading("F. On the real output device through rodio (Option A)");
    let mut sink = match rodio::DeviceSinkBuilder::open_default_sink() {
        Ok(sink) => sink,
        Err(e) => {
            println!("Could not open the device: {e}. Skipped.");
            return;
        }
    };
    sink.log_on_drop(false);
    let config = sink.config();
    let (rate, channels) = (config.sample_rate().get(), config.channel_count().get());
    println!(
        "Device sink as BitNot opens it: {} Hz, {} channels, {:?}, buffer {:?}. Output is {}.",
        rate,
        channels,
        config.sample_format(),
        config.buffer_size(),
        if audible() {
            "audible"
        } else {
            "muted after measurement (multiplied by 0)"
        }
    );
    let gain = if audible() { 0.2f32 } else { 0.0 };
    println!("\nMethod: the device thread pulls the chain; a tap times each pull burst (one device callback).");
    println!("\"Missed\" counts bursts whose production took longer than the audio they held.\n");
    println!("| Run | Callbacks | Audio per callback | Mean time to produce | Longest | Interval between callbacks | Missed |");
    println!("|---|---|---|---|---|---|---|");

    // F1. Gapless on the device: the samples the device thread actually pulled.
    let pairs = make_pairs(dir);
    let pair = &pairs[1]; // FLAC 16-bit 44.1 kHz
    let (player, queue) = Player::new();
    player.append(decoder(&pair.a));
    player.append(decoder(&pair.b));
    let total =
        (decode_all(&pair.a).len() + decode_all(&pair.b).len()) as f64 / 2.0 / pair.rate_a as f64;
    let ring = Ring::new(1 << 19, Instant::now());
    let writer = ring.clone();
    let uniform = UniformSourceIterator::new(queue, TWO, nz_rate(rate));
    let tapped = Processed::new(uniform, move |f: &mut [f32; 2]| {
        writer.push_frame(f);
        f[0] *= gain;
        f[1] *= gain;
    });
    let (timed, stats) = TimingTap::new(tapped);
    sink.mixer().add(timed);
    std::thread::sleep(Duration::from_secs_f64(total + 0.5));
    print_stats(
        "Gapless pair (FLAC 44.1 kHz, resampled to device rate)",
        &stats,
        rate,
        2,
    );
    let mut captured = Vec::new();
    ring.drain(&mut captured);
    // The same chain pulled offline, for comparison.
    let (player2, queue2) = Player::new();
    player2.append(decoder(&pair.a));
    player2.append(decoder(&pair.b));
    let offline: Vec<f32> = pull(
        &mut UniformSourceIterator::new(queue2, TWO, nz_rate(rate)),
        captured.len(),
    );
    let first = first_difference(&captured, &offline);
    drop(player);

    // F2. Everything at once for 20 s.
    let (a, b) = cpu_files(dir);
    let ring2 = Ring::new(1 << 16, Instant::now());
    let source = build(
        "All: crossfade + EQ on both + limiter + capture",
        &a,
        &b,
        &ring2,
    );
    let latest = Arc::new(Mutex::new(Vec::<f64>::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let (r, s, lat) = (ring2.clone(), stop.clone(), latest.clone());
    let origin = Instant::now();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::with_capacity(1 << 16);
        while !s.load(Ordering::Relaxed) {
            buf.clear();
            if r.drain(&mut buf) > 0 {
                let written = r.last_write_ns.load(Ordering::Relaxed) as f64;
                let now = origin.elapsed().as_nanos() as f64;
                lat.lock().unwrap().push((now - written) / 1e6);
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    });
    let stamp_ring = ring2.clone();
    let mut counter = 0u32;
    let stamped = Processed::new(
        SourceAdapter(source, nz_rate(48_000)),
        move |f: &mut [f32; 2]| {
            counter = counter.wrapping_add(1);
            if counter.is_multiple_of(64) {
                stamp_ring.stamp();
            }
            f[0] *= gain;
            f[1] *= gain;
        },
    );
    let (timed, stats2) = TimingTap::new(UniformSourceIterator::new(stamped, TWO, nz_rate(rate)));
    sink.mixer().add(timed);
    std::thread::sleep(Duration::from_secs(20));
    stop.store(true, Ordering::Relaxed);
    reader.join().ok();
    print_stats(
        "All features, 20 s (crossfade, 2× EQ, limiter, capture)",
        &stats2,
        rate,
        2,
    );

    println!();
    println!(
        "- Gapless on the device: {} samples captured from the device thread; identical to the same chain pulled offline: {}.",
        captured.len(),
        match first {
            None => "yes (no sample lost, added, or changed)".to_string(),
            Some(at) => format!("no, first difference at {at}"),
        }
    );
    let lat = latest.lock().unwrap();
    let mut sorted = lat.clone();
    sorted.sort_by(f64::total_cmp);
    if !sorted.is_empty() {
        println!(
            "- Visualizer ring latency (newest captured sample to reader, reader at 60 Hz): median {:.1} ms, 95th percentile {:.1} ms, max {:.1} ms; {} samples dropped.",
            sorted[sorted.len() / 2],
            sorted[sorted.len() * 95 / 100],
            sorted[sorted.len() - 1],
            ring2.dropped.load(Ordering::Relaxed)
        );
    }
    let buffer_ms = match config.buffer_size() {
        rodio::cpal::BufferSize::Fixed(n) => *n as f64 / rate as f64 * 1000.0,
        _ => f64::NAN,
    };
    println!("- Requested device buffer: {buffer_ms:.1} ms. Audio reaches the speakers about this much after it is captured (not measurable through rodio; see G).");
}

/// Lets a boxed iterator be used as a stereo rodio Source.
struct SourceAdapter(Box<dyn Iterator<Item = f32> + Send>, NonZero<u32>);

impl Iterator for SourceAdapter {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.0.next()
    }
}

impl Source for SourceAdapter {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> NonZero<u16> {
        TWO
    }
    fn sample_rate(&self) -> NonZero<u32> {
        self.1
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

// ── G. Minimal direct cpal pipeline (Option B comparison) ──────────────────

pub fn raw_cpal(dir: &Path) {
    heading("G. Minimal direct cpal output (Option B, for comparison)");
    println!("Method: a cpal output stream opened directly at the device's default shared-mode config. Its");
    println!("callback concatenates two 48 kHz decoders (rodio's Symphonia decoder, no Player, no Mixer) and");
    println!("runs the 10-band EQ, so the transition is sample-exact by construction. cpal's callback timestamps");
    println!("give the output latency reported by WASAPI. 10 s.\n");
    let host = rodio::cpal::default_host();
    let Some(device) = host.default_output_device() else {
        println!("No output device. Skipped.");
        return;
    };
    let Ok(supported) = device.default_output_config() else {
        println!("No default config. Skipped.");
        return;
    };
    let rate = supported.sample_rate();
    let channels = supported.channels() as usize;
    println!(
        "- Stream config: {} Hz, {} channels, {:?}, buffer {:?}",
        rate,
        channels,
        supported.sample_format(),
        supported.buffer_size()
    );

    // Two files at the device rate, ten seconds in total.
    let a_path = dir.join("cpal-a.flac");
    let b_path = dir.join("cpal-b.flac");
    if rate == 48_000 || rate == 44_100 {
        write_flac(&a_path, rate, 16, &noise(41, 0.25, rate as usize * 6));
        write_flac(&b_path, rate, 16, &noise(42, 0.25, rate as usize * 6));
    } else {
        println!("- Device rate {rate} Hz is not covered by the test writer. Skipped.");
        return;
    }
    let a_frames = decode_all(&a_path).len() / 2;
    let mut a = decoder(&a_path);
    let mut b = decoder(&b_path);
    let mut eq = eq_stage::<Biquad>(rate);
    let gain = if audible() { 0.2f32 } else { 0.0 };
    let frames_out = Arc::new(AtomicU64::new(0));
    let switch_at = Arc::new(AtomicU64::new(u64::MAX));
    let stats = Arc::new(Mutex::new((
        0u64,
        0u64,
        0u64,
        Vec::<f64>::new(),
        0u64,
        u64::MAX,
        0u64,
    )));
    let (fo, sw, st) = (frames_out.clone(), switch_at.clone(), stats.clone());
    let mut on_b = false;
    let mut last: Option<Instant> = None;
    let config = rodio::cpal::StreamConfig {
        channels: channels as u16,
        sample_rate: rate,
        buffer_size: rodio::cpal::BufferSize::Default,
    };
    let stream = device.build_output_stream::<f32, _, _>(
        config,
        move |data: &mut [f32], info: &rodio::cpal::OutputCallbackInfo| {
            let start = Instant::now();
            for frame in data.chunks_mut(channels) {
                let mut pair = [0.0f32; 2];
                for slot in pair.iter_mut() {
                    *slot = if on_b {
                        b.next().unwrap_or(0.0)
                    } else {
                        match a.next() {
                            Some(s) => s,
                            None => {
                                on_b = true;
                                sw.store(fo.load(Ordering::Relaxed), Ordering::Relaxed);
                                b.next().unwrap_or(0.0)
                            }
                        }
                    };
                }
                eq(&mut pair);
                for (i, s) in frame.iter_mut().enumerate() {
                    *s = pair[i.min(1)] * gain;
                }
                fo.fetch_add(1, Ordering::Relaxed);
            }
            let busy = start.elapsed().as_nanos() as u64;
            let ts = info.timestamp();
            let latency = ts.playback.duration_since(ts.callback).as_secs_f64() * 1000.0;
            if let Ok(mut s) = st.try_lock() {
                s.0 += 1; // callbacks
                s.1 = s.1.max(busy); // longest
                s.2 += busy; // total
                if s.3.len() < 4096 {
                    s.3.push(latency);
                }
                if let Some(prev) = last {
                    let gap = start.duration_since(prev).as_nanos() as u64;
                    s.4 = s.4.max(gap);
                    s.5 = s.5.min(gap);
                }
                if busy as f64 > data.len() as f64 / channels as f64 / rate as f64 * 1e9 {
                    s.6 += 1;
                }
            }
            last = Some(start);
        },
        |e| eprintln!("stream error: {e}"),
        None,
    );
    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            println!("- Could not open the stream: {e}");
            return;
        }
    };
    stream.play().ok();
    std::thread::sleep(Duration::from_secs(10));
    drop(stream);
    let s = stats.lock().unwrap();
    let total = frames_out.load(Ordering::Relaxed);
    let mut lat = s.3.clone();
    lat.sort_by(f64::total_cmp);
    println!(
        "- {} callbacks, {:.2} ms of audio each, mean {:.3} ms to produce, longest {:.3} ms, interval {:.1}–{:.1} ms, missed {}.",
        s.0,
        total as f64 / s.0.max(1) as f64 / rate as f64 * 1000.0,
        s.2 as f64 / s.0.max(1) as f64 / 1e6,
        s.1 as f64 / 1e6,
        s.5 as f64 / 1e6,
        s.4 as f64 / 1e6,
        s.6
    );
    if !lat.is_empty() {
        println!("- WASAPI-reported output latency (callback to playback): median {:.1} ms, range {:.1}–{:.1} ms.", lat[lat.len() / 2], lat[0], lat[lat.len() - 1]);
    }
    let sw = switch_at.load(Ordering::Relaxed);
    println!(
        "- Deck switch at output frame {sw}; file A has {a_frames} frames: {}.",
        if sw == a_frames as u64 {
            "sample-exact"
        } else {
            "not exact"
        }
    );
    // Can WASAPI shared mode be opened at a rate other than the mix rate?
    let other = if rate == 48_000 { 44_100 } else { 48_000 };
    let test = rodio::cpal::StreamConfig {
        channels: channels as u16,
        sample_rate: other,
        buffer_size: rodio::cpal::BufferSize::Default,
    };
    let opened = device.build_output_stream::<f32, _, _>(
        test,
        |d: &mut [f32], _: &_| d.fill(0.0),
        |_| {},
        None,
    );
    println!(
        "- Opening the same device at {other} Hz (not its mix rate) in shared mode: {}.",
        match opened {
            Ok(_) => "accepted (Windows converts)".to_string(),
            Err(e) => format!("refused ({e})"),
        }
    );
    let _ = rms(&[]);
}
