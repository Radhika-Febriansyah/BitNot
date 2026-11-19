//! The output stage: sits between rodio's queue (the Player) and the device
//! mixer, and hands the mixer audio that already has the device's format.
//!
//! Two problems it removes (measured in the Phase 3 proof of concept,
//! docs/reports/phase-3-assessment.md §9.4):
//!
//! - rodio converts sample rates by linear interpolation and starts a new
//!   converter at every decoder packet: THD+N about −35 dB at 1 kHz and −7 dB
//!   at 16 kHz, with small timing steps at each packet.
//! - a Player connected while its queue is empty makes the mixer read the
//!   empty queue's format (mono) for the first stretch of the first track.
//!
//! This stage always reports two channels at the device rate, so the mixer
//! never converts anything and never sees a format change. It converts the
//! rate itself with a windowed-sinc interpolator that runs over the queue's
//! continuous output (not per track), so packet and track boundaries at the
//! same rate are invisible to it. Audio already at the device rate is passed
//! through untouched (bit-exact).

use std::collections::VecDeque;
use std::f64::consts::PI;
use std::num::NonZero;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use rodio::Source;

/// Filter length in samples of the lower of the two rates.
const TAPS: usize = 96;
const HALF: usize = TAPS / 2;
/// Kernel table resolution: entries per sample of the lower rate.
const PHASES: usize = 512;
/// Filter cut-off (−6 dB) as a fraction of the lower Nyquist frequency. With
/// 96 taps the response is flat to 20 kHz at 44.1 kHz (measured: 0.000 dB at
/// 20 kHz, −1.8 dB at 21 kHz) and a 23 kHz tone is suppressed by 91 dB when
/// converting 48 to 44.1 kHz. The PoC's 32 taps at 0.91 was −5.7 dB at 20 kHz.
const CUTOFF: f64 = 0.97;
/// Output frames over which a track at the output rate that follows a change
/// of rate is moved onto whole frames (0.1 s at 48 kHz).
const GLIDE_FRAMES: u32 = 4_800;
/// Kaiser window shape (stop band near −90 dB).
const BETA: f64 = 9.0;

fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, mut k) = (1.0, 1.0, 1.0);
    while term > 1e-14 * sum {
        term *= (x / (2.0 * k)) * (x / (2.0 * k));
        sum += term;
        k += 1.0;
    }
    sum
}

/// Windowed-sinc kernel for |t| from 0 to HALF, PHASES entries per sample.
fn kernel() -> &'static [f64] {
    static TABLE: OnceLock<Vec<f64>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let fc = 0.5 * CUTOFF;
        (0..=HALF * PHASES + 1)
            .map(|i| {
                let t = i as f64 / PHASES as f64;
                if t >= HALF as f64 {
                    return 0.0;
                }
                let sinc = if t == 0.0 {
                    2.0 * fc
                } else {
                    (2.0 * PI * fc * t).sin() / (PI * t)
                };
                let r = t / HALF as f64;
                sinc * bessel_i0(BETA * (1.0 - r * r).sqrt()) / bessel_i0(BETA)
            })
            .collect()
    })
}

/// Builds the kernel table now (a few milliseconds, once per process), so
/// the audio thread never pays for it when resampling first starts.
pub fn prepare_kernel() {
    kernel();
}

#[inline]
fn kernel_at(table: &[f64], t: f64) -> f64 {
    let x = t.abs() * PHASES as f64;
    let i = x as usize;
    if i >= HALF * PHASES {
        return 0.0;
    }
    let f = x - i as f64;
    table[i] + f * (table[i + 1] - table[i])
}

/// A stretch of input at one sample rate. Frame numbers count from the
/// segment's first frame.
struct Segment {
    rate: u32,
    /// Frames from `start` on.
    frames: VecDeque<[f32; 2]>,
    /// Number of the first frame still kept.
    start: u64,
    /// Number of frames in the segment, once its end is known.
    end: Option<u64>,
}

impl Segment {
    fn new(rate: u32, first: [f32; 2]) -> Self {
        Segment {
            rate,
            frames: VecDeque::from([first]),
            start: 0,
            end: None,
        }
    }

    fn frame(&self, n: i64) -> [f32; 2] {
        if n < self.start as i64 || self.end.is_some_and(|end| n >= end as i64) {
            return [0.0; 2];
        }
        self.frames
            .get((n - self.start as i64) as usize)
            .copied()
            .unwrap_or([0.0; 2])
    }

    fn buffered_to(&self) -> u64 {
        self.start + self.frames.len() as u64
    }

    /// Drops frames the filter can no longer reach.
    fn trim(&mut self, keep_from: u64) {
        while self.start < keep_from && !self.frames.is_empty() {
            self.frames.pop_front();
            self.start += 1;
        }
    }

    /// How much the kernel is stretched for a band limit of `band_rate`
    /// (the lowest rate involved): 1 when this segment's rate is that rate.
    fn scale(&self, band_rate: u32) -> f64 {
        (band_rate as f64 / self.rate as f64).min(1.0)
    }

