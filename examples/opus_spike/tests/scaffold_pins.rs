//! Scaffold pins (PR 1, step 1): the facts the vendored decoder + probe seam
//! must satisfy on the **host**, mirroring how flac_spike's `spike-test`
//! gates its crate. Deliberately narrow: the decode witness against the
//! ffmpeg/libopus reference arrives with step 2's embedded arms — there are
//! no committed Opus bytes yet, so nothing here pretends to prove decode
//! correctness. What *is* proven: the constructor contract OPUS.md claims,
//! the derived budget arithmetic, and the profile constants the ROM logs.

use opus_spike::probe::{self, SUPPORTED_RATES_HZ, decode_packet, new_decoder};

/// Constructor contract (OPUS.md, source-verified claim): exactly the five
/// RFC 6716 rates × mono/stereo construct; nothing else does.
#[test]
fn constructor_contract_is_the_five_rates_times_two_channel_counts() {
    for rate in SUPPORTED_RATES_HZ {
        for channels in [1usize, 2] {
            assert!(
                new_decoder(rate, channels).is_ok(),
                "decoder {rate} Hz x {channels} ch must construct"
            );
        }
    }
    // Rejections: rates outside the set (incl. 44.1 kHz, absent from the
    // crate by design), and channel counts outside 1..=2.
    for bad_rate in [0, 7999, 44100, 65536, 96000, -16000] {
        assert!(
            new_decoder(bad_rate, 1).is_err(),
            "decoder at {bad_rate} Hz must be rejected"
        );
    }
    for bad_channels in [0usize, 3] {
        assert!(
            new_decoder(48000, bad_channels).is_err(),
            "decoder with {bad_channels} channels must be rejected"
        );
    }
}

/// The budget is derived, never recalled (OPUS.md measurement plan item 6):
/// 20 ms x 16.78 MHz = 335,600 cycles/packet; at the pinned 20 ms profile
/// that is 960 samples at 48 kHz ⇒ 349.58 c/sample (≈350).
#[test]
fn budget_and_frame_math_are_the_derived_arithmetic() {
    assert_eq!(probe::BUDGET_CYCLES_PER_PACKET, 335_600);
    assert_eq!(probe::frame_samples(48_000), 960);
    assert_eq!(probe::frame_samples(16_000), 320);
    assert_eq!(probe::frame_samples(8_000), 160);
    // The ROM's compile-time per-sample figure, same arithmetic.
    assert_eq!(
        probe::BUDGET_CYCLES_PER_PACKET / probe::frame_samples(48_000) as u32,
        349
    );
}

/// The decode seam must at minimum reject an empty packet with opus-rs's own
/// contract error (a packet of 0 bytes is the PLC/DTX signal — the seam is
/// not yet deciding that policy; this pins only that the seam forwards
/// through to the decoder's documented behavior and does not panic).
#[test]
fn decode_seam_forwards_the_empty_packet_rejection() {
    let mut decoder = new_decoder(48_000, 1).expect("48k mono constructs");
    let mut output = [0.0f32; 960];
    assert_eq!(
        decode_packet(&mut decoder, 48_000, &mut [], &mut output),
        Err("Input packet empty")
    );
}

/// The inline decoder-state size, pinned to the value OPUS.md gotcha 1
/// measured **on the host** (`heap` off ⇒ 178,064 bytes inline). The same
/// vendored source lays out 177,864 bytes when built for thumbv4t — the ROM
/// logged its own number on-target (2026-09-29 serial: `inline 177864
/// bytes`), 200 bytes smaller purely from target layout. `size_of` is
/// per-target, so this test pins the host number by name; the ROM's serial
/// log is the witness for the thumbv4t number. Both agree on what the EWRAM
/// plan argues against: ≈178 KB of inline state against 256 KB total.
#[test]
fn inline_decoder_state_matches_the_opus_md_measured_host_size() {
    assert_eq!(probe::DECODER_STATE_BYTES, 178_064);
}
