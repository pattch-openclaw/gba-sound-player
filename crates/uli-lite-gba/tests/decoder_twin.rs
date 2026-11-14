//! Full-frame differential: the product kernel set decodes the committed
//! corpus bit-for-bit identically to the portable reference.
//!
//! PR A wires `uli_lite_gba::ArmKernels` into `uli_lite::Decoder` through the
//! `Kernels` seam. `tests/twin.rs` proves the four kernels in isolation; this
//! proves the *decoder*: the same frame state, bitstream position, operation
//! counts and 512-sample int8/int16 outputs for every frame of the corpus.
//!
//! This is a host test. The target witness is `examples/uli_bench`, which
//! decodes the same corpus through both kernel sets and checks the album pins.

use uli_lite::{Album, Decoder, DecoderState, PortableKernels};

const BLOB: &[u8] = include_bytes!("../../uli-lite/tests/fixtures/uli2_tiny.uli");
const M: usize = uli_lite::format::M;

#[test]
fn arm_kernels_decode_the_corpus_like_the_reference() {
    let album = Album::parse(BLOB).expect("fixture parses");
    let mut state_ref = DecoderState::new();
    let mut state_arm = DecoderState::new();
    let mut reference = Decoder::<PortableKernels>::new(album, &mut state_ref).expect("reference");
    let mut product =
        Decoder::<uli_lite_gba::ArmKernels>::with_kernels(album, &mut state_arm).expect("product");

    let mut ref8l = [0i8; M];
    let mut ref8r = [0i8; M];
    let mut ref16l = [0i16; M];
    let mut ref16r = [0i16; M];
    let mut arm8l = [0i8; M];
    let mut arm8r = [0i8; M];
    let mut arm16l = [0i16; M];
    let mut arm16r = [0i16; M];

    for frame in 0..album.n_frames() {
        let a = reference
            .decode_frame(&mut ref8l, &mut ref8r, Some(&mut ref16l), Some(&mut ref16r))
            .expect("reference frame");
        let b = product
            .decode_frame(&mut arm8l, &mut arm8r, Some(&mut arm16l), Some(&mut arm16r))
            .expect("product frame");

        assert_eq!(a, b, "frame {frame} metadata diverged");
        assert_eq!(ref8l, arm8l, "frame {frame} int8 L diverged");
        assert_eq!(ref8r, arm8r, "frame {frame} int8 R diverged");
        assert_eq!(ref16l, arm16l, "frame {frame} int16 L diverged");
        assert_eq!(ref16r, arm16r, "frame {frame} int16 R diverged");
    }

    assert_eq!(reference.state().xover(), product.state().xover());
    assert_eq!(reference.state().hfnz(), product.state().hfnz());
    assert_eq!(reference.state().bitpos(), product.state().bitpos());
    assert_eq!(
        reference.state().frame_index(),
        product.state().frame_index()
    );
}

#[test]
fn arm_kernels_seek_like_the_reference() {
    let album = Album::parse(BLOB).expect("fixture parses");
    for track in 0..album.n_tracks() {
        let mut state_ref = DecoderState::new();
        let mut state_arm = DecoderState::new();
        let mut reference =
            Decoder::<PortableKernels>::new(album, &mut state_ref).expect("reference");
        let mut product = Decoder::<uli_lite_gba::ArmKernels>::with_kernels(album, &mut state_arm)
            .expect("product");

        let skip_ref = reference.seek_track(track).expect("reference seek");
        let skip_arm = product.seek_track(track).expect("product seek");
        assert_eq!(skip_ref, skip_arm, "track {track} skip diverged");
        assert_eq!(reference.state().bitpos(), product.state().bitpos());
        assert_eq!(
            reference.state().frame_index(),
            product.state().frame_index()
        );
    }
}