    /// How many frames either side of a position the filter reaches.
    fn reach(&self, band_rate: u32) -> u64 {
        (HALF as f64 / self.scale(band_rate)).ceil() as u64
    }

    /// The band-limited value of this segment at `position` (in its own
    /// frames; it may lie outside the segment, where the segment is silent).
    /// Near a change of rate, both segments use the same band limit, so
    /// their contributions add up to the continuous signal.
    fn value_at(&mut self, position: f64, band_rate: u32) -> [f64; 2] {
        let table = kernel();
        let scale = self.scale(band_rate);
        let reach = self.reach(band_rate) as i64;
        let base = position.floor() as i64;
        let fraction = position - base as f64;
        let first = base + 1 - reach;
        let last = base + reach;
        let limit = self.end.unwrap_or(u64::MAX).min(self.buffered_to()) as i64;
        let mut acc = [0.0f64; 2];
        if first >= self.start as i64 && last < limit {
            // Every frame the filter reaches is buffered: walk a slice.
            let offset = (first - self.start as i64) as usize;
            let frames = &self.frames.make_contiguous()[offset..offset + 2 * reach as usize];
            let mut t = (reach as f64 - 1.0 + fraction) * scale;
            for x in frames {
                let weight = kernel_at(table, t);
                acc[0] += weight * x[0] as f64;
                acc[1] += weight * x[1] as f64;
                t -= scale;
            }
        } else {
            // Near an edge: frames outside the segment are silence.
            for n in first.max(0)..=last {
                let weight = kernel_at(table, (position - n as f64) * scale);
                if weight != 0.0 {
                    let x = self.frame(n);
                    acc[0] += weight * x[0] as f64;
                    acc[1] += weight * x[1] as f64;
                }
            }
        }
        [acc[0] * scale, acc[1] * scale]
    }
}

/// Where each track begins in the queue's output.
///
/// rodio's queue cannot say reliably which source a sample came from at a
/// boundary: a decoder never reports itself finished, so the queue goes on
/// describing the old track until after the new one has delivered a sample,
/// and with another track queued it never reports an in-memory source as
/// finished either. Every track BitNot appends is therefore wrapped in
/// [`Marked`], which records its format when it delivers its first sample.
/// That happens inside the output stage's own read, so the stage knows the
/// exact sample where the new track begins.
#[derive(Default)]
pub struct TrackStarts {
    /// 0 when nothing new has started; otherwise `1 << 63 | channels << 32 | rate`.
    next: AtomicU64,
}

impl TrackStarts {
    fn mark(&self, channels: u16, rate: u32) {
        self.next.store(
            (1 << 63) | ((channels as u64) << 32) | rate as u64,
            Ordering::Release,
        );
    }

    fn take(&self) -> Option<(usize, u32)> {
        let value = self.next.swap(0, Ordering::Acquire);
        (value != 0).then_some((((value >> 32) & 0xFFFF) as usize, value as u32))
    }
}

/// A track as appended to the Player: passes everything through, and marks
/// its start (see [`TrackStarts`]).
pub struct Marked<S: Source> {
    inner: S,
    starts: Arc<TrackStarts>,
    started: bool,
}

impl<S: Source> Marked<S> {
    pub fn new(inner: S, starts: &Arc<TrackStarts>) -> Self {
        Marked {
            inner,
            starts: Arc::clone(starts),
            started: false,
        }
    }
}

impl<S: Source> Iterator for Marked<S> {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next();
        if !self.started && sample.is_some() {
            self.started = true;
            self.starts
                .mark(self.inner.channels().get(), self.inner.sample_rate().get());
        }
        sample
    }
}

impl<S: Source> Source for Marked<S> {
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
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)
    }
}

/// See the module documentation.
pub struct OutputStage<S: Source> {
    input: S,
    out_rate: u32,
    starts: Arc<TrackStarts>,
    /// Format of the samples being read: from the last marked track start,
    /// or (before any) what the input reports.
    format: (usize, u32),
    /// The segment being played.
    current: Option<Segment>,
    /// The segment before it, kept while the filter still reaches into it.
    previous: Option<Segment>,
    /// The segment after it, read ahead once the current one's end is known.
    following: Option<Segment>,
    /// The first frame of the segment after `following` (a very short track).
    pending: Option<(u32, [f32; 2])>,
    /// Position of the next output frame in `current`: `index + fraction / out_rate`.
    index: u64,
    fraction: u64,
    /// While moving onto whole frames after a change of rate: frames left,
    /// and the offset still to absorb (in units of 1 / out_rate of a frame).
    glide: Option<(u32, i64)>,
    /// The input has run out (rodio's Player queue never does; test sources do).
    ended: bool,
    frame: [f32; 2],
    channel: usize,
}

impl<S: Source> OutputStage<S> {
    /// `starts` is shared with the [`Marked`] tracks appended to the queue.
    pub fn new(input: S, out_rate: NonZero<u32>, starts: Arc<TrackStarts>) -> Self {
        let format = (input.channels().get() as usize, input.sample_rate().get());
        OutputStage {
            input,
            out_rate: out_rate.get(),
            starts,
            format,
            current: None,
            previous: None,
            following: None,
            pending: None,
            index: 0,
            fraction: 0,
            glide: None,
            ended: false,
            frame: [0.0; 2],
            channel: 2,
        }
    }

