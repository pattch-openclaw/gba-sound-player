//! Full-frame differential: the GBA-first product decoder is bit-exact with the
//! portable reference, including operation counts, bit positions, overlap state
//! and both seek paths.
//!
//! This is the host witness for PR B. The on-target witness is
//! `examples/uli_bench`, which pins both arms on the committed corpus.

use uli_lite::format::M;
use uli_lite::{Album, CodeTables, Decoder, DecoderState, PortableKernels};
use uli_lite_gba::{GbaDecoder, HotState};

const BLOB: &[u8] = include_bytes!("../../uli-lite/tests/fixtures/uli2_tiny.uli");

fn outputs(
    d: &mut Decoder<'_, '_, PortableKernels>,
    l8: &mut [i8; M],
    r8: &mut [i8; M],
    l16: &mut [i16; M],
    r16: &mut [i16; M],
) -> uli_lite::FrameInfo {
    d.decode_frame(l8, r8, Some(l16), Some(r16)).unwrap()
}

fn product_outputs(
    d: &mut GbaDecoder<'_, '_>,
    l8: &mut [i8; M],
    r8: &mut [i8; M],
    l16: &mut [i16; M],
    r16: &mut [i16; M],
) -> uli_lite::FrameInfo {
    d.decode_frame(l8, r8, Some(l16), Some(r16)).unwrap()
}

#[test]
fn product_decoder_matches_the_reference_frame_for_frame() {
    let album = Album::parse(BLOB).expect("fixture parses");
    let mut ref_state = DecoderState::new();
    let mut reference = Decoder::<PortableKernels>::new(album, &mut ref_state).expect("reference");
    let mut tables = CodeTables::new();
    let mut hot = HotState::new();
    let mut product = GbaDecoder::new(album, &mut tables, &mut hot).expect("product");

    let (mut a8l, mut a8r) = ([0i8; M], [0i8; M]);
    let (mut a16l, mut a16r) = ([0i16; M], [0i16; M]);
    let (mut b8l, mut b8r) = ([0i8; M], [0i8; M]);
    let (mut b16l, mut b16r) = ([0i16; M], [0i16; M]);

    for frame in 0..album.n_frames() {
        let a = outputs(&mut reference, &mut a8l, &mut a8r, &mut a16l, &mut a16r);
        let b = product_outputs(&mut product, &mut b8l, &mut b8r, &mut b16l, &mut b16r);
        assert_eq!(a, b, "frame {frame} metadata diverged");
        assert_eq!(a8l, b8l, "frame {frame} int8 L diverged");
        assert_eq!(a8r, b8r, "frame {frame} int8 R diverged");
        assert_eq!(a16l, b16l, "frame {frame} int16 L diverged");
        assert_eq!(a16r, b16r, "frame {frame} int16 R diverged");
    }

    assert_eq!(reference.state().xover(), product.xover());
    assert_eq!(reference.state().hfnz(), product.hfnz());
    assert_eq!(reference.state().bitpos(), product.bitpos());
    assert_eq!(reference.state().frame_index(), product.frame_index());
}

#[test]
fn product_decoder_seeks_like_the_reference() {
    let album = Album::parse(BLOB).expect("fixture parses");

    for track in 0..album.n_tracks() {
        let mut ref_state = DecoderState::new();
        let mut reference =
            Decoder::<PortableKernels>::new(album, &mut ref_state).expect("reference");
        let mut tables = CodeTables::new();
        let mut hot = HotState::new();
        let mut product = GbaDecoder::new(album, &mut tables, &mut hot).expect("product");

        let a_skip = reference.seek_track(track).expect("reference seek");
        let b_skip = product.seek_track(track).expect("product seek");
        assert_eq!(a_skip, b_skip, "track {track} skip diverged");

        let (mut a8l, mut a8r) = ([0i8; M], [0i8; M]);
        let (mut a16l, mut a16r) = ([0i16; M], [0i16; M]);
        let (mut b8l, mut b8r) = ([0i8; M], [0i8; M]);
        let (mut b16l, mut b16r) = ([0i16; M], [0i16; M]);
        for frame in 0..4 {
            let a = outputs(&mut reference, &mut a8l, &mut a8r, &mut a16l, &mut a16r);
            let b = product_outputs(&mut product, &mut b8l, &mut b8r, &mut b16l, &mut b16r);
            assert_eq!(a, b, "track {track} frame {frame} metadata diverged");
            assert_eq!(a8l, b8l, "track {track} frame {frame} int8 L diverged");
            assert_eq!(a8r, b8r, "track {track} frame {frame} int8 R diverged");
            assert_eq!(a16l, b16l, "track {track} frame {frame} int16 L diverged");
            assert_eq!(a16r, b16r, "track {track} frame {frame} int16 R diverged");
        }
    }

    for entry in 0..album.seek_count() {
        let mut ref_state = DecoderState::new();
        let mut reference =
            Decoder::<PortableKernels>::new(album, &mut ref_state).expect("reference");
        let mut tables = CodeTables::new();
        let mut hot = HotState::new();
        let mut product = GbaDecoder::new(album, &mut tables, &mut hot).expect("product");
        reference.seek_entry(entry).expect("reference seek entry");
        product.seek_entry(entry).expect("product seek entry");
        assert_eq!(reference.state().bitpos(), product.bitpos());
        assert_eq!(reference.state().frame_index(), product.frame_index());
    }
}
