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
            "# n start len title artist (32768 Hz samples)\n\
             1\t0\t{n_samples}\tSilence\tuli-enc test\n"
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
