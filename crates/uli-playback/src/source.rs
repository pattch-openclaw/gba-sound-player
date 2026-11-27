//! The codec-agnostic source seam, and its ULI2 adapter.
//!
//! The player owns a [`PcmSource`] and a ring. It never sees a bitstream, a
//! table, or a container offset: everything ULI2-specific is behind this trait,
//! which is what lets the FLAC path remain a compile-time alternative rather than
//! dead code (PLAN's PR D, "Make the playback layer codec-agnostic").
//!
//! [`UliSource`] is the ULI2 adapter. It carries the `ULA2` container for one
//! reason — the track table is the player's track list, seek table and titles —
//! and it owns the two habits a track change needs: the preroll frame is decoded
//! by the seek (the decoder does that), and the next frame's first `skip` samples
//! are dropped (the *source* does that, because only it knows where a track
//! starts).

use uli_lite::format::M;
use uli_lite::{Album, Error};

/// One decoded frame's planar output, as the seam hands it over.
pub struct Frame<'a> {
    /// 512 left samples, signed 8-bit, already the FIFO's depth.
    pub l: &'a mut [i8],
    /// 512 right samples.
    pub r: &'a mut [i8],
    /// The optional pre-DAC 16-bit output pair (the headphone-spec path). `None`
    /// is the product's fused int8 path, which is what ships.
    pub l16: Option<(&'a mut [i16], &'a mut [i16])>,
}

/// What one pulled frame reports about where it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    /// Track index this frame belongs to.
    pub track: usize,
    /// Decoder frame index (the container's numbering, prerolls included).
    pub frame: u32,
    /// Samples of the current track delivered *including* this frame.
    pub samples_into_track: u32,
    /// This frame reached (or passed) the track's declared length; the next pull
    /// advances to the next track, or ends the album.
    pub end_of_track: bool,
    /// Samples zeroed at the front of this frame because the track entry's
    /// `skip` asks for them to be dropped. Zero except on the first frame after
    /// a track change that is not 512-aligned.
    pub skipped: u16,
}

/// Why a pull failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceError {
    /// The decoder rejected the stream.
    Decode(Error),
    /// The album finished.
    End,
    /// The requested track does not exist.
    BadTrack,
    /// The caller's output buffers are shorter than one frame.
    ShortBuffer,
}

/// The seam's view of a decoder: one frame in, one frame out, plus seeking.
///
/// Implemented for both decoders the project carries — `uli_lite::Decoder<K>`
/// (the bit-exact reference) and `uli_lite_gba::GbaDecoder` (the product path) —
/// so the player ROM and the host suite run the *same* seam code.
pub trait FrameDecoder {
    /// Decode the next frame into `out`, returning **its** frame index (the
    /// container's numbering). The index is the decoder's return value, not its
    /// internal counter read afterwards: that counter already points at the next
    /// frame, and a seam that reported it would name every frame one too high.
    fn decode(&mut self, out: &mut Frame<'_>) -> Result<u32, Error>;
    /// Seek to a track's preroll, returning the samples to drop from the next
    /// frame.
    fn seek_track(&mut self, track: usize) -> Result<usize, Error>;
}

impl<'a, 's, K: uli_lite::Kernels> FrameDecoder for uli_lite::Decoder<'a, 's, K> {
    fn decode(&mut self, out: &mut Frame<'_>) -> Result<u32, Error> {
        match out.l16.as_mut() {
            Some((l16, r16)) => self
                .decode_frame(out.l, out.r, Some(l16), Some(r16))
                .map(|info| info.frame),
            None => self
                .decode_frame(out.l, out.r, None, None)
                .map(|info| info.frame),
        }
    }

    fn seek_track(&mut self, track: usize) -> Result<usize, Error> {
        uli_lite::Decoder::seek_track(self, track)
    }
}

impl FrameDecoder for uli_lite_gba::GbaDecoder<'_, '_> {
    fn decode(&mut self, out: &mut Frame<'_>) -> Result<u32, Error> {
        match out.l16.as_mut() {
            Some((l16, r16)) => self
                .decode_frame(out.l, out.r, Some(l16), Some(r16))
                .map(|info| info.frame),
            None => self
                .decode_frame(out.l, out.r, None, None)
                .map(|info| info.frame),
        }
    }