    /// Reads one input frame as stereo, with the rate it was produced at.
    /// Mono is duplicated to both sides; beyond two channels, the first two
    /// (front left and right) are kept, as rodio's own converter does.
    fn read(&mut self) -> Option<(u32, [f32; 2])> {
        if self.ended {
            return None;
        }
        let Some(first) = self.input.next() else {
            self.ended = true;
            return None;
        };
        // A track that started with this very sample marked itself (see
        // TrackStarts). Silence the queue plays between tracks keeps the
        // format of the track before it.
        if let Some(format) = self.starts.take() {
            self.format = format;
        }
        let (channels, rate) = self.format;
        let mut frame = [first, first];
        if channels > 1 {
            frame[1] = self.input.next().unwrap_or(0.0);
            for _ in 2..channels {
                self.input.next();
            }
        }
        Some((rate, frame))
    }

    /// Reads until frame `n` of the current segment is buffered, or its end
    /// is known. A change of rate ends it and starts the following segment.
    fn fill_current(&mut self, n: u64) {
        loop {
            let current = self.current.as_ref().expect("current segment");
            if current.end.is_some() || current.buffered_to() > n {
                return;
            }
            let rate = current.rate;
            let read = self.read();
            let current = self.current.as_mut().unwrap();
            match read {
                Some((r, frame)) if r == rate => current.frames.push_back(frame),
                Some((r, frame)) => {
                    current.end = Some(current.buffered_to());
                    self.following = Some(Segment::new(r, frame));
                }
                None => current.end = Some(current.buffered_to()),
            }
        }
    }

    /// Reads until frame `n` of the following segment is buffered, or its end is known.
    fn fill_following(&mut self, n: u64) {
        loop {
            let Some(following) = self.following.as_ref() else {
                return;
            };
            if following.end.is_some() || following.buffered_to() > n {
                return;
            }
            let rate = following.rate;
            let read = self.read();
            let following = self.following.as_mut().unwrap();
            match read {
                Some((r, frame)) if r == rate => following.frames.push_back(frame),
                Some((r, frame)) => {
                    following.end = Some(following.buffered_to());
                    self.pending = Some((r, frame));
                }
                None => following.end = Some(following.buffered_to()),
            }
        }
    }

    /// Maps a position in a segment that ends at `end` (frames, at `from`
    /// Hz) to a position in the segment that follows it (at `to` Hz). Each
    /// frame stands for the half-frame either side of it, so the two grids
    /// are joined where those halves meet; otherwise the two sides would
    /// overlap by half the difference of their frame lengths.
    fn across(position: f64, end: f64, from: u32, to: u32) -> f64 {
        (position - end + 0.5) * to as f64 / from as f64 - 0.5
    }

    /// The band limit near the current segment: the lowest of the output
    /// rate, its rate, and the rates of the neighbours it may overlap.
    fn band_rate(&self) -> u32 {
        let mut band = self.out_rate;
        for segment in [&self.current, &self.previous, &self.following]
            .into_iter()
            .flatten()
        {
            band = band.min(segment.rate);
        }
        band
    }

    /// The moment the current segment ends, as a position in it, and the
    /// first output position at or after that moment.
    fn first_position_after_end(&self) -> Option<f64> {
        let current = self.current.as_ref()?;
        let end = current.end?;
        let out = self.out_rate as u64;
        // Positions advance by rate/out per output frame, exactly.
        let now = self.index * out + self.fraction;
        let target = end * out;
        let steps = if now >= target {
            0
        } else {
            (target - now).div_ceil(current.rate as u64)
        };
        Some((now + steps * current.rate as u64) as f64 / out as f64)
    }

    /// Moves on to the following segment once the current one has ended.
    /// Returns false at the end of the stream.
    fn switch(&mut self) -> bool {
        let first = self.first_position_after_end().unwrap();
        let Some(next) = self.following.take() else {
            // Input ended; nothing follows.
            self.current = None;
            return false;
        };
        let current = self.current.take().unwrap();
        let end = current.end.unwrap() as f64;
        let position = Self::across(first, end, current.rate, next.rate);
        let out = self.out_rate as u64;
        let index = position.floor().max(0.0);
        let mut fraction = ((position - index) * out as f64).round() as u64;
        self.index = index as u64;
        if fraction >= out {
            fraction -= out;
            self.index += 1;
        }
        self.fraction = fraction;
        // A segment at the output rate is passed through on whole frames.
        // Rather than jump there (a time step of up to half a frame), the
        // position glides onto the nearest whole frame over GLIDE_FRAMES.
        self.glide = None;
        if next.rate == self.out_rate && fraction != 0 {
            let units = if fraction <= out / 2 {
                -(fraction as i64)
            } else {
                (out - fraction) as i64
            };
            self.glide = Some((GLIDE_FRAMES, units));
        }
        self.previous = Some(current);
        self.current = Some(next);
        if let Some((rate, frame)) = self.pending.take() {
            self.following = Some(Segment::new(rate, frame));
        }
        true
    }

