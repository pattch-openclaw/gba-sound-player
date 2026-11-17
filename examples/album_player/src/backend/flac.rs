//! The FLAC backend — the shipped arm, and **the only place in the app that names a codec crate**.
//!
//! It owns the whole `GAFP`-shaped half of the seam: parse the packed manifest, run the boot gate
//! the shipped product path runs, own the decoder and its scratch buffers, and hand interleaved
//! int16 to the caller. Nothing above `src/backend/` sees a `Manifest`, a `Decoder` or a frame
//! index; the trait it implements speaks album samples only (§4.2 rule 1).
//!
//! ```text
//! boot gate   Manifest::parse → DEFAULT_PROFILE.validate → channels() == 2 → rate identity
//!             (mirrors examples/flac_integration/src/product_play_main.rs's `fn manifest()`,
//!             ~:1077-1140, including the pin re-check at ~:1095-1101)
//! frame       Decoder::decode_next, or decode_next_with::<ArmRiceKernel> under `arm-kernel`
//! scratch     FrameBuffers::required_len(&manifest) → owned Vec<i32> per channel
//! ```

use alloc::vec::Vec;

use flac_lite::format::DEFAULT_PROFILE;
use flac_lite::{DecodedFrame, Decoder, FrameBuffers, Manifest};

use crate::assets;
use crate::geometry;
use crate::meta::Album;
use crate::pcm::{AlbumMeta, FrameCostModel, PcmSource, SourceError, TrackMeta};

/// A [`PcmSource`] over the album's packed `FLAC` blobs.
pub struct FlacSource {
    album: Album<'static>,
    /// The current track's packed container, kept so [`PcmSource::reset_decoder_state`] can rebuild
    /// the decoder without a fallible path.
    manifest: Option<Manifest<'static>>,
    /// `None` after a reset (rebuilt lazily) or at end of stream.
    decoder: Option<Decoder<'static>>,
    /// Decode scratch, one buffer per channel, sized once per track — never reallocated per frame.
    left: Vec<i32>,
    right: Vec<i32>,
    /// Samples per channel of the current track's largest frame, i.e. the caller's `out` requirement.
    frame_len: usize,
    /// End of the current track reached (`decode_next` said so).
    eos: bool,
}

impl FlacSource {
    /// Open an album, positioned on **no** track.
    ///
    /// Nothing is decoded and no container is parsed until the caller names a track with
    /// [`start_track`](PcmSource::start_track), which is where the boot gate runs — so opening an
    /// album no longer pays to gate track 0 (the PR-3a fixture's track 0 is the 12,293,131-byte full
    /// master, which the ROM never plays).
    ///
    /// The `Result` is kept so the seam's constructor stays fallible for the next backend arm; there
    /// is no failing path here now. `pull_frame` before the first `start_track` is `Ok(0)`: an album
    /// with no current track has no stream, and that is not an error.
    pub fn new(album: Album<'static>) -> Result<Self, SourceError> {
        Ok(Self {
            album,
            manifest: None,
            decoder: None,
            left: Vec::new(),
            right: Vec::new(),
            frame_len: 0,
            eos: true,
        })
    }
}

impl PcmSource for FlacSource {
    fn n_tracks(&self) -> usize {
        self.album.n_tracks() as usize
    }

    fn track(&self, i: usize) -> Option<TrackMeta> {
        if i >= self.n_tracks() {
            return None;
        }
        let track = self.album.track(i as u32);
        Some(TrackMeta {
            start_sample: track.start_sample,
            n_samples: track.n_samples,
            title: track.title,
            artist: track.artist,
        })
    }

    fn album(&self) -> AlbumMeta {
        AlbumMeta {
            title: self.album.album_title(),
            artist: self.album.album_artist(),
            year: self.album.year(),
            n_samples: self.album.album_samples(),
            art: None,
        }
    }

    fn rate_hz(&self) -> u32 {
        self.album.rate_hz()
    }

