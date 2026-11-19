//! Objective measurements on interleaved stereo f32 audio.

use std::f64::consts::{PI, TAU};

pub fn channel(interleaved: &[f32], ch: usize) -> Vec<f64> {
    interleaved
        .iter()
        .skip(ch)
        .step_by(2)
        .map(|&x| x as f64)
        .collect()
}

/// Largest second difference |x[n] - 2x[n-1] + x[n-2]| in `range` (frames).
/// A smooth signal has a small, steady value; a click or gap shows as a spike.
pub fn click_peak(x: &[f64], range: std::ops::Range<usize>) -> f64 {
    let start = range.start.max(2);
    let end = range.end.min(x.len());
    (start..end)
        .map(|n| (x[n] - 2.0 * x[n - 1] + x[n - 2]).abs())
        .fold(0.0, f64::max)
}

pub fn rms(x: &[f64]) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
}

pub fn db(v: f64) -> f64 {
    20.0 * v.max(1e-12).log10()
}

/// Stereo RMS (both channels together) per window, in dBFS.
pub fn windowed_rms_db(interleaved: &[f32], window_frames: usize) -> Vec<f64> {
    interleaved
        .chunks(window_frames * 2)
        .map(|w| {
            let sum: f64 = w.iter().map(|&s| (s as f64) * (s as f64)).sum();
            db((sum / w.len() as f64).sqrt())
        })
        .collect()
}

/// Amplitude of `freq` in `x` (Goertzel), as a linear peak amplitude.
pub fn tone_amplitude(x: &[f64], rate: f64, freq: f64) -> f64 {
    // Use a whole number of cycles for an exact estimate.
    let cycles = ((x.len() as f64) * freq / rate).floor();
    let n = ((cycles * rate / freq).round() as usize)
        .min(x.len())
        .max(1);
    let w = TAU * freq / rate;
    let (mut s, mut c) = (0.0, 0.0);
    for (i, v) in x[..n].iter().enumerate() {
        s += v * (w * i as f64).sin();
        c += v * (w * i as f64).cos();
    }
    2.0 * (s * s + c * c).sqrt() / n as f64
}

/// THD+N in dB: what remains after removing the best-fitting sine at `freq`
/// (plus DC), relative to the signal.
pub fn thd_n_db(x: &[f64], rate: f64, freq: f64) -> f64 {
    let w = TAU * freq / rate;
    // Least squares for a*sin + b*cos + c.
    let mut m = [[0.0f64; 3]; 3];
    let mut v = [0.0f64; 3];
    for (i, &y) in x.iter().enumerate() {
        let basis = [(w * i as f64).sin(), (w * i as f64).cos(), 1.0];
        for r in 0..3 {
            v[r] += basis[r] * y;
            for c in 0..3 {
                m[r][c] += basis[r] * basis[c];
            }
        }
    }
    let coeffs = solve3(m, v);
    let mut residual = 0.0;
    let mut total = 0.0;
    for (i, &y) in x.iter().enumerate() {
        let fit = coeffs[0] * (w * i as f64).sin() + coeffs[1] * (w * i as f64).cos() + coeffs[2];
        residual += (y - fit) * (y - fit);
        total += y * y;
    }
    10.0 * (residual / total).max(1e-30).log10()
}

fn solve3(mut m: [[f64; 3]; 3], mut v: [f64; 3]) -> [f64; 3] {
    for col in 0..3 {
        let pivot = (col..3)
            .max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))
            .unwrap();
        m.swap(col, pivot);
        v.swap(col, pivot);
        for row in col + 1..3 {
            let f = m[row][col] / m[col][col];
            for k in col..3 {
                m[row][k] -= f * m[col][k];
            }
            v[row] -= f * v[col];
        }
    }
    let mut out = [0.0; 3];
    for row in (0..3).rev() {
        let mut s = v[row];
        for k in row + 1..3 {
            s -= m[row][k] * out[k];
        }
        out[row] = s / m[row][row];
    }
    out
}

/// In-place radix-2 FFT (length must be a power of two).
pub fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (ang * k as f64).sin_cos();
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * c - im[b] * s;
                let ti = re[b] * s + im[b] * c;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

/// Magnitude spectrum (Hann window) of the first `n` samples, n a power of two.
pub fn spectrum(x: &[f64], n: usize) -> Vec<f64> {
    let mut re: Vec<f64> = (0..n)
        .map(|i| x[i] * (0.5 - 0.5 * (TAU * i as f64 / n as f64).cos()))
        .collect();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im);
    (0..n / 2)
        .map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt() / (n as f64 / 4.0))
        .collect()
}
