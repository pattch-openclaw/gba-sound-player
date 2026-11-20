//! The WAV front end — E-R2's **convenience layer**, and nothing more.
//!
//! The contract boundary stays `album.f32` + `tracks.tsv`: this module produces
//! exactly the samples that pair would hold, in the same units (PCM16 scale, so a
//! 16-bit source needs no scaling at all), and `--emit-pcm` writes the pair out so
//! the equivalence is *checkable* rather than asserted. What it does **not** claim
//! is byte-equality at the WAV boundary: the resample inside it is not held to the
//! reference's bytes, and only its SNR is quoted (E-R2, "The acceptance question").
//!
//! Supported, on purpose and refusing everything else with a citation:
//!
//! - RIFF/WAVE, including `WAVE_FORMAT_EXTENSIBLE` whose subformat is PCM;
//! - integer PCM **8/16/24/32-bit** and IEEE **float 32-bit**;
//! - **one or two channels** — mono is duplicated, because the format is stereo
//!   and a downmix would be an album decision;
//! - **any rate**; 32,768 Hz bypasses the resampler entirely, which is what makes
//!   a natively-32,768 Hz stereo WAV reach *the same blob* as its PCM pair;
//! - `LIST/INFO` `INAM` (title) and `IART` (artist), which is what a lossless→WAV
//!   conversion leaves behind. `prep.py` gets those from `ffprobe`; reading them
//!   from the file is the no-external-tool version of the same thing.
//!
//! A truncated `data` chunk is a refusal rather than a short album. That is the
//! same rule the PCM boundary applies to a padded tail: the input's declared
//! length is the album's, and the encoder neither drops a tail nor invents one.

use std::path::Path;

use crate::Error;

/// The two tags the container carries, as the file states them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
}

/// A decoded WAV, at the units the encoder's PCM boundary uses.
#[derive(Clone, Debug)]
pub struct Wav {
    /// The file's own sample rate. `32_768` means no resampling at all.
    pub rate: u32,
    /// 1 or 2; `samples` is interleaved at this width.
    pub channels: u16,
    /// 8, 16, 24 or 32 for PCM; 32 for float.
    pub bits: u16,
    /// Interleaved samples at **PCM16 scale**: a 16-bit source arrives unchanged,
    /// which is why a 32,768 Hz pair reaches the identical blob.
    pub samples: Vec<f32>,
    pub tags: Tags,
}

impl Wav {
    /// Number of interleaved frames (a frame is one sample per channel).
    pub fn n_frames(&self) -> usize {
        self.samples.len() / self.channels as usize
    }
}

/// Read and decode one WAV file.
pub fn read(path: &Path) -> Result<Wav, Error> {
    let bytes = std::fs::read(path).map_err(|e| Error::Wav(format!("{}: {e}", path.display())))?;
    parse(&bytes).map_err(|e| match e {
        Error::Wav(m) => Error::Wav(format!("{}: {m}", path.display())),
        other => other,
    })
}