    fn next_frame(&mut self) -> Option<[f32; 2]> {
        let out = self.out_rate;
        if self.current.is_none() {
            let (rate, first) = self.pending.take().or_else(|| self.read())?;
            self.current = Some(Segment::new(rate, first));
            self.index = 0;
            self.fraction = 0;
        }
        loop {
            let reach = self.current.as_ref().unwrap().reach(self.band_rate());
            self.fill_current(self.index + reach);
            let current = self.current.as_ref().unwrap();
            if current.end.is_some_and(|end| self.index >= end) {
                if !self.switch() {
                    return None;
                }
                continue;
            }
            break;
        }

        let band = self.band_rate();
        let current = self.current.as_ref().unwrap();
        let (rate, reach) = (current.rate, current.reach(band));
        let position = self.index as f64 + self.fraction as f64 / out as f64;

        // The following segment is needed when the filter reaches past the end.
        let following_at = match current.end {
            Some(end) if self.index + reach >= end && self.following.is_some() => {
                let next = self.following.as_ref().unwrap();
                let at = Self::across(position, end as f64, rate, next.rate);
                let needed = (at.max(0.0).ceil() as u64) + next.reach(band);
                self.fill_following(needed);
                Some(at)
            }
            _ => None,
        };
        // The previous segment matters while the filter still reaches back into it.
        let previous_at = match self.previous.as_ref() {
            Some(previous) => {
                // The inverse of `across`, from this segment back into the previous one.
                let at = (position + 0.5) * previous.rate as f64 / rate as f64 - 0.5
                    + previous.end.unwrap() as f64;
                if at - previous.end.unwrap() as f64 >= previous.reach(band) as f64 {
                    self.previous = None;
                    None
                } else {
                    Some(at)
                }
            }
            None => None,
        };

        let current = self.current.as_mut().unwrap();
        let value =
            if rate == out && self.fraction == 0 && following_at.is_none() && previous_at.is_none()
            {
                // At the output rate and clear of any boundary: pass through exactly.
                let x = current.frame(self.index as i64);
                [x[0] as f64, x[1] as f64]
            } else {
                // Away from boundaries the band limit is this segment's own.
                let band = if following_at.is_some() || previous_at.is_some() {
                    band
                } else {
                    out.min(rate)
                };
                let mut v = current.value_at(position, band);
                if let (Some(at), Some(next)) = (following_at, self.following.as_mut()) {
                    let w = next.value_at(at, band);
                    v = [v[0] + w[0], v[1] + w[1]];
                }
                if let (Some(at), Some(previous)) = (previous_at, self.previous.as_mut()) {
                    let w = previous.value_at(at, band);
                    v = [v[0] + w[0], v[1] + w[1]];
                }
                v
            };

        // Exact rational step: no drift however long the stream runs. While
        // gliding onto whole frames, each step is nudged by a share of the
        // remaining offset (about 100 ppm for a tenth of a second).
        let mut step = rate as i64;
        if let Some((left, units)) = self.glide.as_mut() {
            let nudge = *units / *left as i64;
            step += nudge;
            *units -= nudge;
            *left -= 1;
            if *left == 0 {
                self.glide = None;
            }
        }
        self.fraction = (self.fraction as i64 + step) as u64;
        while self.fraction >= out as u64 {
            self.fraction -= out as u64;
            self.index += 1;
        }
        let current = self.current.as_mut().unwrap();
        current.trim((self.index + 1).saturating_sub(reach));
        // f32 → f64 → f32 is exact, so passed-through frames stay bit-exact.
        Some([value[0] as f32, value[1] as f32])
    }
}

impl<S: Source> Iterator for OutputStage<S> {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        if self.channel == 2 {
            self.frame = self.next_frame()?;
            self.channel = 0;
        }
        let sample = self.frame[self.channel];
        self.channel += 1;
        Some(sample)
    }
}

impl<S: Source> Source for OutputStage<S> {
    fn current_span_len(&self) -> Option<usize> {
        // The format never changes, so there are no spans to announce.
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

#[cfg(test)]
pub(crate) mod test_audio {
    //! Deterministic test signals, a WAV writer, and measurements shared by
    //! the output-stage and device-output tests.

    use std::f64::consts::TAU;
    use std::num::NonZero;
    use std::path::Path;

    pub fn nz(rate: u32) -> NonZero<u32> {
        NonZero::new(rate).unwrap()
    }

    pub const STEREO: NonZero<u16> = NonZero::new(2).unwrap();

    /// Stereo sine (left sin, right cos) following real time from `t0`, so
    /// pieces at any rates continue one another exactly.
    pub fn tone(freq: f64, amp: f64, rate: u32, t0: f64, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|n| {
                let p = TAU * freq * (t0 + n as f64 / rate as f64);
                [(amp * p.sin()) as f32, (amp * p.cos()) as f32]
            })
            .collect()
    }

    /// The same tone at `rate` as the ideal output to compare with.
    pub fn ideal(freq: f64, amp: f64, rate: u32, frames: usize) -> Vec<f32> {
        tone(freq, amp, rate, 0.0, frames)
    }

    /// 16-bit PCM WAV with `channels` interleaved channels.
    pub fn write_wav(path: &Path, rate: u32, channels: u16, samples: &[f32]) {
        let data: Vec<u8> = samples
            .iter()
            .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes())
            .collect();
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        out.extend_from_slice(&(channels * 2).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&data);
        std::fs::write(path, out).unwrap();
    }

