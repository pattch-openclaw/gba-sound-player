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
//! file stem and `?` — the reference's own order (`prep.py`). Both are folded to
//! ASCII because the product font is a 96-glyph subset; a fold that leaves a
//! non-ASCII title in the container would be a display bug at 03:00, not here.
//!
//! **The fold is not NFKD.** `album_prep.py` decomposes with Unicode's tables and
//! drops the combining marks, so `Ö` becomes `O`; this reproduces that for the
//! Latin-1 and Latin-Extended-A precomposed letters, which is where real tags
//! live, and drops anything else outside ASCII. A character outside those blocks
//! folds to nothing rather than to a wrong letter — the same thing the reference
//! does with a script it cannot decompose.

use std::path::Path;

use crate::wav;
use crate::{Error, Pcm, Samples, TrackInput};
use uli_lite::format::MAX_TRACKS;

/// The reference's tag truncation (`uli_enc.c` writes `title[120]`, `artist[150]`).
const TITLE_MAX: usize = 120;
const ARTIST_MAX: usize = 150;
/// What an empty artist becomes, in the reference's own table.
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

/// The folded `(title, artist)` pair, tags first, the file's name last.
fn tags(path: &Path, tags: &wav::Tags) -> (String, String) {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = fold(tags.title.as_deref().unwrap_or(&stem));
    let artist = fold(tags.artist.as_deref().unwrap_or(""));
    let title = truncate(&title, TITLE_MAX);
    let artist = truncate(
        if artist.is_empty() { UNKNOWN } else { &artist },
        ARTIST_MAX,
    );
    (title, artist)
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// §5.5's ASCII fold: map the precomposed Latin letters to their base, drop what is
/// not ASCII, collapse whitespace, and unwrap a title that is entirely
/// parenthesised. An empty result stays empty and is the caller's to replace.
pub fn fold(text: &str) -> String {
    let mut ascii = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii() {
            ascii.push(ch);
        } else if let Some(base) = latin_base(ch) {
            ascii.push_str(base);
        }
        // Everything else is dropped, as the reference drops a mark it cannot map.
    }
    let collapsed = ascii.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() >= 2 && collapsed.starts_with('(') && collapsed.ends_with(')') {
        let inner = collapsed[1..collapsed.len() - 1].trim();
        if !inner.is_empty() {
            return inner.to_string();
        }
    }
    collapsed
}

/// The base ASCII letters for the precomposed Latin-1 / Latin-Extended-A block.
///
/// Grouped by base letter rather than listed one code point per line, because the
/// list's meaning is the grouping: every diacritic on `A` is an `A`. Ligatures and
/// the two letters that expand (`Æ`, `ß`, `Œ`, `Þ`) expand, matching NFKD.
fn latin_base(ch: char) -> Option<&'static str> {
    let s = match ch {
        'À'..='Å' | 'Ā' | 'Ă' | 'Ą' => "A",
        'à'..='å' | 'ā' | 'ă' | 'ą' => "a",
        'Æ' => "AE",
        'æ' => "ae",
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' => "C",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'Ď' | 'Đ' | 'Ð' => "D",
        'ď' | 'đ' | 'ð' => "d",
        'È'..='Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => "E",
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' => "G",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'Ĥ' | 'Ħ' => "H",
        'ĥ' | 'ħ' => "h",
        'Ì'..='Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' => "I",
        'ì'..='ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
        'Ĳ' => "IJ",
        'ĳ' => "ij",
        'Ĵ' => "J",
        'ĵ' => "j",
        'Ķ' => "K",
        'ķ' => "k",
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' => "L",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' | 'Ŋ' => "N",
        'ñ' | 'ń' | 'ņ' | 'ň' | 'ŋ' => "n",
        'Ò'..='Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' => "O",
        'ò'..='ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => "o",
        'Œ' => "OE",
        'œ' => "oe",
        'Ŕ' | 'Ŗ' | 'Ř' => "R",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'Ś' | 'Ŝ' | 'Ş' | 'Š' | 'ß' => "S",
        'ś' | 'ŝ' | 'ş' | 'š' => "s",
        'Ţ' | 'Ť' | 'Ŧ' | 'Þ' => "T",
        'ţ' | 'ť' | 'ŧ' | 'þ' => "t",
        'Ù'..='Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => "U",
        'ù'..='ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'Ŵ' => "W",
        'ŵ' => "w",
        'Ý' | 'Ŷ' | 'Ÿ' => "Y",
        'ý' | 'ÿ' | 'ŷ' => "y",
        'Ź' | 'Ż' | 'Ž' => "Z",
        'ź' | 'ż' | 'ž' => "z",
        'ſ' => "s",
        _ => return None,
    };
    Some(s)
}
