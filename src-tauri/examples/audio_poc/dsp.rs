//! DSP for the experiments, written for this proof of concept only.
//!
//! Filters follow the public RBJ "Audio EQ Cookbook" (biquads) and Andrew
//! Simper's "linear trapezoidal SVF" papers. Nothing is taken from another
//! project's source.

use std::f64::consts::PI;
use std::num::NonZero;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rodio::Source;

// ── Filters ────────────────────────────────────────────────────────────────

/// ISO octave centres used by most 10-band graphic equalisers.
pub const BANDS: [f64; 10] = [
    31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
/// One octave wide.
pub const Q: f64 = std::f64::consts::SQRT_2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    LowShelf,
    Peak,
    HighShelf,
}

pub fn band_kind(index: usize) -> Kind {
    match index {
        0 => Kind::LowShelf,
        9 => Kind::HighShelf,
        _ => Kind::Peak,
    }
}

pub trait Filter: Default + Send {
    fn design(&mut self, kind: Kind, freq: f64, q: f64, gain_db: f64, rate: f64);
    fn process(&mut self, ch: usize, x: f64) -> f64;
    fn reset(&mut self);
}

/// RBJ biquad, transposed direct form II, f64 state.
#[derive(Default)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    s1: [f64; 2],
    s2: [f64; 2],
}

impl Filter for Biquad {
    fn design(&mut self, kind: Kind, freq: f64, q: f64, gain_db: f64, rate: f64) {
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let (b0, b1, b2, a0, a1, a2) = match kind {
            Kind::Peak => (
                1.0 + alpha * a,
                -2.0 * cos,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos,
                1.0 - alpha / a,
            ),
            Kind::LowShelf => {
                let sq = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos + sq),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - sq),
                    (a + 1.0) + (a - 1.0) * cos + sq,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - sq,
                )
            }
            Kind::HighShelf => {
                let sq = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos + sq),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - sq),
                    (a + 1.0) - (a - 1.0) * cos + sq,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - sq,
                )
            }
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    #[inline]
    fn process(&mut self, ch: usize, x: f64) -> f64 {
        let y = self.b0 * x + self.s1[ch];
        self.s1[ch] = self.b1 * x - self.a1 * y + self.s2[ch];
        self.s2[ch] = self.b2 * x - self.a2 * y;
        y
    }

    fn reset(&mut self) {
        self.s1 = [0.0; 2];
        self.s2 = [0.0; 2];
    }
}

/// Simper linear trapezoidal state-variable filter (bell and shelves).
#[derive(Default)]
pub struct Svf {
    a1: f64,
    a2: f64,
    a3: f64,
    m0: f64,
    m1: f64,
    m2: f64,
    ic1: [f64; 2],
    ic2: [f64; 2],
}

impl Filter for Svf {
    fn design(&mut self, kind: Kind, freq: f64, q: f64, gain_db: f64, rate: f64) {
        let a = 10f64.powf(gain_db / 40.0);
        let base = (PI * freq / rate).tan();
        let (g, k, m0, m1, m2) = match kind {
            Kind::Peak => (
                base,
                1.0 / (q * a),
                1.0,
                (1.0 / (q * a)) * (a * a - 1.0),
                0.0,
            ),
            Kind::LowShelf => (
                base / a.sqrt(),
                1.0 / q,
                1.0,
                (1.0 / q) * (a - 1.0),
                a * a - 1.0,
            ),
            Kind::HighShelf => (
                base * a.sqrt(),
                1.0 / q,
                a * a,
                (1.0 / q) * (1.0 - a) * a,
                1.0 - a * a,
            ),
        };
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
        self.m0 = m0;
        self.m1 = m1;
        self.m2 = m2;
    }

    #[inline]
    fn process(&mut self, ch: usize, x: f64) -> f64 {
        let v3 = x - self.ic2[ch];
        let v1 = self.a1 * self.ic1[ch] + self.a2 * v3;
        let v2 = self.ic2[ch] + self.a2 * self.ic1[ch] + self.a3 * v3;
        self.ic1[ch] = 2.0 * v1 - self.ic1[ch];
        self.ic2[ch] = 2.0 * v2 - self.ic2[ch];
        self.m0 * x + self.m1 * v1 + self.m2 * v2
    }

    fn reset(&mut self) {
        self.ic1 = [0.0; 2];
        self.ic2 = [0.0; 2];
    }
}

// ── Equaliser ──────────────────────────────────────────────────────────────

