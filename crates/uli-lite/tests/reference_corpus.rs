use uli_lite::{Album, Decoder, DecoderState, Error, Ops};

const BLOB: &[u8] = include_bytes!("fixtures/uli2_tiny.uli");
const EXPECTED: &str = include_str!("fixtures/uli2_tiny_expected.txt");

const M: usize = 512;

#[derive(Debug)]
struct Row {
    frame: u32,
    h8: u32,
    h16: u32,
    bits: u32,
    bitpos: u32,
    ops: Ops,
    cycles: u32,
    xover: u32,
    hfnz: u32,
}

fn fnv(mut h: u32, bytes: &[u8]) -> u32 {
    for &b in bytes {
        h ^= b as u32;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

fn fnv_i16(mut h: u32, samples: &[i16]) -> u32 {
    for &s in samples {
        h = fnv(h, &s.to_le_bytes());
    }
    h
}

fn expected() -> (Vec<Row>, u32, u32) {
    let mut rows = Vec::new();
    let mut all8 = None;
    let mut all16 = None;
    for line in EXPECTED.lines() {
        if line.starts_with("# all8 ") {
            let mut it = line.split_whitespace();
            assert_eq!(it.next(), Some("#"));
            assert_eq!(it.next(), Some("all8"));
            all8 = Some(u32::from_str_radix(it.next().unwrap(), 16).unwrap());
            assert_eq!(it.next(), Some("all16"));
            all16 = Some(u32::from_str_radix(it.next().unwrap(), 16).unwrap());
            continue;
        }
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(f.len(), 18, "bad expected row: {line}");
        let u = |i: usize| f[i].parse::<u32>().unwrap();
        let h = |i: usize| u32::from_str_radix(f[i], 16).unwrap();
        rows.push(Row {
            frame: u(0),
            h8: h(1),
            h16: h(2),
            bits: u(3),
            bitpos: u(4),
            ops: Ops {
                bandsyms: u(5),
                coded_bands: u(6),
                pairs: u(7),
                quads: u(8),
                nz: u(9),
                zc: u(10),
                raws: u(11),
                escs: u(12),
                ms: u(13),
                long_codes: u(14),
            },
            cycles: u(15),
            xover: u(16),
            hfnz: u(17),
        });
    }
    (rows, all8.unwrap(), all16.unwrap())
}

fn new_state() -> Box<DecoderState> {
    Box::new(DecoderState::new())
}

fn decode_tiny() -> (Vec<(u32, u32)>, u32, u32) {
    let album = Album::parse(BLOB).unwrap();
    assert_eq!(album.n_frames(), 129);
    assert_eq!(album.n_tracks(), 1);
    assert_eq!(album.title(0), Some("Tiny Fixture"));
    assert_eq!(album.artist(0), Some("uli-lite"));

    let mut state = new_state();
    let mut decoder = Decoder::new(album, &mut state).unwrap();
    let (rows, want8, want16) = expected();
    assert_eq!(rows.len(), album.n_frames() as usize);

    let mut l8 = [0i8; M];
    let mut r8 = [0i8; M];
    let mut l16 = [0i16; M];
    let mut r16 = [0i16; M];
    let mut hashes = Vec::new();
    let mut all8 = 2_166_136_261u32;
    let mut all16 = 2_166_136_261u32;

    for (i, row) in rows.iter().enumerate() {
        let info = decoder
            .decode_frame(&mut l8, &mut r8, Some(&mut l16), Some(&mut r16))
            .unwrap();
        assert_eq!(info.frame, row.frame, "frame index");
        assert_eq!(info.bits, row.bits, "bits at frame {i}");
        assert_eq!(info.bitpos, row.bitpos, "bitpos at frame {i}");
        assert_eq!(info.ops, row.ops, "ops at frame {i}");
        assert_eq!(info.cycles, row.cycles, "cycles at frame {i}");
        assert_eq!(info.xover, row.xover, "xover at frame {i}");
        assert_eq!(info.hfnz, row.hfnz, "hfnz at frame {i}");

        let h8 = fnv(
            fnv(2_166_136_261, &l8.map(|v| v as u8)),
            &r8.map(|v| v as u8),
        );
        let h16 = fnv_i16(fnv_i16(2_166_136_261, &l16), &r16);
        assert_eq!(h8, row.h8, "int8 hash at frame {i}");
        assert_eq!(h16, row.h16, "int16 hash at frame {i}");
        hashes.push((h8, h16));
        all8 = fnv(all8, &h8.to_le_bytes());
        all16 = fnv(all16, &h16.to_le_bytes());
    }

    assert_eq!(all8, want8);
    assert_eq!(all16, want16);
    assert_eq!(decoder.state().xover(), 0);
    assert_eq!(decoder.state().hfnz(), 0);
    assert_eq!(
        decoder.decode_frame(&mut l8, &mut r8, None, None),
        Err(Error::EndOfStream)
    );
    (hashes, all8, all16)
}

#[test]
fn tiny_album_is_bit_exact_against_the_c_reference() {
    decode_tiny();
}

#[test]
fn seek_track_matches_continuous_decode() {
    let (hashes, _, _) = decode_tiny();
    let album = Album::parse(BLOB).unwrap();
    let mut state = new_state();
    let mut decoder = Decoder::new(album, &mut state).unwrap();
    assert_eq!(decoder.seek_track(0).unwrap(), 0);
    let mut l8 = [0i8; M];
    let mut r8 = [0i8; M];
    let mut l16 = [0i16; M];
    let mut r16 = [0i16; M];
    for k in 1..5 {
        decoder
            .decode_frame(&mut l8, &mut r8, Some(&mut l16), Some(&mut r16))
            .unwrap();
        let h8 = fnv(
            fnv(2_166_136_261, &l8.map(|v| v as u8)),
            &r8.map(|v| v as u8),
        );
        assert_eq!(h8, hashes[k].0, "seek_track frame {k}");
    }
}

#[test]
fn seek_entry_matches_continuous_decode_after_one_frame() {
    let (hashes, _, _) = decode_tiny();
    let album = Album::parse(BLOB).unwrap();
    let mut state = new_state();
    let mut decoder = Decoder::new(album, &mut state).unwrap();
    decoder.seek_entry(0).unwrap();
    let mut l8 = [0i8; M];
    let mut r8 = [0i8; M];
    // The reference seek test discards the first frame after a seek entry.
    decoder.decode_frame(&mut l8, &mut r8, None, None).unwrap();
    for k in 1..5 {
        decoder.decode_frame(&mut l8, &mut r8, None, None).unwrap();
        let h8 = fnv(
            fnv(2_166_136_261, &l8.map(|v| v as u8)),
            &r8.map(|v| v as u8),
        );
        assert_eq!(h8, hashes[k].0, "seek_entry frame {k}");
    }
}

#[test]
fn malformed_albums_are_rejected_without_panicking() {
    for cut in [0usize, 1, 4, 87, 88, 100, BLOB.len() - 1] {
        assert!(Album::parse(&BLOB[..cut]).is_err(), "cut at {cut}");
    }
    let mut bad = BLOB.to_vec();
    bad[0] ^= 0xff;
    assert!(Album::parse(&bad).is_err());
    bad = BLOB.to_vec();
    let n = bad.len();
    bad[n - 1] ^= 0xff;
    // The final byte is padding/stream, not necessarily structural; the
    // parser may accept it. What matters is that it does not panic.
    let _ = Album::parse(&bad);
}

#[test]
fn short_output_is_rejected_before_mutating_the_frame_position() {
    let album = Album::parse(BLOB).unwrap();
    let mut state = new_state();
    let mut decoder = Decoder::new(album, &mut state).unwrap();
    let mut l = [0i8; 511];
    let mut r = [0i8; M];
    assert_eq!(
        decoder.decode_frame(&mut l, &mut r, None, None),
        Err(Error::OutputTooSmall)
    );
    assert_eq!(decoder.state().frame_index(), 0);
    let mut good_l = [0i8; M];
    assert!(
        decoder
            .decode_frame(&mut good_l, &mut r, None, None)
            .is_ok()
    );
}