    fn seek_track(&mut self, track: usize) -> Result<usize, Error> {
        uli_lite_gba::GbaDecoder::seek_track(self, track)
    }
}

/// What the player and the UI need from a codec.
///
/// Deliberately narrow: frames, seeking, and enough position for a display. No
/// timing, no allocation, no float — the platform owns the clock, and the seam
/// reports samples.
pub trait PcmSource {
    /// Samples per channel per frame (ULI2: 512).
    fn frame_len(&self) -> usize;
    /// Decode the next frame. `Err(SourceError::End)` once the album is over.
    fn pull(&mut self, out: &mut Frame<'_>) -> Result<Stamp, SourceError>;
    /// Jump to a track (preroll and skip handled by the source).
    fn seek_track(&mut self, track: usize) -> Result<(), SourceError>;
    /// Tracks in the album.
    fn track_count(&self) -> usize;
    /// The track currently playing.
    fn track_index(&self) -> usize;
    /// Its title, from the container's strings.
    fn track_title(&self) -> &str;
    /// Its length in samples.
    fn track_samples(&self) -> u32;
    /// Samples delivered so far in this track.
    fn samples_into_track(&self) -> u32;
    /// True once the last track has been delivered.
    fn finished(&self) -> bool;
}

/// The ULI2 adapter: a decoder plus the `ULA2` track table.
pub struct UliSource<'a, D: FrameDecoder> {
    album: Album<'a>,
    dec: D,
    track: usize,
    samples_into_track: u32,
    skip_left: u16,
    advance_pending: bool,
    finished: bool,
}

impl<'a, D: FrameDecoder> UliSource<'a, D> {
    /// Wrap `dec` (already constructed over `album`) and seek to track 0, so the
    /// first [`PcmSource::pull`] returns audio rather than the album's preroll.
    pub fn new(album: Album<'a>, mut dec: D) -> Result<Self, SourceError> {
        let skip = dec.seek_track(0).map_err(SourceError::Decode)?;
        Ok(Self {
            album,
            dec,
            track: 0,
            samples_into_track: 0,
            skip_left: skip as u16,
            advance_pending: false,
            finished: false,
        })
    }

    /// The album this source plays (the player reads nothing else from it).
    pub fn album(&self) -> Album<'a> {
        self.album
    }

    /// Advance to the next track, or finish. Returns `false` when finished.
    fn advance(&mut self) -> bool {
        let next = self.track + 1;
        if next >= self.album.n_tracks() {
            self.finished = true;
            return false;
        }
        match self.dec.seek_track(next) {
            Ok(skip) => {
                self.track = next;
                self.samples_into_track = 0;
                self.skip_left = skip as u16;
                self.advance_pending = false;
                true
            }
            Err(_) => {
                self.finished = true;
                false
            }
        }
    }
}