    /// 16-bit stereo AIFF.
    pub fn write_aiff(path: &Path, rate: u32, samples: &[f32]) {
        let mut ssnd = vec![0u8; 8];
        for s in samples {
            ssnd.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_be_bytes());
        }
        let exponent = 16383 + (31 - rate.leading_zeros()) as u16;
        let mantissa = (rate as u64) << (63 - (exponent - 16383));
        let mut comm = Vec::new();
        comm.extend_from_slice(&2i16.to_be_bytes());
        comm.extend_from_slice(&((samples.len() / 2) as u32).to_be_bytes());
        comm.extend_from_slice(&16i16.to_be_bytes());
        comm.extend_from_slice(&exponent.to_be_bytes());
        comm.extend_from_slice(&mantissa.to_be_bytes());
        let mut body = b"AIFF".to_vec();
        body.extend_from_slice(b"COMM");
        body.extend_from_slice(&(comm.len() as u32).to_be_bytes());
        body.extend_from_slice(&comm);
        body.extend_from_slice(b"SSND");
        body.extend_from_slice(&(ssnd.len() as u32).to_be_bytes());
        body.extend_from_slice(&ssnd);
        let mut out = b"FORM".to_vec();
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&body);
        std::fs::write(path, out).unwrap();
    }

    /// Decodes a file the way the application does.
    pub fn decoder(path: &Path) -> rodio::Decoder<std::io::BufReader<std::fs::File>> {
        let file = std::fs::File::open(path).unwrap();
        let len = file.metadata().unwrap().len();
        rodio::Decoder::builder()
            .with_data(std::io::BufReader::new(file))
            .with_byte_len(len)
            .with_seekable(true)
            .with_hint(path.extension().and_then(|e| e.to_str()).unwrap_or("wav"))
            .build()
            .unwrap()
    }

    pub fn left(x: &[f32]) -> Vec<f64> {
        x.iter().step_by(2).map(|&v| v as f64).collect()
    }

    /// Amplitude of `freq` in `x` (both quadratures).
    pub fn amplitude(x: &[f64], rate: f64, freq: f64) -> f64 {
        let w = TAU * freq / rate;
        let (mut s, mut c) = (0.0, 0.0);
        for (i, v) in x.iter().enumerate() {
            s += v * (w * i as f64).sin();
            c += v * (w * i as f64).cos();
        }
        2.0 * (s * s + c * c).sqrt() / x.len() as f64
    }

    /// THD+N in dB: what remains after removing the best-fitting sine at
    /// `freq` (any phase) and DC, relative to the signal.
    pub fn thd_n_db(x: &[f64], rate: f64, freq: f64) -> f64 {
        let w = TAU * freq / rate;
        let basis = |i: usize| [(w * i as f64).sin(), (w * i as f64).cos(), 1.0];
        let mut m = [[0.0f64; 3]; 3];
        let mut v = [0.0f64; 3];
        for (i, &y) in x.iter().enumerate() {
            let b = basis(i);
            for r in 0..3 {
                v[r] += b[r] * y;
                for c in 0..3 {
                    m[r][c] += b[r] * b[c];
                }
            }
        }
        // Cramer's rule for the 3×3 normal equations.
        let det = |a: [[f64; 3]; 3]| {
            a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
                - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
                + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0])
        };
        let d = det(m);
        let c: [f64; 3] = std::array::from_fn(|col| {
            let mut a = m;
            for (row, value) in v.iter().enumerate() {
                a[row][col] = *value;
            }
            det(a) / d
        });
        let (mut residual, mut total) = (0.0, 0.0);
        for (i, &y) in x.iter().enumerate() {
            let b = basis(i);
            let fit = c[0] * b[0] + c[1] * b[1] + c[2];
            residual += (y - fit) * (y - fit);
            total += y * y;
        }
        10.0 * (residual / total).max(1e-30).log10()
    }

    /// Worst THD+N over 100 ms blocks, so slow drift cannot hide in a long fit.
    pub fn worst_block_thd(x: &[f64], rate: f64, freq: f64) -> f64 {
        let block = rate as usize / 10;
        x.chunks(block)
            .filter(|c| c.len() == block)
            .map(|c| thd_n_db(c, rate, freq))
            .fold(f64::MIN, f64::max)
    }

    pub fn max_difference(a: &[f32], b: &[f32]) -> f32 {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::test_audio::*;
    use super::*;
    use rodio::buffer::SamplesBuffer;
    use rodio::Player;

    fn through(input: impl Source, out_rate: u32) -> Vec<f32> {
        OutputStage::new(input, nz(out_rate), Arc::default()).collect()
    }

    /// A Player whose queue feeds an output stage, as in DeviceOutput.
    struct Chain {
        player: Player,
        starts: Arc<TrackStarts>,
        stage: OutputStage<rodio::queue::SourcesQueueOutput>,
    }

    fn chain(out_rate: u32) -> Chain {
        let (player, queue) = Player::new();
        let starts = Arc::new(TrackStarts::default());
        let stage = OutputStage::new(queue, nz(out_rate), Arc::clone(&starts));
        Chain {
            player,
            starts,
            stage,
        }
    }

    impl Chain {
        fn append(&self, source: impl Source + Send + 'static) {
            self.player.append(Marked::new(source, &self.starts));
        }
        fn pull(&mut self, samples: usize) -> Vec<f32> {
            self.stage.by_ref().take(samples).collect()
        }
    }

    fn db(v: f64) -> f64 {
        20.0 * v.log10()
    }

    #[test]
    fn audio_at_the_output_rate_passes_through_bit_exact() {
        // Deterministic noise-like content at 48 kHz.
        let samples: Vec<f32> = (0..96_000u32)
            .map(|i| ((i.wrapping_mul(2_654_435_761) >> 8) as f32 / (1 << 24) as f32 - 0.5) * 0.8)
            .collect();
        let out = through(
            SamplesBuffer::new(STEREO, nz(48_000), samples.clone()),
            48_000,
        );
        assert_eq!(out.len(), samples.len());
        assert!(out
            .iter()
            .zip(&samples)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
    }

    #[test]
    fn converting_44k1_to_48k_is_clean_and_flat_to_20_khz() {
        for freq in [997.0, 4_999.0, 9_997.0, 15_997.0, 19_997.0] {
            let input = tone(freq, 0.5, 44_100, 0.0, 44_100);
            let out = left(&through(
                SamplesBuffer::new(STEREO, nz(44_100), input),
                48_000,
            ));
            let middle = &out[4_800..43_200];
            let gain = db(amplitude(middle, 48_000.0, freq) / 0.5);
            let thd = worst_block_thd(middle, 48_000.0, freq);
            println!("44.1 to 48 kHz, {freq} Hz: gain {gain:+.4} dB, worst THD+N {thd:.1} dB");
            assert!(gain.abs() < 0.01, "{freq} Hz: gain {gain:.4} dB");
            assert!(thd < -85.0, "{freq} Hz: THD+N {thd:.1} dB");
        }
    }

    #[test]
    fn converting_48k_to_44k1_is_clean_and_suppresses_aliases() {
        let input = tone(997.0, 0.5, 48_000, 0.0, 48_000);
        let out = left(&through(
            SamplesBuffer::new(STEREO, nz(48_000), input),
            44_100,
        ));
        let middle = &out[4_410..39_690];
        let gain = db(amplitude(middle, 44_100.0, 997.0) / 0.5);
        let thd = worst_block_thd(middle, 44_100.0, 997.0);
        // 23 kHz cannot exist at 44.1 kHz; it would fold to 21.1 kHz.
        let input = tone(23_000.0, 0.5, 48_000, 0.0, 48_000);
        let out = left(&through(
            SamplesBuffer::new(STEREO, nz(48_000), input),
            44_100,
        ));
        let folded = db(amplitude(&out[4_410..39_690], 44_100.0, 21_100.0) / 0.5);
        println!("48 to 44.1 kHz: 997 Hz gain {gain:+.4} dB, THD+N {thd:.1} dB; 23 kHz alias {folded:.1} dB");
        assert!(gain.abs() < 0.01);
        assert!(thd < -85.0);
        assert!(folded < -85.0, "alias at {folded:.1} dB");
    }

    #[test]
    fn output_length_is_exact_and_timing_does_not_drift() {
        // 10 s at 44.1 kHz must become exactly 10 s at 48 kHz.
        let input = tone(997.0, 0.5, 44_100, 0.0, 441_000);
        let out = through(SamplesBuffer::new(STEREO, nz(44_100), input), 48_000);
        assert_eq!(out.len() / 2, 480_000);
        // Matches the ideal tone at 48 kHz from start to end: no drift.
        let reference = ideal(997.0, 0.5, 48_000, 480_000);
        let early = max_difference(&out[200..48_000], &reference[200..48_000]);
        let late = max_difference(&out[900_000..959_800], &reference[900_000..959_800]);
        println!("10 s at 44.1 kHz: largest deviation from the ideal 48 kHz tone: first second {early:e}, last second {late:e}");
        assert!(
            early < 1e-4 && late < 1e-4,
            "early {early:e}, late {late:e}"
        );
    }

    #[test]
    fn decoder_packets_cause_no_steps() {
        // A real decoder delivers 2304-sample packets; the rodio converter
        // restarted at each one (47 timing steps in 2 s in the PoC).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        write_wav(&path, 44_100, 2, &tone(997.0, 0.5, 44_100, 0.0, 88_200));
        let mut c = chain(48_000);
        c.append(decoder(&path));
        let out = c.pull(96_000 * 2);
        let reference = ideal(997.0, 0.5, 48_000, 96_000);
        // 16-bit quantisation alone allows about 3e-5.
        let worst = max_difference(&out[200..95_000 * 2], &reference[200..95_000 * 2]);
        // From 100 ms in: the first block holds the signal's own abrupt start.
        let thd = worst_block_thd(&left(&out[4_800 * 2..95_000 * 2]), 48_000.0, 997.0);
        println!("decoded 16-bit WAV, 44.1 to 48 kHz: largest deviation {worst:e}, worst THD+N {thd:.1} dB");
        assert!(worst < 2e-4, "largest deviation {worst:e}");
        assert!(thd < -85.0);
    }

    #[test]
    fn a_boundary_between_tracks_at_the_same_rate_is_continuous() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.wav");
        let b = dir.path().join("b.wav");
        // The join falls mid-cycle and off any packet boundary.
        let frames_a = 66_187;
        write_wav(&a, 44_100, 2, &tone(997.0, 0.5, 44_100, 0.0, frames_a));
        write_wav(
            &b,
            44_100,
            2,
            &tone(997.0, 0.5, 44_100, frames_a as f64 / 44_100.0, 44_100),
        );
        let mut c = chain(48_000);
        c.append(decoder(&a));
        c.append(decoder(&b));
        let total = ((frames_a + 44_100) as f64 * 48_000.0 / 44_100.0) as usize;
        let out = c.pull(total * 2);
        let reference = ideal(997.0, 0.5, 48_000, total);
        let join = (frames_a as f64 * 48_000.0 / 44_100.0) as usize;
        let range = (join - 480) * 2..(join + 480) * 2;
        let around = max_difference(&out[range.clone()], &reference[range]);
        println!("44.1 kHz track to 44.1 kHz track: largest deviation within 10 ms of the join {around:e}");
        assert!(around < 2e-4, "deviation at the join {around:e}");
    }

    /// One continuous tone split across two files at different rates, joined
    /// in the queue: what comes out, and where the join is.
    fn continuous_pair(ra: u32, rb: u32, out_rate: u32) -> (Vec<f32>, usize, usize) {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.wav");
        let b = dir.path().join("b.wav");
        // The join falls mid-cycle and off any packet boundary.
        let frames_a = ra as usize + 37;
        let frames_b = rb as usize / 2 + 11;
        write_wav(&a, ra, 2, &tone(997.0, 0.5, ra, 0.0, frames_a));
        write_wav(
            &b,
            rb,
            2,
            &tone(997.0, 0.5, rb, frames_a as f64 / ra as f64, frames_b),
        );
        let mut c = chain(out_rate);
        c.append(decoder(&a));
        c.append(decoder(&b));
        let ideal_frames = ((frames_a as f64 / ra as f64 + frames_b as f64 / rb as f64)
            * out_rate as f64)
            .round() as usize;
        let mut out = c.pull((ideal_frames + 4_000) * 2);
        // The tone (sin, cos) has constant power 0.25; where it falls below
        // half is where B ends (the filter's ringing after an abrupt end is
        // not counted as length).
        let end = out
            .chunks(2)
            .rposition(|f| f[0] * f[0] + f[1] * f[1] > 0.125)
            .unwrap()
            + 1;
        out.truncate(end * 2);
        let join = (frames_a as f64 * out_rate as f64 / ra as f64).round() as usize;
        (out, join, ideal_frames)
    }

    #[test]
    fn a_change_of_rate_keeps_a_continuous_signal_continuous() {
        for (ra, rb, out_rate) in [
            (44_100, 48_000, 48_000),
            (48_000, 44_100, 48_000),
            (44_100, 48_000, 44_100),
            (48_000, 44_100, 44_100),
        ] {
            let (out, join, ideal_frames) = continuous_pair(ra, rb, out_rate);
            let reference = ideal(997.0, 0.5, out_rate, ideal_frames);
            // Around the join the output follows the tone: no gap, no dip.
            let range = (join - 480) * 2..(join + 480) * 2;
            let worst = max_difference(&out[range.clone()], &reference[range.clone()]);
            let silent = out[range]
                .chunks(2)
                .filter(|f| f[0] == 0.0 && f[1] == 0.0)
                .count();
            // Exact length: within one frame of the two files' total duration.
            let length = out.len() as i64 / 2 - ideal_frames as i64;
            println!("{ra} → {rb} Hz at {out_rate} Hz: largest deviation within 10 ms of the join {worst:.5}, length error {length} frames");
            assert_eq!(silent, 0, "{ra} → {rb}: silence at the join");
            // 0.5 amplitude: 0.01 is −34 dB relative to the tone, for under a millisecond.
            assert!(worst < 0.01, "{ra} → {rb} at {out_rate}: deviation {worst}");
            assert!(
                length.abs() <= 1,
                "{ra} → {rb} at {out_rate}: {length} frames"
            );
            // Timing holds afterwards: still on the ideal tone half a second on
            // (within the half-frame glide for a track at the output rate).
            let late = (ideal_frames - 2_000) * 2..(ideal_frames - 1_000) * 2;
            let after = max_difference(&out[late.clone()], &reference[late]);
            assert!(
                after < 0.035,
                "{ra} → {rb} at {out_rate}: drifted by {after}"
            );
        }
    }

    #[test]
    fn after_a_change_of_rate_a_track_at_the_output_rate_is_bit_exact_again() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.wav");
        let b = dir.path().join("b.wav");
        write_wav(&a, 44_100, 2, &tone(997.0, 0.5, 44_100, 0.0, 44_100 + 37));
        write_wav(&b, 48_000, 2, &tone(440.0, 0.5, 48_000, 0.0, 48_000));
        let b_samples: Vec<f32> = decoder(&b).collect();
        let mut c = chain(48_000);
        c.append(decoder(&a));
        c.append(decoder(&b));
        let out = c.pull(96_000 * 2);
        // After the 0.1 s glide onto whole frames, B is passed through exactly,
        // within a frame of where its start fell.
        let join = ((44_100.0 + 37.0) * 48_000.0 / 44_100.0) as usize;
        let window = &out[(join + 6_000) * 2..(join + 6_100) * 2];
        let found = (5_998..6_003).find(|&k| {
            window
                .iter()
                .zip(&b_samples[k * 2..])
                .all(|(x, y)| x.to_bits() == y.to_bits())
        });
        assert!(found.is_some(), "B is not bit-exact after the glide");
    }

    #[test]
    fn a_change_of_channel_count_between_tracks_is_followed() {
        // In-memory buffers report themselves exhausted after their last
        // sample; decoders do not. Both orders must land on the right frame.
        let mono: Vec<f32> = (0..1_001).map(|i| 0.001 * i as f32).collect();
        let stereo: Vec<f32> = (0..1_000).flat_map(|i| [0.3, -0.0001 * i as f32]).collect();
        let mut c = chain(48_000);
        c.append(SamplesBuffer::new(
            NonZero::new(1).unwrap(),
            nz(48_000),
            mono.clone(),
        ));
        c.append(SamplesBuffer::new(STEREO, nz(48_000), stereo.clone()));
        let out = c.pull((1_001 + 1_000) * 2);
        assert!(out[..2_002]
            .chunks(2)
            .zip(&mono)
            .all(|(f, m)| f[0] == *m && f[1] == *m));
        assert!(out[2_002..].iter().zip(&stereo).all(|(a, b)| a == b));

        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("mono.wav");
        let b = dir.path().join("stereo.wav");
        let mono_tone: Vec<f32> = tone(500.0, 0.4, 48_000, 0.0, 4_801)
            .into_iter()
            .step_by(2)
            .collect();
        write_wav(&a, 48_000, 1, &mono_tone);
        write_wav(&b, 48_000, 2, &tone(700.0, 0.4, 48_000, 0.0, 4_800));
        let want_a: Vec<f32> = decoder(&a).collect();
        let want_b: Vec<f32> = decoder(&b).collect();
        let mut c = chain(48_000);
        c.append(decoder(&a));
        c.append(decoder(&b));
        let out = c.pull((4_801 + 4_800) * 2);
        assert!(out[..4_801 * 2]
            .chunks(2)
            .zip(&want_a)
            .all(|(f, m)| f[0] == *m && f[1] == *m));
        assert!(out[4_801 * 2..].iter().zip(&want_b).all(|(x, y)| x == y));
    }

    #[test]
    fn its_format_never_changes() {
        // The mixer reads the format once; it must never need to read it again.
        let mut c = chain(48_000);
        assert_eq!(
            (c.stage.channels().get(), c.stage.sample_rate().get()),
            (2, 48_000)
        );
        assert_eq!(c.stage.current_span_len(), None);
        // An empty queue (mono silence), then a 44.1 kHz stereo source.
        let _ = c.pull(1_000);
        c.append(SamplesBuffer::new(
            STEREO,
            nz(44_100),
            tone(997.0, 0.5, 44_100, 0.0, 4_410),
        ));
        let _ = c.pull(4_000);
        assert_eq!(
            (c.stage.channels().get(), c.stage.sample_rate().get()),
            (2, 48_000)
        );
    }

    /// Not a correctness test: prints the cost per second of audio. Run with
    /// `cargo test --release --lib resample::tests::cost -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn cost() {
        let input = tone(997.0, 0.5, 44_100, 0.0, 44_100 * 30);
        let started = std::time::Instant::now();
        let n = through(SamplesBuffer::new(STEREO, nz(44_100), input), 48_000).len();
        let ms = started.elapsed().as_secs_f64() * 1000.0 / 30.0;
        println!("44.1 to 48 kHz: {ms:.2} ms of CPU per second of audio ({n} samples)");
    }
}