/// Decode RIFF/WAVE bytes. Split from [`read`] so the parser is testable without a
/// temporary file, and so a caller that already holds the bytes can use it.
pub fn parse(bytes: &[u8]) -> Result<Wav, Error> {
    if bytes.len() < 12 {
        return Err(bad(format!(
            "{} bytes is shorter than a RIFF header (12)",
            bytes.len()
        )));
    }
    if &bytes[0..4] != b"RIFF" {
        return Err(bad(format!("no RIFF signature (found {:?})", &bytes[0..4])));
    }
    if &bytes[8..12] != b"WAVE" {
        return Err(bad(format!("RIFF type is {:?}, not WAVE", &bytes[8..12])));
    }

    let mut fmt: Option<Fmt> = None;
    let mut data: Option<&[u8]> = None;
    let mut tags = Tags::default();

    // Chunk walk. Every chunk is `id size body`, padded to an even length; the
    // sizes are the file's, so an over-long one is refused rather than clamped.
    let mut off = 12usize;
    while off + 8 <= bytes.len() {
        let id = &bytes[off..off + 4];
        let size = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap()) as usize;
        let body_at = off + 8;
        let avail = bytes.len() - body_at;
        if size > avail {
            return Err(bad(format!(
                "chunk {:?} declares {size} bytes but {} remain: the file is truncated \
                 (RF64/streamed sizes are not supported, and a shortened album is not \
                 an album)",
                String::from_utf8_lossy(id),
                avail
            )));
        }
        let body = &bytes[body_at..body_at + size];
        match id {
            b"fmt " => fmt = Some(Fmt::parse(body)?),
            b"data" => {
                if data.replace(body).is_some() {
                    return Err(bad("more than one data chunk".to_string()));
                }
            }
            b"LIST" => tags = tags.or(info_tags(body)),
            _ => {}
        }
        // `size & 1` is the RIFF pad byte; it is allowed to be missing at EOF.
        off = body_at + size + (size & 1);
    }

    let fmt = fmt.ok_or_else(|| bad("no fmt chunk".to_string()))?;
    let data = data.ok_or_else(|| bad("no data chunk".to_string()))?;

    let block = fmt.block_align as usize;
    if block == 0 || block != (fmt.channels as usize * fmt.bits.div_ceil(8) as usize) {
        return Err(bad(format!(
            "block align {} disagrees with {} channel(s) of {}-bit samples ({})",
            fmt.block_align,
            fmt.channels,
            fmt.bits,
            fmt.channels as usize * fmt.bits.div_ceil(8) as usize
        )));
    }
    if data.len() % block != 0 {
        return Err(bad(format!(
            "data chunk is {} bytes, not a whole number of {block}-byte frames",
            data.len()
        )));
    }

    let samples = decode(data, &fmt)?;
    Ok(Wav {
        rate: fmt.rate,
        channels: fmt.channels,
        bits: fmt.bits,
        samples,
        tags,
    })
}

/// The `fmt ` chunk, after the format's own validation.
#[derive(Clone, Copy, Debug)]
struct Fmt {
    format: u16,
    channels: u16,
    rate: u32,
    block_align: u16,
    bits: u16,
}

impl Fmt {
    fn parse(body: &[u8]) -> Result<Self, Error> {
        if body.len() < 16 {
            return Err(bad(format!(
                "fmt chunk is {} bytes, shorter than the 16-byte PCM header",
                body.len()
            )));
        }
        let u16_at = |o: usize| u16::from_le_bytes(body[o..o + 2].try_into().unwrap());
        let u32_at = |o: usize| u32::from_le_bytes(body[o..o + 4].try_into().unwrap());
        let mut format = u16_at(0);
        if format == 0xFFFE {
            // WAVE_FORMAT_EXTENSIBLE: the real tag is the first halfword of the
            // subformat GUID, at 24. Some tools write 24-bit stereo this way.
            if body.len() < 40 || u16_at(16) < 22 {
                return Err(bad(
                    "WAVE_FORMAT_EXTENSIBLE without a full 22-byte extension".to_string(),
                ));
            }
            format = u16_at(24);
        }
        let channels = u16_at(2);
        let rate = u32_at(4);
        let block_align = u16_at(12);
        let bits = u16_at(14);

        if channels == 0 || channels > 2 {
            return Err(bad(format!(
                "{channels} channels: this front end takes 1 (duplicated) or 2, because \
                 the format is stereo and a downmix is an album decision, not a \
                 convenience"
            )));
        }
        if rate == 0 {
            return Err(bad("a sample rate of 0 Hz".to_string()));
        }
        match (format, bits) {
            (1, 8 | 16 | 24 | 32) | (3, 32) => {}
            (1, b) => {
                return Err(bad(format!(
                    "{b}-bit integer PCM: E-R2 names 8/16/24/32, and a narrower or \
                     wider sample would have to be requantised silently"
                )));
            }
            (3, b) => {
                return Err(bad(format!("{b}-bit float: only 32-bit float is read")));
            }
            (f, _) => {
                return Err(bad(format!(
                    "format tag {f} is not PCM (1) or IEEE float (3); other codecs \
                     are not WAV-front-end business — decode them to WAV first"
                )));
            }
        }
        Ok(Self {
            format,
            channels,
            rate,
            block_align,
            bits,
        })
    }

