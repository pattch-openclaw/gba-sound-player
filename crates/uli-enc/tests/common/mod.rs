//! Shared test scaffolding: temp dirs, the two input files, and the repo root.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// A uniquely named directory under the system temp dir, removed on drop.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(tag: &str) -> Self {
        for n in 0.. {
            let path =
                std::env::temp_dir().join(format!("uli-enc-{tag}-{}-{n}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("temp dir {}: {e}", path.display()),
            }
        }
        unreachable!()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A silent `album.f32` of `n_samples` interleaved stereo frames.
pub fn silence_album(dir: &Path, n_samples: usize) -> PathBuf {
    let path = dir.join("album.f32");
    std::fs::write(&path, vec![0u8; n_samples * 8]).expect("write album.f32");
    path
}

/// Replace the album's bytes with an arbitrary payload (for the size check).
pub fn raw_album(dir: &Path, bytes: &[u8]) -> PathBuf {
    let path = dir.join("album.f32");
    std::fs::write(&path, bytes).expect("write album.f32");
    path
}

/// A one-track `tracks.tsv` covering `n_samples`.
pub fn single_track(dir: &Path, n_samples: u32) -> PathBuf {
    write_tracks(
        dir,
        &format!(
            "# n start len title artist ({} Hz samples)\n\
             1\t0\t{n_samples}\tSilence\tuli-enc test\n",
            uli_lite::format::RATE
        ),
    )
}

pub fn write_tracks(dir: &Path, text: &str) -> PathBuf {
    let path = dir.join("tracks.tsv");
    std::fs::write(&path, text).expect("write tracks.tsv");
    path
}

/// The repository root, derived from this crate's manifest dir (`crates/<name>`).
///
/// Test-relative paths would otherwise anchor to the manifest, which is how
/// `uli-lite-gba/tests/premise.rs` ended up pointing three `../` *outside* the
/// repo and reporting four on-disk corpora as absent. Same trap, one home.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|crates| crates.parent())
        .expect("this crate lives at <repo>/crates/<name>")
        .to_path_buf()
}

/// Write frozen fixture bytes into `dir` so a test can hand them to code that
/// takes a path rather than a slice. The bytes are `include_bytes!`-ed by the
/// test, so a missing fixture is a compile error, never a skipped assertion.
pub fn materialise(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write fixture");
    path
}

/// A WAV's `fmt ` fields, as the S5 front end's tests state them.
#[derive(Clone, Copy, Debug)]
pub struct WavSpec {
    /// 1 = integer PCM, 3 = IEEE float, 0xFFFE = extensible (subformat 1).
    pub tag: u16,
    pub channels: u16,
    pub rate: u32,
    pub bits: u16,
    pub title: Option<&'static str>,
    pub artist: Option<&'static str>,
}

impl WavSpec {
    pub fn pcm(channels: u16, rate: u32, bits: u16) -> Self {
        Self {
            tag: 1,
            channels,
            rate,
            bits,
            title: None,
            artist: None,
        }
    }
}

/// Serialise a RIFF/WAVE file whose payload is the *native* encoding of `samples`
/// (for 8-bit PCM, unsigned 0..=255; otherwise two's complement little-endian).
///
/// Tests build their inputs this way rather than committing binaries: the point of
/// the WAV front end is that it reads a format, and a hand-built header is the only
/// way to exercise 24-bit or a mono file without carrying a fixture per case.
pub fn wav_bytes(spec: &WavSpec, samples: &[i32]) -> Vec<u8> {
    let width = spec.bits.div_ceil(8) as usize;
    let mut data = Vec::with_capacity(samples.len() * width);
    for &s in samples {
        match spec.bits {
            8 => data.push(s as u8),
            16 => data.extend_from_slice(&(s as i16).to_le_bytes()),
            24 => {
                let b = s.to_le_bytes();
                data.extend_from_slice(&b[0..3]);
            }
            32 => data.extend_from_slice(&s.to_le_bytes()),
            other => panic!("no test encoding for {other} bits"),
        }
    }
    wav_with_payload(spec, &data)
}

/// A float32 WAV, whose payload is the samples' raw f32 bits.
pub fn wav_bytes_f32(spec: &WavSpec, samples: &[f32]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    wav_with_payload(spec, &data)
}

fn wav_with_payload(spec: &WavSpec, data: &[u8]) -> Vec<u8> {
    let mut chunks: Vec<u8> = Vec::new();

    // fmt: the 16-byte PCM header, plus the 22-byte extension for extensible.
    let fmt_len = if spec.tag == 0xFFFE { 40 } else { 16 };
    let mut fmt = Vec::with_capacity(fmt_len);
    let effective_tag = if spec.tag == 0xFFFE { 1 } else { spec.tag };
    fmt.extend_from_slice(&spec.tag.to_le_bytes());
    fmt.extend_from_slice(&spec.channels.to_le_bytes());
    fmt.extend_from_slice(&spec.rate.to_le_bytes());
    let block = spec.channels as u32 * u32::from(spec.bits.div_ceil(8));
    fmt.extend_from_slice(&(spec.rate * block).to_le_bytes());
    fmt.extend_from_slice(&(block as u16).to_le_bytes());
    fmt.extend_from_slice(&spec.bits.to_le_bytes());
    if spec.tag == 0xFFFE {
        fmt.extend_from_slice(&22u16.to_le_bytes());
        fmt.extend_from_slice(&spec.bits.to_le_bytes());
        fmt.extend_from_slice(&0u32.to_le_bytes());
        fmt.extend_from_slice(&effective_tag.to_le_bytes());
        fmt.extend_from_slice(&[0u8; 14]);
    }
    chunks.extend_from_slice(b"fmt ");
    chunks.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    chunks.extend_from_slice(&fmt);
    if fmt.len() % 2 != 0 {
        chunks.push(0);
    }

    if spec.title.is_some() || spec.artist.is_some() {
        let mut info = Vec::new();
        info.extend_from_slice(b"INFO");
        for (id, value) in [(b"INAM", spec.title), (b"IART", spec.artist)] {
            let Some(value) = value else { continue };
            let mut text = value.as_bytes().to_vec();
            text.push(0);
            info.extend_from_slice(id);
            info.extend_from_slice(&(text.len() as u32).to_le_bytes());
            info.extend_from_slice(&text);
            if text.len() % 2 != 0 {
                info.push(0);
            }
        }
        chunks.extend_from_slice(b"LIST");
        chunks.extend_from_slice(&(info.len() as u32).to_le_bytes());
        chunks.extend_from_slice(&info);
        if info.len() % 2 != 0 {
            chunks.push(0);
        }
    }

    chunks.extend_from_slice(b"data");
    chunks.extend_from_slice(&(data.len() as u32).to_le_bytes());
    chunks.extend_from_slice(data);
    if data.len() % 2 != 0 {
        chunks.push(0);
    }

    let mut out = Vec::with_capacity(12 + chunks.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((4 + chunks.len()) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(&chunks);
    out
}
