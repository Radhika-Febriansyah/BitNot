//! Deterministic test signals and minimal file writers (WAV, AIFF, FLAC).
//!
//! No encoder is installed on the test machine, so the files are written here.
//! FLAC uses VERBATIM subframes only: valid FLAC that any decoder must read
//! bit-exactly, which is all the experiments need.

use std::f64::consts::TAU;
use std::fs;
use std::path::Path;

pub type Frames = Vec<[f64; 2]>;

/// Stereo sine: left is sin, right is cos, so a channel swap or a lost frame is
/// visible. `start` is the first frame index, so two calls continue each other
/// with no phase break.
pub fn stereo_sine(freq: f64, amp: f64, rate: u32, start: usize, frames: usize) -> Frames {
    (start..start + frames)
        .map(|n| {
            let phase = TAU * freq * n as f64 / rate as f64;
            [amp * phase.sin(), amp * phase.cos()]
        })
        .collect()
}

/// Two tones summed (for spectrum checks).
pub fn two_tones(f1: f64, f2: f64, amp: f64, rate: u32, frames: usize) -> Frames {
    (0..frames)
        .map(|n| {
            let t = n as f64 / rate as f64;
            let v = amp * 0.5 * ((TAU * f1 * t).sin() + (TAU * f2 * t).sin());
            [v, v]
        })
        .collect()
}

/// Deterministic noise with a gentle low-pass tilt (music-like spectrum),
/// different per seed and uncorrelated between seeds.
pub fn noise(seed: u64, amp: f64, frames: usize) -> Frames {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    };
    let (mut l, mut r) = (0.0, 0.0);
    (0..frames)
        .map(|_| {
            // One-pole low-pass mixed with the raw noise: about -6 dB/octave above 1 kHz.
            l = 0.86 * l + 0.14 * next();
            r = 0.86 * r + 0.14 * next();
            [amp * 2.2 * l, amp * 2.2 * r]
        })
        .collect()
}

fn quantize(x: f64, bits: u32) -> i32 {
    let max = ((1i64 << (bits - 1)) - 1) as f64;
    (x * max).round().clamp(-max - 1.0, max) as i32
}

/// The exact values a decoder should return for `frames` written at `bits`.
pub fn as_decoded(frames: &Frames, bits: u32) -> Vec<f32> {
    let scale = (1i64 << (bits - 1)) as f64;
    frames
        .iter()
        .flat_map(|f| f.map(|x| (quantize(x, bits) as f64 / scale) as f32))
        .collect()
}

