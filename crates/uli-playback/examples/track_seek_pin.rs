//! Witness: **track seek** reproduces the continuous stream, including a track
//! that does not start on a frame boundary.
//!
//! The bench pins the *continuous* decode against the C reference decoder's album
//! hash, so a seek that reproduces the continuous stream is pinned to the C
//! reference transitively. What a seek adds is the two things this checks:
//!
//! * **the preroll**: `seek_track` decodes the track's preroll frame and discards
//!   it, so the first frame the seam hands over is the track's first *audio* frame
//!   — the one the continuous decode reaches at `preroll_frame + 1`;
//! * **the skip**: a track whose `start_sample` is not a multiple of 512 sets
//!   `skip = start_sample - 512 * preroll_frame`, and that many samples of the
//!   first frame after the seek are not the track's. This witness asserts the
//!   policy the seam implements — the skipped prefix is zeroed and reported, and
//!   everything from `skip` on is bit-identical to the continuous stream.
//!
//! Usage:
//!   cargo run --example track_seek_pin -- <album.uli>
//!
//! A witness, not a gate: it needs a generated multi-track blob (gitignored), so
//! it runs from `make uli-playback-track-pin`, never from `make test`.

use std::env;
use std::fs;
use std::process;

use uli_lite::format::M;
use uli_lite::{Album, Decoder, DecoderState};
use uli_playback::source::{Frame, PcmSource, UliSource};

/// Frames compared after each seek (the C decoder publishes four; this is one
/// more so the skip's effect is visible on two frames).
const FRAMES: usize = 5;

type Buf = [i8; M];

fn continuous_frames(album: Album<'_>, first: u32, out: &mut [Buf]) {
    let mut state = DecoderState::new();
    let mut dec = Decoder::new(album, &mut state).expect("tables");
    let mut l = [0i8; M];
    let mut r = [0i8; M];
    let mut got = 0usize;
    for f in 0.. {
        if f > first + out.len() as u32 {
            break;
        }
        dec.decode_frame(&mut l, &mut r, None, None)
            .expect("decode");
        if f >= first && got < out.len() {
            out[got] = l;
            got += 1;
        }
    }
    assert_eq!(
        got,
        out.len(),
        "continuous decode reached the requested frames"
    );
}

fn compare(tag: &str, got: &[i8], want: &[i8], from: usize) -> usize {
    let mut differ = 0;
    for i in from..M {
        if got[i] != want[i] {
            differ += 1;
        }
    }
    if differ != 0 {
        eprintln!("{tag}: {differ} samples differ from the continuous stream (from {from})");
    }
    differ
}

fn main() {
    let path = match env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: track_seek_pin <album.uli>");
            process::exit(2);
        }
    };
    let blob = fs::read(&path).unwrap_or_else(|e| {
        eprintln!("track_seek_pin: {path}: {e}");
        process::exit(1);
    });
    let album = Album::parse(&blob).unwrap_or_else(|e| {
        eprintln!("track_seek_pin: parse: {e:?}");
        process::exit(1);
    });
    if album.n_tracks() < 2 {
        eprintln!("track_seek_pin: needs a multi-track album to witness a track change");
        process::exit(1);
    }

    let mut failures = 0usize;
    for t in 0..album.n_tracks() {
        let tr = album.track(t).expect("track");
        let skip = usize::from(tr.skip);
        let first = tr.preroll_frame + 1;

        let mut want = [[0i8; M]; FRAMES];
        continuous_frames(album, first, &mut want);

        // The same window through the seam, after a seek to this track.
        let mut state = DecoderState::new();
        let dec = Decoder::new(album, &mut state).expect("tables");
        let mut src = UliSource::new(album, dec).expect("seek to track 0");
        src.seek_track(t).expect("seek");

        let mut l = [0i8; M];
        let mut r = [0i8; M];
        let mut first_skip_reported = 0u16;
        for k in 0..FRAMES {
            let stamp = src
                .pull(&mut Frame {
                    l: &mut l,
                    r: &mut r,
                    l16: None,
                })
                .unwrap_or_else(|e| {
                    eprintln!("track {t} frame {k}: {e:?}");
                    process::exit(1);
                });
            if k == 0 {
                first_skip_reported = stamp.skipped;
            }
            // First post-seek frame: only the tail past `skip` must match; the
            // prefix is the dropped region and is zero-filled by policy.
            let from = if k == 0 { skip } else { 0 };
            failures += compare(&format!("track {t} frame {k}"), &l, &want[k], from);
            if k == 0 {
                for i in 0..skip {
                    if l[i] != 0 {
                        eprintln!("track {t}: skipped sample {i} is not zero");
                        failures += 1;
                        break;
                    }
                }
            }
        }
        println!(
            "track {}: preroll_frame={} skip={} reported={} {} frames reproduce the continuous stream from sample {}",
            t + 1,
            tr.preroll_frame,
            skip,
            first_skip_reported,
            FRAMES,
            skip
        );
    }

    if failures == 0 {
        println!(
            "track_seek_pin: OK — every track's seek reproduces the continuous stream ({} tracks, skip respected)",
            album.n_tracks()
        );
    } else {
        println!("track_seek_pin: FAIL — {failures} differences");
        process::exit(1);
    }
}