/// Frames between coefficient updates while a band glides.
pub const GLIDE_BLOCK: usize = 16;

pub struct Equalizer<F: Filter> {
    filters: [F; 10],
    gain: [f64; 10],
    target: [f64; 10],
    /// dB per GLIDE_BLOCK frames; 0 means changes apply at once.
    step: f64,
    rate: f64,
    counter: usize,
    block: usize,
}

impl<F: Filter> Equalizer<F> {
    /// `glide_ms` 0 applies changes immediately.
    pub fn new(rate: f64, glide_ms: f64) -> Self {
        Self::with_block(rate, glide_ms, GLIDE_BLOCK)
    }

    /// `block`: frames between coefficient updates while gliding.
    pub fn with_block(rate: f64, glide_ms: f64, block: usize) -> Self {
        let mut eq = Equalizer {
            filters: std::array::from_fn(|_| F::default()),
            gain: [0.0; 10],
            target: [0.0; 10],
            step: 0.0,
            rate,
            counter: 0,
            block,
        };
        // Up to a 24 dB move takes `glide_ms`.
        if glide_ms > 0.0 {
            let blocks = (glide_ms / 1000.0 * rate / block as f64).max(1.0);
            eq.step = 24.0 / blocks;
        }
        for i in 0..10 {
            eq.redesign(i);
        }
        eq
    }

    fn redesign(&mut self, i: usize) {
        self.filters[i].design(band_kind(i), BANDS[i], Q, self.gain[i], self.rate);
    }

    pub fn set_gains(&mut self, gains: [f64; 10]) {
        self.target = gains;
        if self.step == 0.0 {
            for i in 0..10 {
                if self.gain[i] != gains[i] {
                    if self.gain[i] == 0.0 {
                        self.filters[i].reset(); // was bypassed: start from silence
                    }
                    self.gain[i] = gains[i];
                    self.redesign(i);
                }
            }
        }
    }

    fn glide(&mut self) {
        for i in 0..10 {
            let (now, target) = (self.gain[i], self.target[i]);
            if now != target {
                if now == 0.0 {
                    self.filters[i].reset();
                }
                let next = if (target - now).abs() <= self.step {
                    target
                } else {
                    now + self.step * (target - now).signum()
                };
                self.gain[i] = next;
                self.redesign(i);
            }
        }
    }

    #[inline]
    pub fn process(&mut self, frame: &mut [f32; 2]) {
        if self.step > 0.0 {
            if self.counter == 0 {
                self.glide();
            }
            self.counter = (self.counter + 1) % self.block;
        }
        for ch in 0..2 {
            let mut x = frame[ch] as f64;
            for i in 0..10 {
                // A band at exactly 0 dB is skipped: bit-exact, and free.
                if self.gain[i] != 0.0 {
                    x = self.filters[i].process(ch, x);
                }
            }
            frame[ch] = x as f32;
        }
    }

    /// Every band processed even at 0 dB (to measure filter transparency).
    pub fn process_unbypassed(&mut self, frame: &mut [f32; 2]) {
        for ch in 0..2 {
            let mut x = frame[ch] as f64;
            for f in self.filters.iter_mut() {
                x = f.process(ch, x);
            }
            frame[ch] = x as f32;
        }
    }
}

// ── Limiter ────────────────────────────────────────────────────────────────

/// Peak limiter: instant attack, exponential release, no look-ahead. Its gain
/// is exactly 1.0 until a peak passes the threshold, so quiet audio passes
/// through unchanged (bit-exact).
pub struct Limiter {
    threshold: f64,
    release: f64,
    gain: f64,
    pub engaged_frames: u64,
    pub over_tenth_db: u64,
    pub over_one_db: u64,
    pub min_gain: f64,
}

impl Limiter {
    pub fn new(threshold_db: f64, release_ms: f64, rate: f64) -> Self {
        Limiter {
            threshold: 10f64.powf(threshold_db / 20.0),
            release: (-1.0 / (release_ms / 1000.0 * rate)).exp(),
            gain: 1.0,
            engaged_frames: 0,
            over_tenth_db: 0,
            over_one_db: 0,
            min_gain: 1.0,
        }
    }