    fn bytes_per_sample(&self) -> usize {
        self.bits as usize / 8
    }
}

/// Decode the sample bytes to interleaved PCM16-scale f32.
fn decode(data: &[u8], fmt: &Fmt) -> Result<Vec<f32>, Error> {
    let width = fmt.bytes_per_sample();
    let n = data.len() / width;
    let mut out = Vec::with_capacity(n);
    let mut non_finite = 0usize;
    match (fmt.format, fmt.bits) {
        (1, 8) => {
            // 8-bit WAV is unsigned, centred at 128; the scale is 256 per PCM16
            // step, so a 16-bit source and an 8-bit one land on one scale.
            for &b in data {
                out.push((i32::from(b) - 128) as f32 * 256.0);
            }
        }
        (1, 16) => {
            for c in data.chunks_exact(2) {
                out.push(f32::from(i16::from_le_bytes([c[0], c[1]])));
            }
        }
        (1, 24) => {
            for c in data.chunks_exact(3) {
                let v = (i32::from(c[0]) | i32::from(c[1]) << 8 | i32::from(c[2]) << 16) << 8 >> 8;
                out.push(v as f32 * (1.0 / 256.0));
            }
        }
        (1, 32) => {
            for c in data.chunks_exact(4) {
                let v = i32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                out.push(v as f32 * (1.0 / 65536.0));
            }
        }
        (3, 32) => {
            for c in data.chunks_exact(4) {
                let v = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                if !v.is_finite() {
                    non_finite += 1;
                }
                out.push(v * 32768.0);
            }
        }
        (f, b) => {
            // `Fmt::parse` already refused this; the arm exists so a future edit
            // to the match above cannot silently decode a format it cannot read.
            return Err(bad(format!("cannot decode format tag {f}, {b} bits")));
        }
    }
    if non_finite != 0 {
        return Err(bad(format!(
            "{non_finite} non-finite float sample(s): a NaN or infinity would poison the \
             MDCT and the SNR, so it is refused at the boundary rather than carried in"
        )));
    }
    Ok(out)
}

impl Tags {
    /// Merge the second file's tags under the first's: a `LIST/INFO` chunk read
    /// once is the whole source of truth, so this exists only for chunk order.
    fn or(self, other: Tags) -> Tags {
        Tags {
            title: self.title.or(other.title),
            artist: self.artist.or(other.artist),
        }
    }
}

/// The `LIST/INFO` sub-chunks the container carries, NUL-terminated and possibly
/// not UTF-8 (a Latin-1 tag is common in old files: lossy-decode, do not refuse).
fn info_tags(body: &[u8]) -> Tags {
    if body.len() < 4 || &body[0..4] != b"INFO" {
        return Tags::default();
    }
    let mut tags = Tags::default();
    let mut off = 4usize;
    while off + 8 <= body.len() {
        let id = &body[off..off + 4];
        let size = u32::from_le_bytes(body[off + 4..off + 8].try_into().unwrap()) as usize;
        let at = off + 8;
        if size > body.len() - at {
            break;
        }
        let raw = &body[at..at + size];
        let text = String::from_utf8_lossy(raw.split(|&b| b == 0).next().unwrap_or(raw))
            .trim()
            .to_string();
        let text = (!text.is_empty()).then_some(text);
        match id {
            b"INAM" => tags.title = text.or(tags.title),
            b"IART" => tags.artist = text.or(tags.artist),
            _ => {}
        }
        off = at + size + (size & 1);
    }
    tags
}

fn bad(message: String) -> Error {
    Error::Wav(message)
}