pub fn write_wav16(path: &Path, rate: u32, frames: &Frames) {
    let mut data = Vec::with_capacity(frames.len() * 4);
    for f in frames {
        for x in f {
            data.extend_from_slice(&(quantize(*x, 16) as i16).to_le_bytes());
        }
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    fs::write(path, out).expect("write wav");
}

/// 80-bit IEEE extended float, as AIFF stores the sample rate.
fn extended(rate: u32) -> [u8; 10] {
    let exponent = 16383 + (31 - rate.leading_zeros()) as u16;
    let mantissa = (rate as u64) << (63 - (exponent - 16383));
    let mut out = [0u8; 10];
    out[..2].copy_from_slice(&exponent.to_be_bytes());
    out[2..].copy_from_slice(&mantissa.to_be_bytes());
    out
}

pub fn write_aiff16(path: &Path, rate: u32, frames: &Frames) {
    let mut ssnd = vec![0u8; 8]; // offset and block size
    for f in frames {
        for x in f {
            ssnd.extend_from_slice(&(quantize(*x, 16) as i16).to_be_bytes());
        }
    }
    let mut comm = Vec::new();
    comm.extend_from_slice(&2i16.to_be_bytes());
    comm.extend_from_slice(&(frames.len() as u32).to_be_bytes());
    comm.extend_from_slice(&16i16.to_be_bytes());
    comm.extend_from_slice(&extended(rate));
    let mut body = Vec::new();
    body.extend_from_slice(b"AIFF");
    body.extend_from_slice(b"COMM");
    body.extend_from_slice(&(comm.len() as u32).to_be_bytes());
    body.extend_from_slice(&comm);
    body.extend_from_slice(b"SSND");
    body.extend_from_slice(&(ssnd.len() as u32).to_be_bytes());
    body.extend_from_slice(&ssnd);
    let mut out = b"FORM".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    fs::write(path, out).expect("write aiff");
}

struct Bits {
    bytes: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn new() -> Self {
        Bits {
            bytes: Vec::new(),
            acc: 0,
            n: 0,
        }
    }
    fn put(&mut self, value: u64, width: u32) {
        for i in (0..width).rev() {
            self.acc = (self.acc << 1) | ((value >> i) & 1);
            self.n += 1;
            if self.n == 8 {
                self.bytes.push(self.acc as u8);
                self.acc = 0;
                self.n = 0;
            }
        }
    }
}

fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x8005
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// UTF-8-style coded frame number (FLAC frame header).
fn coded_number(n: u32) -> Vec<u8> {
    match n {
        0..=0x7F => vec![n as u8],
        0x80..=0x7FF => vec![0xC0 | (n >> 6) as u8, 0x80 | (n & 0x3F) as u8],
        0x800..=0xFFFF => vec![
            0xE0 | (n >> 12) as u8,
            0x80 | ((n >> 6) & 0x3F) as u8,
            0x80 | (n & 0x3F) as u8,
        ],
        _ => panic!("too many frames for this test writer"),
    }
}

/// Writes stereo FLAC with VERBATIM subframes (16 or 24 bit; 44.1 or 48 kHz).
pub fn write_flac(path: &Path, rate: u32, bits: u32, frames: &Frames) {
    const BLOCK: usize = 4096;
    let rate_code = match rate {
        44_100 => 0b1001,
        48_000 => 0b1010,
        _ => panic!("unsupported rate"),
    };
    let size_code = match bits {
        16 => 0b100,
        24 => 0b110,
        _ => panic!("unsupported bits"),
    };
    let mut out = b"fLaC".to_vec();
    // STREAMINFO, the last metadata block.
    let mut info = Bits::new();
    info.put(BLOCK as u64, 16);
    info.put(BLOCK as u64, 16);
    info.put(0, 24);
    info.put(0, 24);
    info.put(rate as u64, 20);
    info.put(1, 3); // channels - 1
    info.put((bits - 1) as u64, 5);
    info.put(frames.len() as u64, 36);
    info.bytes.extend_from_slice(&[0u8; 16]); // MD5 unknown
    out.push(0x80); // last block, type 0
    out.extend_from_slice(&(info.bytes.len() as u32).to_be_bytes()[1..]);
    out.extend_from_slice(&info.bytes);

    for (index, chunk) in frames.chunks(BLOCK).enumerate() {
        let mut header = vec![
            0xFF,
            0xF8,
            (0b0111 << 4) | rate_code,
            (0b0001 << 4) | (size_code << 1),
        ];
        header.extend(coded_number(index as u32));
        header.extend_from_slice(&((chunk.len() - 1) as u16).to_be_bytes());
        header.push(crc8(&header));
        let mut body = Bits::new();
        for ch in 0..2 {
            body.put(0b0000_0010, 8); // VERBATIM, no wasted bits
            for f in chunk {
                let v = quantize(f[ch], bits);
                body.put((v as u32 as u64) & ((1u64 << bits) - 1), bits);
            }
        }
        let mut frame = header;
        frame.extend_from_slice(&body.bytes);
        assert_eq!(body.n, 0, "frame is byte aligned");
        let crc = crc16(&frame);
        frame.extend_from_slice(&crc.to_be_bytes());
        out.extend_from_slice(&frame);
    }
    fs::write(path, out).expect("write flac");
}