    #[inline]
    pub fn process(&mut self, frame: &mut [f32; 2]) {
        let peak = (frame[0] as f64).abs().max((frame[1] as f64).abs());
        // The most gain this frame allows without passing the threshold.
        let allowed = if peak > self.threshold {
            self.threshold / peak
        } else {
            1.0
        };
        if allowed < self.gain {
            self.gain = allowed; // instant attack
        } else if self.gain < 1.0 {
            // Release towards what is allowed, never above it.
            self.gain = allowed - (allowed - self.gain) * self.release;
            if allowed == 1.0 && self.gain > 0.999_999 {
                self.gain = 1.0;
            }
        }
        if self.gain < 1.0 {
            self.engaged_frames += 1;
            if self.gain < 0.988_553 {
                self.over_tenth_db += 1;
            }
            if self.gain < 0.891_251 {
                self.over_one_db += 1;
            }
            self.min_gain = self.min_gain.min(self.gain);
            frame[0] = (frame[0] as f64 * self.gain) as f32;
            frame[1] = (frame[1] as f64 * self.gain) as f32;
        }
    }
}

// ── Crossfade envelope ─────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    Linear,
    EqualPower,
}

/// Gain for a fade at `t` in 0..=1. `incoming` rises; otherwise it falls.
pub fn fade_gain(curve: Curve, t: f64, incoming: bool) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match (curve, incoming) {
        (Curve::Linear, true) => t,
        (Curve::Linear, false) => 1.0 - t,
        (Curve::EqualPower, true) => (t * PI / 2.0).sin(),
        (Curve::EqualPower, false) => (t * PI / 2.0).cos(),
    }
}

pub struct Fade {
    pub curve: Curve,
    pub incoming: bool,
    pub start: u64,
    pub length: u64,
    frame: u64,
}

impl Fade {
    pub fn new(curve: Curve, incoming: bool, start: u64, length: u64) -> Self {
        Fade {
            curve,
            incoming,
            start,
            length,
            frame: 0,
        }
    }

    #[inline]
    pub fn process(&mut self, frame: &mut [f32; 2]) {
        let t = (self.frame as f64 - self.start as f64) / self.length as f64;
        let g = fade_gain(self.curve, t, self.incoming);
        if g != 1.0 {
            frame[0] = (frame[0] as f64 * g) as f32;
            frame[1] = (frame[1] as f64 * g) as f32;
        }
        self.frame += 1;
    }
}

// ── Capture ring (visualizer) ──────────────────────────────────────────────

/// Single-producer, single-consumer ring of f32 samples. The audio side never
/// waits: when the reader falls behind, new samples are dropped and counted.
pub struct Ring {
    buf: Box<[AtomicU32]>,
    mask: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
    pub dropped: AtomicU64,
    /// Instant (as nanoseconds since `origin`) of the most recent write.
    pub last_write_ns: AtomicU64,
    origin: Instant,
}

impl Ring {
    pub fn new(capacity_pow2: usize, origin: Instant) -> Arc<Ring> {
        assert!(capacity_pow2.is_power_of_two());
        Arc::new(Ring {
            buf: (0..capacity_pow2).map(|_| AtomicU32::new(0)).collect(),
            mask: capacity_pow2 - 1,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            dropped: AtomicU64::new(0),
            last_write_ns: AtomicU64::new(0),
            origin,
        })
    }

    #[inline]
    pub fn push_frame(&self, frame: &[f32; 2]) {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head - tail + 2 > self.mask + 1 {
            self.dropped.fetch_add(2, Ordering::Relaxed);
            return;
        }
        self.buf[head & self.mask].store(frame[0].to_bits(), Ordering::Relaxed);
        self.buf[(head + 1) & self.mask].store(frame[1].to_bits(), Ordering::Relaxed);
        self.head.store(head + 2, Ordering::Release);
    }

