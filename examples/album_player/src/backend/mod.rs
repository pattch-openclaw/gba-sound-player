//! The backend seam's selection point: **the only module that knows an implementor exists** (§4.2b).
//!
//! One backend is chosen at build time (`make album-rom BACKEND=flac|stub`, chosen by cargo feature),
//! never at run time. Everything above this module names [`Source`], [`PcmSource`] and `SourceError`
//! — never `flac-lite`, never a codec crate. That is §4.2 rule 2 made structural: the app cannot
//! write `if flac { … }` above this directory because it has no codec name to write.
//!
//! ```
//! src/backend/mod.rs    this file — the selection, the generated-pin cross-check, one constructor
//! src/backend/flac.rs   feature "backend-flac" — links flac-lite (+ flac-lite-gba under arm-kernel)
//! src/backend/stub.rs   feature "backend-stub" — synthetic PCM, no codec at all
//! ```
//!
//! `stub.rs` is platform-free and codec-free so the host suite and AP-9's stub arm can drive the
//! whole seam without a decoder; `flac.rs` is the shipped arm. Both are first-class: neither is the
//! app's owner, and the ROM's `main` must never need to change between them.

use crate::assets;
use crate::meta::{Album, Track};
use crate::pcm::SourceError;

#[cfg(feature = "backend-flac")]
pub mod flac;
#[cfg(feature = "backend-stub")]
pub mod stub;

/// The selected backend's implementor — the only type the application layer constructs.
///
/// `backend-flac` wins when both features are on (the host suite's build), because the shipped
/// image is the FLAC arm; a stub-only image (`--no-default-features --features backend-stub`)
/// selects the synthetic one and links no decoder at all.
#[cfg(feature = "backend-flac")]
pub type Source = flac::FlacSource;

/// The selected backend's implementor when the FLAC arm is not compiled in.
#[cfg(all(feature = "backend-stub", not(feature = "backend-flac")))]
pub type Source = stub::StubSource;

/// Open the album this image carries: parse `ALBM1`, cross-check every generated pin against the
/// manifest, and start the first track.
///
/// **The pin cross-check is §4.3 rule 1 and spec D-C rule 2**: the manifest is authoritative, the
/// bake cross-checked it against each packed container, and the ROM re-checks the pair rather than
/// picking whichever file it read last. Any disagreement is a named [`SourceError::PinMismatch`].
///
/// This constructor exists for the FLAC arm only, because a stub image's track list is the caller's
/// (there is no container to read) — a stub ROM builds [`Source`] with `StubSource::new`.
#[cfg(feature = "backend-flac")]
pub fn open() -> Result<Source, SourceError> {
    let album = Album::parse(assets::ALBUM1).map_err(SourceError::Manifest)?;
    verify_pins(&album)?;
    Source::new(album)
}

/// Every field the bake recorded twice must agree: the manifest's `Track` and the generated
/// `assets::TRACKS` pin. One failure per field, each naming the track.
///
/// Called by both arms' boot paths (and by the host gate for every track), so a stale generated
/// file cannot be believed by a build that never re-ran the bake. Public because a stub image reads
/// the same manifest and owes the same refusal — the check is arm-independent, and a stub ROM that
/// skipped it could play a timeline the packed container does not have.
pub fn verify_pins(album: &Album<'static>) -> Result<(), SourceError> {
    let pins = &assets::TRACKS;
    if pins.len() != album.n_tracks() as usize {
        return Err(SourceError::PinMismatch {
            track: album.n_tracks() as usize,
            field: "n_tracks",
        });
    }
    for (index, pin) in pins.iter().enumerate() {
        let track: Track<'static> = album.track(index as u32);
        if track.frames != pin.frames {
            return Err(pin_mismatch(index, "frames"));
        }
        if track.n_samples != pin.n_samples {
            return Err(pin_mismatch(index, "n_samples"));
        }
        if track.start_sample != pin.start_sample {
            return Err(pin_mismatch(index, "start_sample"));
        }
        if track.recip != pin.recip {
            return Err(pin_mismatch(index, "recip"));
        }
        if track.max_fixed_order != pin.max_fixed_order {
            return Err(pin_mismatch(index, "max_fixed_order"));
        }
        // The manifest's `(offset, len)` slices the asset region; the pin records that length. A
        // blob that shrank under a stale pin is caught here, as a named refusal, rather than as a
        // decode failure at play time.
        if track.asset.len() != pin.asset_len as usize {
            return Err(pin_mismatch(index, "asset_len"));
        }
    }
    Ok(())
}

/// The one-field disagreement, so the caller's log names the field and not just the track.
fn pin_mismatch(track: usize, field: &'static str) -> SourceError {
    SourceError::PinMismatch { track, field }
}
