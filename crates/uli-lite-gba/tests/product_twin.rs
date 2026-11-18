//! Full-frame differential: the GBA-first product decoder is bit-exact with the
//! portable reference, including operation counts, bit positions, overlap state
//! and both seek paths.
//!
//! This is the host witness for PR B. The on-target witness is
//! `examples/uli_bench`, which pins both arms on the committed corpus.

use uli_lite::format::M;
use uli_lite::{Album, CodeTables, Decoder, DecoderState, PortableKernels};
use uli_lite_gba::{ClassTables, GbaDecoder, HotState};

static BLOB_STORAGE: uli_lite::Aligned<
    { include_bytes!("../../uli-lite/tests/fixtures/uli2_tiny.uli").len() },
> = uli_lite::Aligned(*include_bytes!(
    "../../uli-lite/tests/fixtures/uli2_tiny.uli"
));
const BLOB: &[u8] = &BLOB_STORAGE.0;

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
    let mut class = ClassTables::new();
    let mut hot = HotState::new();
    let mut product = GbaDecoder::new(album, &mut tables, &mut class, &mut hot).expect("product");

    let (mut a8l, mut a8r) = ([0i8; M], [0i8; M]);
    let (mut a16l, mut a16r) = ([0i16; M], [0i16; M]);
    let (mut b8l, mut b8r) = ([0i8; M], [0i8; M]);
    let (mut b16l, mut b16r) = ([0i16; M], [0i16; M]);

    for frame in 0..album.n_frames() {
        let a = outputs(&mut reference, &mut a8l, &mut a8r, &mut a16l, &mut a16r);
        let b = product_outputs(&mut product, &mut b8l, &mut b8r, &mut b16l, &mut b16r);
        // The full metadata comparison needs the counters, which the product
        // build compiles out (PR C-3b). In that build, check every field the
        // product must still agree on and prove the diagnostics stay zeroed.
        #[cfg(feature = "verify")]
        assert_eq!(a, b, "frame {frame} metadata diverged");
        #[cfg(not(feature = "verify"))]
        {
            assert_eq!(a.frame, b.frame, "frame {frame} index diverged");
            assert_eq!(a.bits, b.bits, "frame {frame} bit count diverged");
            assert_eq!(a.bitpos, b.bitpos, "frame {frame} bit position diverged");
            assert_eq!(
                b.ops,
                uli_lite::Ops::ZERO,
                "frame {frame}: a product build counted operations"
            );
            assert_eq!(
                b.cycles, 0,
                "frame {frame}: a product build ran the cycle model"
            );
        }
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
        let mut class = ClassTables::new();
        let mut hot = HotState::new();
        let mut product =
            GbaDecoder::new(album, &mut tables, &mut class, &mut hot).expect("product");

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
            #[cfg(feature = "verify")]
            assert_eq!(a, b, "track {track} frame {frame} metadata diverged");
            #[cfg(not(feature = "verify"))]
            {
                assert_eq!(a.frame, b.frame, "track {track} frame {frame} index");
                assert_eq!(a.bits, b.bits, "track {track} frame {frame} bits");
                assert_eq!(a.bitpos, b.bitpos, "track {track} frame {frame} bitpos");
                assert_eq!(
                    b.ops,
                    uli_lite::Ops::ZERO,
                    "track {track} frame {frame}: a product build counted operations"
                );
            }
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
        let mut class = ClassTables::new();
        let mut hot = HotState::new();
        let mut product =
            GbaDecoder::new(album, &mut tables, &mut class, &mut hot).expect("product");
        reference.seek_entry(entry).expect("reference seek entry");
        product.seek_entry(entry).expect("product seek entry");
        assert_eq!(reference.state().bitpos(), product.bitpos());
        assert_eq!(reference.state().frame_index(), product.frame_index());
    }
}

/// The product build must not pay for the counters it cannot see (PR C-3b).
/// If this fails, something re-enabled `Ops`/`Diag` outside the `verify` gate.
#[cfg(not(feature = "verify"))]
#[test]
fn product_build_compiles_the_diagnostics_out() {
    let album = Album::parse(BLOB).expect("fixture parses");
    let mut tables = CodeTables::new();
    let mut class = ClassTables::new();
    let mut hot = HotState::new();
    let mut product = GbaDecoder::new(album, &mut tables, &mut class, &mut hot).expect("product");
    let (mut l8, mut r8) = ([0i8; M], [0i8; M]);
    let (mut l16, mut r16) = ([0i16; M], [0i16; M]);
    let info = product
        .decode_frame(&mut l8, &mut r8, Some(&mut l16), Some(&mut r16))
        .expect("frame 0 decodes");
    assert_eq!(info.ops, uli_lite::Ops::ZERO, "ops are not compiled out");
    assert_eq!(info.cycles, 0, "the cycle model is not compiled out");
    assert_eq!(
        (info.xover, info.hfnz),
        (0, 0),
        "diagnostics are not compiled out"
    );
    assert_eq!(product.ops(), uli_lite::Ops::ZERO);
    assert_eq!((product.xover(), product.hfnz()), (0, 0));
}

/// The product reader loads the stream with aligned `u16` reads (one `ldrh` per
/// refill), which is only sound for a halfword-aligned blob. An odd-based blob
/// must be **rejected by name**, not decoded — this is the witness that the
/// alignment contract is enforced rather than assumed, and that a future
/// embedder which forgets `uli_lite::Aligned` gets a diagnosable error.
#[test]
fn product_decoder_rejects_an_unaligned_blob() {
    // `Vec` is aligned; the byte at index 0 shifts the album to an odd address
    // while keeping its contents byte-identical and self-contained.
    let mut shifted = Vec::with_capacity(BLOB.len() + 1);
    shifted.push(0u8);
    shifted.extend_from_slice(BLOB);
    let odd = &shifted[1..];
    assert_eq!(
        (odd.as_ptr() as usize) & 1,
        1,
        "the test needs an odd-based slice"
    );
    let album = uli_lite::Album::parse(odd).expect("the album itself is intact");
    let mut tables = uli_lite::CodeTables::new();
    let mut class = ClassTables::new();
    let mut hot = HotState::new();
    assert_eq!(
        GbaDecoder::new(album, &mut tables, &mut class, &mut hot).err(),
        Some(uli_lite::Error::UnalignedStream)
    );
}
