//! The stub backend: deterministic synthetic PCM, **no codec, no `agb`, no registers** (§4.2b).
//!
//! This arm exists so the whole seam above `src/backend/` can be driven with seconds-long test runs
//! and no decoder: the host suite (`tests/album_seam_host.rs`) and AP-9's `stub` leg both play
//! through it. It is platform-free by construction — the only dependency is this crate's own
//! [`geometry`](crate::geometry) — which is what makes "the UI cannot know which codec it drives"
//! (§4.2b) a fact about the code rather than an aspiration.
//!
//! What it produces is a triangle wave whose amplitude steps per track, so consecutive tracks are
//! distinguishable by eye in a ledger and by value in a test, and nothing about it depends on
//! wall-clock time or on a random source. Positions are album samples, like every other arm.

use crate::geometry;
use crate::pcm::{AlbumMeta, FrameCostModel, PcmSource, SourceError, TrackMeta};

/// One track's boundaries and labels, supplied by the caller (there is no container to read).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StubTrack {
    /// Album position of the track's first sample.
    pub start_sample: u32,
    /// Length of the track, in samples.
    pub n_samples: u32,
    /// Title a screen or log line may show.
    pub title: &'static str,
    /// Artist (`""` when unknown).
    pub artist: &'static str,
}

/// A synthetic [`PcmSource`].
pub struct StubSource {
    tracks: &'static [StubTrack],
    track: usize,
    /// Samples already produced of the current track.
    pos: u32,
    rate_hz: u32,
    frame_samples: u32,
}

impl StubSource {
    /// A stub over `tracks`, positioned at the first track.
    ///
    /// The declared rate and frame size come from the geometry this build compiles against —
    /// [`geometry::RATE_HZ`] and [`geometry::FRAME_SAMPLES`] — so the stub's answers are the ones
    /// the surrounding app's constants describe, not a second set of numbers.
    pub fn new(tracks: &'static [StubTrack]) -> Self {
        Self {
            tracks,
            track: 0,
            pos: 0,
            rate_hz: geometry::RATE_HZ,
            frame_samples: geometry::FRAME_SAMPLES,
        }
    }
}

impl PcmSource for StubSource {
    fn n_tracks(&self) -> usize {
        self.tracks.len()
    }

    fn track(&self, i: usize) -> Option<TrackMeta> {
        self.tracks.get(i).map(|track| TrackMeta {
            start_sample: track.start_sample,
            n_samples: track.n_samples,
            title: track.title,
            artist: track.artist,
        })
    }

    fn album(&self) -> AlbumMeta {
        AlbumMeta {
            title: "stub album",
            artist: "",
            year: 0,
            n_samples: self
                .tracks
                .last()
                .map_or(0, |track| track.start_sample + track.n_samples),
            art: None,
        }
    }

    fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    fn start_track(&mut self, i: usize) -> Result<(), SourceError> {
        if i >= self.tracks.len() {
            return Err(SourceError::NoSuchTrack {
                index: i,
                n_tracks: self.tracks.len(),
            });
        }
        self.track = i;
        self.pos = 0;
        Ok(())
    }

    /// Q5: seek is deferred past v1, and the stub does not emulate one.
    fn start_sample(&mut self, _s: u32) -> Result<(), SourceError> {
        Err(SourceError::Unsupported(crate::pcm::Capability::Seek))
    }

    fn pull_frame(&mut self, out: &mut [i16]) -> Result<usize, SourceError> {
        let Some(track) = self.tracks.get(self.track) else {
            return Ok(0);
        };
        let remaining = track.n_samples.saturating_sub(self.pos);
        if remaining == 0 {
            return Ok(0);
        }
        let n = remaining.min(self.frame_samples) as usize;
        let have = out.len() / 2;
        if have < n {
            return Err(SourceError::BufferTooSmall { have, need: n });
        }
        for (index, pair) in out[..2 * n].chunks_exact_mut(2).enumerate() {
            let sample = sample_at(self.pos + index as u32, self.track);
            pair[0] = sample;
            pair[1] = -sample;
        }
        self.pos += n as u32;
        Ok(n)
    }

    fn producer_frame(&self) -> FrameCostModel {
        FrameCostModel::NOT_A_DECODER
    }

    fn reset_decoder_state(&mut self) {
        self.pos = 0;
    }
}

/// A deterministic triangle in `[-15128, 15128]`, offset by track so adjacent tracks differ.
///
/// `index & 0xFF` gives a 256-sample period (a 64 Hz tone at the product clock — an audible
/// "something is playing" tone on hardware, if anyone ever builds this arm) and the per-track
/// offset keeps the arms distinguishable without a second algorithm.
fn sample_at(index: u32, track: usize) -> i16 {
    let phase = (index & 0xFF) as i16;
    let triangle = if phase < 128 { phase } else { 255 - phase };
    triangle * 64 + (track as i16 & 7) * 1_000
}