    pub fn stamp(&self) {
        self.last_write_ns
            .store(self.origin.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }

    /// Reads everything available into `out`. Returns the number of samples read.
    pub fn drain(&self, out: &mut Vec<f32>) -> usize {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        for i in tail..head {
            out.push(f32::from_bits(
                self.buf[i & self.mask].load(Ordering::Relaxed),
            ));
        }
        self.tail.store(head, Ordering::Release);
        head - tail
    }
}

// ── A rodio Source that runs a frame processor ────────────────────────────

pub trait FrameProcessor: Send {
    fn process(&mut self, frame: &mut [f32; 2]);
}

impl<F: FnMut(&mut [f32; 2]) + Send> FrameProcessor for F {
    fn process(&mut self, frame: &mut [f32; 2]) {
        self(frame)
    }
}

/// Wraps a stereo rodio source and processes it frame by frame: the shape a
/// production EQ, limiter, fade, or capture stage would take inside rodio.
pub struct Processed<S: Source, P: FrameProcessor> {
    inner: S,
    processor: P,
    frame: [f32; 2],
    index: usize,
}

impl<S: Source, P: FrameProcessor> Processed<S, P> {
    pub fn new(inner: S, processor: P) -> Self {
        assert_eq!(
            inner.channels().get(),
            2,
            "the experiments use stereo sources"
        );
        Processed {
            inner,
            processor,
            frame: [0.0; 2],
            index: 2,
        }
    }
}

impl<S: Source, P: FrameProcessor> Iterator for Processed<S, P> {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        if self.index == 2 {
            let l = self.inner.next()?;
            let r = self.inner.next().unwrap_or(0.0);
            self.frame = [l, r];
            self.processor.process(&mut self.frame);
            self.index = 0;
        }
        let s = self.frame[self.index];
        self.index += 1;
        Some(s)
    }
}

impl<S: Source, P: FrameProcessor> Source for Processed<S, P> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> NonZero<u16> {
        self.inner.channels()
    }
    fn sample_rate(&self) -> NonZero<u32> {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

// ── Callback timing tap (device runs) ─────────────────────────────────────

/// Measures how the device thread pulls samples: each burst of pulls separated
/// by a pause is one device callback. Records the time spent producing each
/// burst against the audio time the burst holds. No allocation after creation.
pub struct CallbackStats {
    pub callbacks: u64,
    pub samples: u64,
    pub max_busy_ns: u64,
    pub total_busy_ns: u64,
    pub max_gap_ns: u64,
    pub min_gap_ns: u64,
    /// Bursts whose production took longer than the audio they held.
    pub missed: u64,
    pub max_burst_samples: u64,
}

pub struct TimingTap<S: Source> {
    inner: S,
    pub stats: Arc<std::sync::Mutex<CallbackStats>>,
    burst_start: Option<Instant>,
    last: Option<Instant>,
    burst_samples: u64,
    sample_ns: f64,
    counter: u32,
}

impl<S: Source> TimingTap<S> {
    pub fn new(inner: S) -> (Self, Arc<std::sync::Mutex<CallbackStats>>) {
        let stats = Arc::new(std::sync::Mutex::new(CallbackStats {
            callbacks: 0,
            samples: 0,
            max_busy_ns: 0,
            total_busy_ns: 0,
            max_gap_ns: 0,
            min_gap_ns: u64::MAX,
            missed: 0,
            max_burst_samples: 0,
        }));
        let sample_ns = 1e9 / (inner.sample_rate().get() as f64 * inner.channels().get() as f64);
        (
            TimingTap {
                inner,
                stats: stats.clone(),
                burst_start: None,
                last: None,
                burst_samples: 0,
                sample_ns,
                counter: 0,
            },
            stats,
        )
    }

    fn close_burst(&mut self, end: Instant, next_start: Instant) {
        if let (Some(start), Ok(mut s)) = (self.burst_start, self.stats.try_lock()) {
            let busy = end.duration_since(start).as_nanos() as u64;
            let gap = next_start.duration_since(start).as_nanos() as u64;
            s.callbacks += 1;
            s.samples += self.burst_samples;
            s.total_busy_ns += busy;
            s.max_busy_ns = s.max_busy_ns.max(busy);
            s.max_gap_ns = s.max_gap_ns.max(gap);
            s.min_gap_ns = s.min_gap_ns.min(gap);
            s.max_burst_samples = s.max_burst_samples.max(self.burst_samples);
            if busy as f64 > self.burst_samples as f64 * self.sample_ns {
                s.missed += 1;
            }
        }
    }
}

impl<S: Source> Iterator for TimingTap<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        // Reading the clock for every sample would distort the measurement;
        // every 32nd sample is enough to find the bursts.
        if self.counter.is_multiple_of(32) {
            let now = Instant::now();
            match self.last {
                Some(last) if now.duration_since(last) > Duration::from_micros(800) => {
                    self.close_burst(last, now);
                    self.burst_start = Some(now);
                    self.burst_samples = 0;
                }
                None => self.burst_start = Some(now),
                _ => {}
            }
            self.last = Some(now);
        }
        self.counter = self.counter.wrapping_add(1);
        self.burst_samples += 1;
        self.inner.next()
    }
}

