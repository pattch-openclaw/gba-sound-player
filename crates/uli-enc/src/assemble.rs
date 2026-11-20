//! Album assembly: WAV sources in, one gapless timeline and its track table out.
//!
//! This is [`crate::pcm`]'s boundary reached from files instead of from a pair: the
//! tracks are decoded, brought to 32,768 Hz stereo at PCM16 scale, and joined
//! **gaplessly** — track `i+1` starts exactly where track `i` ended, so
//! `NS = Σ len` and the track table is a partition of the album, not a wish. A
//! track's `len` is its own resampled length, which is why the table and the
//! samples are produced by the same code and cannot disagree.
//!
//! Titles and artists come from the file's `LIST/INFO` tags, falling back to the
//! file stem and `?` — the reference's own order (`prep.py`). They are carried
//! **verbatim**, including a non-ASCII one.
//!
//! **The encoder does not fold text, and that is deliberate.** §5.5's ASCII fold
//! lives in one place — `scripts/album_prep.py::fold`, used by the bake — and a
//! second implementation here would be a second owner of a rule that has to mean
//! one thing. A Unicode table in a codec encoder is also a table that can only be
//! half right without NFKD: the version this file used to carry mapped Latin-1 and
//! Latin-Extended-A and silently dropped every other script, turning a Japanese
//! title into an empty string. Whether a title is folded, shortened or refused is
//! the album layer's decision, taken before the encoder ever sees a `tracks.tsv`.

use std::path::Path;

use crate::wav;
use crate::{Error, Pcm, Samples, TrackInput};
use uli_lite::format::MAX_TRACKS;

/// What an absent artist becomes, in the reference's own table.
const UNKNOWN: &str = "?";

/// Assemble `paths` (in play order) into the encoder's PCM boundary.
///
/// Frames are joined gaplessly; the resulting count is `Σ` each source's resampled
/// length. Refusals name the rule and the file, and happen before any encoding:
/// more than [`MAX_TRACKS`] sources, an empty source, a zero-length album, and the
/// format's `u32` sample count.
pub fn assemble(paths: &[std::path::PathBuf]) -> Result<(Pcm, Samples), Error> {
    if paths.is_empty() {
        return Err(Error::Wav(
            "no --wav sources: an album needs at least one track".to_string(),
        ));
    }
    if paths.len() > MAX_TRACKS as usize {
        return Err(Error::Wav(format!(
            "{} sources: the format holds at most {MAX_TRACKS} tracks, so the album \
             would have to drop some — which is an album decision, not an encoder one",
            paths.len()
        )));
    }

    let mut tracks = Vec::with_capacity(paths.len());
    let mut data: Vec<f32> = Vec::new();
    let mut start = 0u64;
    for path in paths {
        let w = wav::read(path)?;
        let decoded = to_stereo(&w.samples, w.channels)?;
        let stereo = if w.rate == crate::resample::RATE {
            // The natively-32,768 Hz path, unchanged and unfiltered: this is what
            // makes a 32,768 Hz stereo WAV reach the identical blob its PCM pair
            // would. No meter, no normalisation, no dither — a copy.
            decoded
        } else {
            crate::resample::resample_stereo(&decoded, w.rate)?
        };
        let n = stereo.len() / 2;
        if n == 0 {
            return Err(Error::Wav(format!(
                "{}: 0 samples at {} Hz: an empty track would occupy no time and make \
                 the album's track table claim a boundary nothing can land on",
                path.display(),
                w.rate
            )));
        }
        let (title, artist) = tags(path, &w.tags);
        tracks.push(TrackInput {
            start_sample: u32::try_from(start).map_err(|_| {
                Error::Wav(format!(
                    "the album reaches sample {start} before {} and no longer fits the \
                     format's u32 sample count",
                    path.display()
                ))
            })?,
            n_samples: u32::try_from(n).map_err(|_| {
                Error::Wav(format!(
                    "{}: {n} samples does not fit the format's u32 track length",
                    path.display()
                ))
            })?,
            title,
            artist,
        });
        data.extend_from_slice(&stereo);
        start += n as u64;
    }

    let n_samples = u32::try_from(start).map_err(|_| {
        Error::Wav(format!(
            "the assembled album is {start} samples, past the format's u32 sample count"
        ))
    })?;
    Ok((
        Pcm::assembled(n_samples, tracks),
        Samples::from_interleaved(data),
    ))
}

/// 1 → 2 channels by duplication, 2 → unchanged. Anything else was refused at parse.
fn to_stereo(samples: &[f32], channels: u16) -> Result<Vec<f32>, Error> {
    match channels {
        2 => Ok(samples.to_vec()),
        1 => Ok(samples.iter().flat_map(|&s| [s, s]).collect()),
        n => Err(Error::Wav(format!(
            "{n} channels reached assembly; the parser should have refused that"
        ))),
    }
}

/// The `(title, artist)` pair: the file's tags, then its name, then `?`.
///
/// Verbatim on purpose — see the module doc. The encoder carrying a title
/// unchanged is what keeps it from owning a second copy of §5.5's fold; `?` is the
/// reference's own placeholder for an absent artist.
fn tags(path: &Path, tags: &wav::Tags) -> (String, String) {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = tags.title.clone().unwrap_or(stem);
    let artist = match tags.artist.as_deref() {
        Some(a) if !a.is_empty() => a.to_string(),
        _ => UNKNOWN.to_string(),
    };
    (title, artist)
}
