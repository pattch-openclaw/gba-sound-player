//! # uli-playback — ULI2's playback seam, hardware-free
//!
//! PR D's design in three pieces, none of which knows about timers, DMA or the
//! cartridge:
//!
//! * [`source`] — the **codec-agnostic** `PcmSource` the player owns, and
//!   `UliSource`, its ULI2 adapter over the product decoder and the `ULA2`
//!   container. This is the only module that knows what a bitstream is.
//! * [`fold`] — the **output stage**. The decoder always synthesises full stereo;
//!   a speaker-profile build folds it *after* the decoder, here, and never by
//!   shortening synthesis to one channel.
//! * [`player`] — the ring and its ledger, over `gba_playback::playback::RingFeed`
//!   (linked, not re-invented), with the underrun / tail / starved vocabulary the
//!   gate reads.
//!
//! ## What is deliberately absent
//!
//! No critical section: `thumbv4t-none-eabi` has no atomics and the ARM7TDMI has
//! no atomic instructions, so the *caller* owns the interrupt discipline exactly
//! as `gba-playback`'s docs state. No allocation, no float, no time source: the
//! platform decides when to call [`player::Player::feed`] and when to
//! [`player::Player::take`], and the seam reports samples, not seconds.
//!
//! ## Provenance of the two decoders the seam can carry
//!
//! [`source::UliSource`] is generic over [`source::FrameDecoder`], implemented for
//! both `uli_lite::Decoder<K>` (the bit-exact reference, used by the host suite
//! and available to a non-GBA consumer) and `uli_lite_gba::GbaDecoder` (the
//! product path the ROM links). They are interchangeable by construction — the
//! host twin proves the second decodes the corpus frame-for-frame identically —
//! and the seam's own transparency test pins the first against a direct decode.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The host suite is the only consumer of `std`; the crate itself is `no_std`.
#[cfg(test)]
extern crate std;

pub mod fold;
pub mod player;
pub mod source;