impl<S: Source> Source for TimingTap<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> NonZero<u16> {
        self.inner.channels()
    }
    fn sample_rate(&self) -> NonZero<u32> {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

// ── Windowed-sinc resampler (feasibility check only) ──────────────────────

fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, mut k) = (1.0, 1.0, 1.0);
    while term > 1e-12 * sum {
        term *= (x / (2.0 * k)) * (x / (2.0 * k));
        sum += term;
        k += 1.0;
    }
    sum
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Polyphase windowed-sinc sample-rate converter as a rodio Source: up by L,
/// low-pass, down by M. TAPS input frames per output frame; Kaiser window.
/// Continuous across the source's spans (no restarts).
pub struct SincResampler<S: Source> {
    inner: S,
    up: u64,
    down: u64,
    out_rate: u32,
    taps: usize,
    h: Vec<f32>,
    buf: std::collections::VecDeque<[f32; 2]>,
    buf_start: u64,
    k: u64,
    ended_at: Option<u64>,
    frame: [f32; 2],
    index: usize,
}

pub const SINC_TAPS: usize = 32;

impl<S: Source> SincResampler<S> {
    pub fn new(inner: S, out_rate: u32) -> Self {
        let in_rate = inner.sample_rate().get() as u64;
        let g = gcd(in_rate, out_rate as u64);
        let (up, down) = (out_rate as u64 / g, in_rate / g);
        let taps = SINC_TAPS;
        let len = taps * up as usize;
        // Cut-off a little below the lower Nyquist frequency, in units of the upsampled rate.
        let cutoff =
            0.5 * (in_rate.min(out_rate as u64) as f64 / in_rate as f64) * 0.91 / up as f64;
        let beta = 9.0;
        let centre = (len as f64 - 1.0) / 2.0;
        let h = (0..len)
            .map(|i| {
                let t = i as f64 - centre;
                let sinc = if t == 0.0 {
                    2.0 * cutoff
                } else {
                    (2.0 * PI * cutoff * t).sin() / (PI * t)
                };
                let w = bessel_i0(beta * (1.0 - (t / centre).powi(2)).max(0.0).sqrt())
                    / bessel_i0(beta);
                (sinc * w * up as f64) as f32
            })
            .collect();
        SincResampler {
            inner,
            up,
            down,
            out_rate,
            taps,
            h,
            buf: std::collections::VecDeque::new(),
            buf_start: 0,
            k: 0,
            ended_at: None,
            frame: [0.0; 2],
            index: 2,
        }
    }

    fn input(&self, n: u64) -> [f32; 2] {
        if n < self.buf_start {
            return [0.0; 2];
        }
        self.buf
            .get((n - self.buf_start) as usize)
            .copied()
            .unwrap_or([0.0; 2])
    }

    fn next_frame(&mut self) -> Option<[f32; 2]> {
        let m = self.k * self.down;
        let n0 = m / self.up;
        let p = (m % self.up) as usize;
        if let Some(end) = self.ended_at {
            if n0 >= end + self.taps as u64 / 2 {
                return None;
            }
        }
        // Make sure frame n0 is buffered.
        while self.ended_at.is_none() && self.buf_start + self.buf.len() as u64 <= n0 {
            match (self.inner.next(), self.inner.next()) {
                (Some(l), r) => self.buf.push_back([l, r.unwrap_or(0.0)]),
                _ => self.ended_at = Some(self.buf_start + self.buf.len() as u64),
            }
        }
        // Keep only what the filter can still reach.
        while self.buf.len() > self.taps + 1 && self.buf_start + (self.taps as u64) < n0 {
            self.buf.pop_front();
            self.buf_start += 1;
        }
        let mut acc = [0.0f32; 2];
        for j in 0..self.taps {
            let coeff = self.h[p + j * self.up as usize];
            if n0 < j as u64 {
                break;
            }
            let x = self.input(n0 - j as u64);
            acc[0] += coeff * x[0];
            acc[1] += coeff * x[1];
        }
        self.k += 1;
        Some(acc)
    }
}

impl<S: Source> Iterator for SincResampler<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.index == 2 {
            self.frame = self.next_frame()?;
            self.index = 0;
        }
        let s = self.frame[self.index];
        self.index += 1;
        Some(s)
    }
}

impl<S: Source> Source for SincResampler<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> NonZero<u16> {
        NonZero::new(2).unwrap()
    }
    fn sample_rate(&self) -> NonZero<u32> {
        NonZero::new(self.out_rate).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