impl<D: FrameDecoder> PcmSource for UliSource<'_, D> {
    fn frame_len(&self) -> usize {
        M
    }

    fn pull(&mut self, out: &mut Frame<'_>) -> Result<Stamp, SourceError> {
        if out.l.len() < M || out.r.len() < M {
            return Err(SourceError::ShortBuffer);
        }
        if let Some((l16, r16)) = out.l16.as_ref() {
            if l16.len() < M || r16.len() < M {
                return Err(SourceError::ShortBuffer);
            }
        }
        if self.advance_pending && !self.advance() {
            return Err(SourceError::End);
        }
        if self.finished {
            return Err(SourceError::End);
        }

        let frame = self.dec.decode(out).map_err(SourceError::Decode)?;

        // A track that does not start on a frame boundary: the seek reports the
        // samples to drop, and dropping them means not emitting them — the frame
        // is zero-filled at the front and the count is reported instead.
        let mut skipped = 0u16;
        if self.skip_left > 0 {
            let n = usize::from(self.skip_left).min(M);
            out.l[..n].fill(0);
            out.r[..n].fill(0);
            skipped = n as u16;
            self.skip_left -= skipped;
        }

        self.samples_into_track = self
            .samples_into_track
            .saturating_add(M as u32 - u32::from(skipped));
        let end_of_track = self.samples_into_track >= self.track_samples();
        self.advance_pending = end_of_track;

        Ok(Stamp {
            track: self.track,
            frame,
            samples_into_track: self.samples_into_track,
            end_of_track,
            skipped,
        })
    }

    fn seek_track(&mut self, track: usize) -> Result<(), SourceError> {
        match self.dec.seek_track(track) {
            Ok(skip) => {
                self.track = track;
                self.samples_into_track = 0;
                self.skip_left = skip as u16;
                self.advance_pending = false;
                self.finished = false;
                Ok(())
            }
            Err(_) => Err(SourceError::BadTrack),
        }
    }

    fn track_count(&self) -> usize {
        self.album.n_tracks()
    }

    fn track_index(&self) -> usize {
        self.track
    }

    fn track_title(&self) -> &str {
        self.album.title(self.track).unwrap_or("")
    }

    fn track_samples(&self) -> u32 {
        self.album.track(self.track).map_or(0, |t| t.n_samples)
    }

    fn samples_into_track(&self) -> u32 {
        self.samples_into_track
    }

    fn finished(&self) -> bool {
        self.finished
    }
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use uli_lite::format::M;
    use uli_lite::{Album, Decoder, DecoderState};

    use super::{Frame, PcmSource, SourceError, UliSource};

    const CORPUS_BLOB: uli_lite::Aligned<
        { include_bytes!("../../uli-lite/tests/fixtures/uli2_tiny.uli").len() },
    > = uli_lite::Aligned(*include_bytes!(
        "../../uli-lite/tests/fixtures/uli2_tiny.uli"
    ));
    const CORPUS: &[u8] = &CORPUS_BLOB.0;
    fn fnv(mut h: u32, bytes: &[u8]) -> u32 {
        for &b in bytes {
            h ^= u32::from(b);
            h = h.wrapping_mul(16_777_619);
        }
        h
    }

    fn source<'a>(album: Album<'a>, state: &'a mut DecoderState) -> UliSource<'a, Decoder<'a, 'a>> {
        let dec = Decoder::new(album, state).expect("tables");
        UliSource::new(album, dec).expect("seek to track 0")
    }

    /// The corpus's C-pinned per-frame hashes: `uli2_tiny_expected.txt` was
    /// generated by the bit-exact reference decoder, so frame *k*'s hash is an oracle
    /// value for the same audio from any implementation. Frame 0 is the preroll
    /// and is not audio, which matters here: the seam seeks to the track and
    /// therefore never emits it.
    fn pinned_frames() -> Vec<(u32, u32, u32)> {
        let text = include_str!("../../uli-lite/tests/fixtures/uli2_tiny_expected.txt");
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut it = line.split_whitespace();
            let frame: u32 = it.next().expect("frame").parse().expect("frame number");
            let i8h = u32::from_str_radix(it.next().expect("int8"), 16).expect("int8 hash");
            let i16h = u32::from_str_radix(it.next().expect("int16"), 16).expect("int16 hash");
            out.push((frame, i8h, i16h));
        }
        out
    }

    /// The seam must be **transparent**: every frame it emits has to match the C
    /// reference decoder's own hash for that frame index, one for one — which is
    /// a stronger statement than an album total, because it pins *which* frames
    /// the seam hands over as well as their content.
    #[test]
    fn the_seam_reproduces_the_c_pinned_frames_one_for_one() {
        let album = Album::parse(CORPUS).expect("corpus parses");
        let mut state = DecoderState::new();
        let mut src = source(album, &mut state);
        let pinned = pinned_frames();
        assert_eq!(pinned.len(), album.n_frames() as usize);

        let mut l = [0i8; M];
        let mut r = [0i8; M];
        let mut pulled = 0usize;
        loop {
            let (mut l16, mut r16) = ([0i16; M], [0i16; M]);
            let stamp = match src.pull(&mut Frame {
                l: &mut l,
                r: &mut r,
                l16: Some((&mut l16, &mut r16)),
            }) {
                Ok(s) => s,
                Err(SourceError::End) => break,
                Err(e) => panic!("pull failed after {pulled} frames: {e:?}"),
            };
            let frame = stamp.frame;
            assert_ne!(frame, 0, "a preroll frame must never be emitted as audio");
            let (_, want8, want16) = pinned[frame as usize];
            let mut h8 = 2_166_136_261u32;
            h8 = fnv(h8, &l.iter().map(|&v| v as u8).collect::<Vec<_>>());
            h8 = fnv(h8, &r.iter().map(|&v| v as u8).collect::<Vec<_>>());
            assert_eq!(h8, want8, "frame {frame}: seam int8 differs from the pin");

            let mut h16 = 2_166_136_261u32;
            for &v in l16.iter() {
                h16 = fnv(h16, &v.to_le_bytes());
            }
            for &v in r16.iter() {
                h16 = fnv(h16, &v.to_le_bytes());
            }
            assert_eq!(
                h16, want16,
                "frame {frame}: seam int16 differs from the pin"
            );
            if frame < 5 {}

            let is_last = frame as usize + 1 == album.n_frames() as usize;
            assert_eq!(stamp.end_of_track, is_last, "frame {frame}: end_of_track");
            pulled += 1;
        }
        assert_eq!(
            pulled,
            album.n_frames() as usize - 1,
            "the preroll is not audio"
        );
        assert!(src.finished());
        assert_eq!(
            src.pull(&mut Frame {
                l: &mut l,
                r: &mut r,
                l16: None
            })
            .unwrap_err(),
            SourceError::End
        );
    }

    /// Seeking rewinds the source's own counters, not only the decoder's.
    #[test]
    fn seeking_resets_the_position_and_replays_identically() {
        let album = Album::parse(CORPUS).expect("corpus parses");
        let mut state = DecoderState::new();
        let mut src = source(album, &mut state);

        let mut l = [0i8; M];
        let mut r = [0i8; M];
        let mut first = [0i8; M];
        src.pull(&mut Frame {
            l: &mut l,
            r: &mut r,
            l16: None,
        })
        .expect("first frame");
        first.copy_from_slice(&l);
        for _ in 0..3 {
            src.pull(&mut Frame {
                l: &mut l,
                r: &mut r,
                l16: None,
            })
            .expect("decode");
        }
        assert_eq!(src.samples_into_track(), 4 * M as u32);

        src.seek_track(0).expect("re-seek");
        assert_eq!(src.samples_into_track(), 0);
        assert!(!src.finished());
        src.pull(&mut Frame {
            l: &mut l,
            r: &mut r,
            l16: None,
        })
        .expect("replay");
        assert_eq!(&l[..], &first[..], "a re-seek replays the same samples");
    }

    /// The optional pre-DAC 16-bit output is carried by the same pull.
    #[test]
    fn the_int16_path_rides_the_same_frame() {
        let album = Album::parse(CORPUS).expect("corpus parses");
        let mut state = DecoderState::new();
        let mut src = source(album, &mut state);

        let (mut l8, mut r8) = ([0i8; M], [0i8; M]);
        let (mut l16, mut r16) = ([0i16; M], [0i16; M]);
        src.pull(&mut Frame {
            l: &mut l8,
            r: &mut r8,
            l16: Some((&mut l16, &mut r16)),
        })
        .expect("first frame");
        // The fused int8 is the 16-bit sample's *rounded* top byte — ULI2's
        // `(acc + 2^39) >> 40`, not a truncation — so it may differ from a plain
        // shift by one LSB and never by more. (The plan forbids silently
        // replacing this with the FLAC path's truncation; this test is what
        // would notice.)
        for i in 0..M {
            let tl = i16::from(l8[i]) - (l16[i] >> 8);
            let tr = i16::from(r8[i]) - (r16[i] >> 8);
            assert!(tl.abs() <= 1, "left sample {i}: {tl} LSB from truncation");
            assert!(tr.abs() <= 1, "right sample {i}: {tr} LSB from truncation");
        }
    }

    /// A short output buffer is refused before the decoder consumes a frame.
    #[test]
    fn a_short_buffer_is_refused_and_consumes_nothing() {
        let album = Album::parse(CORPUS).expect("corpus parses");
        let mut state = DecoderState::new();
        let mut src = source(album, &mut state);
        let mut l = [0i8; M - 1];
        let mut r = [0i8; M];
        assert_eq!(
            src.pull(&mut Frame {
                l: &mut l,
                r: &mut r,
                l16: None
            })
            .unwrap_err(),
            SourceError::ShortBuffer
        );
        assert_eq!(src.samples_into_track(), 0);
    }
}