    fn start_track(&mut self, i: usize) -> Result<(), SourceError> {
        if i >= self.n_tracks() {
            return Err(SourceError::NoSuchTrack {
                index: i,
                n_tracks: self.n_tracks(),
            });
        }
        let track = self.album.track(i as u32);

        let manifest = Manifest::parse(track.asset)
            .map_err(|_| SourceError::Asset("the track's GAFP manifest was refused"))?;
        if DEFAULT_PROFILE.validate(&manifest).is_err() {
            return Err(SourceError::Asset(
                "the stream is outside the encode profile",
            ));
        }
        if manifest.channels() != 2 {
            // The DMA pairing, the ping-pong and the FIFO8 pins are all stereo shaped; a mono
            // asset is a real product case and 7-2's to add (product_play_main.rs:1109-1117).
            return Err(SourceError::Asset(
                "this playback path is stereo-only (channels != 2)",
            ));
        }
        if geometry::playback_rate_for_stream(manifest.sample_rate_hz()) != Some(geometry::RATE_HZ)
        {
            // Q14 / plan item 7-0: identity or refusal, never a reinterpretation. Written as a
            // comparison so a re-introduced mapping has to change this line.
            return Err(SourceError::Asset(
                "the stream's declared rate has no playback mapping",
            ));
        }

        // The container against the pin the bake wrote down (§4.3 rule 1, D-C rule 2). The bake has
        // already compared these; the ROM re-checks the pair rather than trusting whichever file it
        // read last, exactly as product_play_main.rs:1095-1101 does.
        let pin = &assets::TRACKS[i];
        if manifest.frame_count() != pin.frames as usize {
            return Err(SourceError::PinMismatch {
                track: i,
                field: "frames",
            });
        }
        if manifest.total_samples() != pin.samples {
            return Err(SourceError::PinMismatch {
                track: i,
                field: "samples",
            });
        }
        if manifest.max_fixed_order() != pin.max_fixed_order {
            return Err(SourceError::PinMismatch {
                track: i,
                field: "max_fixed_order",
            });
        }

        let required = FrameBuffers::required_len(&manifest);
        self.left.resize(required, 0);
        self.right.resize(required, 0);
        self.frame_len = required;
        self.decoder = Some(
            Decoder::new(&manifest)
                .map_err(|_| SourceError::Asset("the decoder refused the manifest"))?,
        );
        self.manifest = Some(manifest);
        self.eos = false;
        Ok(())
    }

    /// Q5: seek is deferred past v1. The FLAC decoder can only seek by frame index, which is
    /// precisely the position unit §4.2 rule 1 forbids crossing this seam, so there is nothing
    /// honest to implement here yet.
    fn start_sample(&mut self, _s: u32) -> Result<(), SourceError> {
        Err(SourceError::Unsupported(crate::pcm::Capability::Seek))
    }

    fn pull_frame(&mut self, out: &mut [i16]) -> Result<usize, SourceError> {
        if self.eos {
            return Ok(0);
        }
        if self.decoder.is_none() {
            // A reset dropped the decoder; rebuild it from the track's own container.
            let manifest = self
                .manifest
                .ok_or(SourceError::Asset("no track has been selected"))?;
            self.decoder = Some(
                Decoder::new(&manifest)
                    .map_err(|_| SourceError::Asset("the decoder refused the manifest"))?,
            );
        }

        let need = self.frame_len;
        let have = out.len() / 2;
        if have < need {
            return Err(SourceError::BufferTooSmall { have, need });
        }

        let mut written = 0usize;
        let mut ended = false;
        {
            let Some(decoder) = self.decoder.as_mut() else {
                return Ok(0);
            };
            let mut bufs = FrameBuffers::new(&mut self.left, &mut self.right);
            match decode_one(decoder, &mut bufs) {
                Ok(Some(frame)) => {
                    written = frame.left.len();
                    for (index, (&left, &right)) in
                        frame.left.iter().zip(frame.right.iter()).enumerate()
                    {
                        out[2 * index] = to_i16(left)?;
                        out[2 * index + 1] = to_i16(right)?;
                    }
                }
                Ok(None) => ended = true,
                Err(_) => return Err(SourceError::Asset("a frame failed to decode")),
            }
        }

        if ended {
            self.decoder = None;
            self.eos = true;
            return Ok(0);
        }
        Ok(written)
    }

    fn producer_frame(&self) -> FrameCostModel {
        FrameCostModel::FLAC
    }

    fn reset_decoder_state(&mut self) {
        // Dropping the decoder is the cold start; the container is kept so the next pull rebuilds
        // from frame 0 without a fallible reset hook.
        self.decoder = None;
        self.eos = false;
    }
}

/// The 16-bit profile's range, checked rather than truncated: a decoded sample outside it means
/// the bitstream desynchronised, and folding or playing it would launder a corrupt decode
/// (`gba_playback::checksum::fold_i16le_stereo` refuses the same way, for the same reason).
fn to_i16(sample: i32) -> Result<i16, SourceError> {
    i16::try_from(sample)
        .map_err(|_| SourceError::Asset("decoded sample outside the int16 profile"))
}

/// One frame through the kernel this build names.
///
/// On a host build — or with `arm-kernel` off — this is the core's `TableCountKernel`. The
/// `target_arch` half of the gate is required, not decorative: `flac-lite-gba` is an arm-only
/// dependency, so a host build of this library must not name it even when the feature is on. That
/// is what lets `tests/album_seam_host.rs` drive this exact module against the real fixture blobs.
#[cfg(all(feature = "arm-kernel", target_arch = "arm"))]
fn decode_one<'b>(
    decoder: &mut Decoder<'_>,
    bufs: &'b mut FrameBuffers<'_>,
) -> flac_lite::Result<Option<DecodedFrame<'b>>> {
    decoder.decode_next_with::<flac_lite_gba::rice_arm::ArmRiceKernel>(bufs)
}

/// [`decode_one`]'s default-kernel arm.
#[cfg(not(all(feature = "arm-kernel", target_arch = "arm")))]
fn decode_one<'b>(
    decoder: &mut Decoder<'_>,
    bufs: &'b mut FrameBuffers<'_>,
) -> flac_lite::Result<Option<DecodedFrame<'b>>> {
    decoder.decode_next(bufs)
}
