//! 6a-7's width leg — the vendored **i64** decode path (probe code; the
//! instrument's only consumer).
//!
//! Item 6a-6 moved the core's default arithmetic width to `i32`
//! (`i32-integrator` on) but left its *cycle* marginal unread: a same-image
//! pair needs both widths in one link, and the core selects a width at compile
//! time. This module is the missing second half — a verbatim i64 realization of
//! `frame::decode_frame` → `subframe::decode_subframe` → integrators →
//! `stereo::decorrelate`, sharing everything width-independent with the control
//! (`BitReader`, `FrameHeader::parse`, `SubframeType::parse`,
//! `PredictorState::fill`, `residual::decode_residual`). The variable is the
//! hot-path arithmetic width, only — the E4 sample's shape, inverted.
//!
//! 6a-7 times three legs in one calibration: `ctrl` (the core's i32 default),
//! `wide` (this module), and `asm` (the landed ARM kernel over the core's i32
//! default). The readings are then `ctrl − wide` (the width marginal) and
//! `ctrl − asm` (the stack, re-anchored on the post-6a-6 tree).
//!
//! ## Why this is the pre-6a-6 control, and not a new algorithm
//!
//! The i64 forms below are the core's code as it stood before 6a-6 (the
//! `--no-default-features` reference build), vendored so both widths can be
//! timed in one link. Every operation matches that build:
//!
//! * `integrate_fixed` — i64 cascade, `out as i32` at the store;
//! * `integrate_lpc` — i64 dot product, algebraic `>> shift`, `as i32`;
//! * `pad_block` — `i64 << wasted` then truncate;
//! * `decorrelate` — i64 add/sub and mid/side halving, truncate at store.
//!
//! Bit-identity is a gate, not a hope: `tests/item6a7_witness.rs` decodes the
//! committed clips to the reference PCM *and* compares this path to
//! `driver::decode_one` frame window by frame window. The ROM repeats the
//! per-frame equality check outside its timing windows and voids the numbers on
//! the first divergence (E1/E4/6a-5b pattern).

use crate::assets::SpikeClip;
use crate::driver::DriverError;
use flac_lite::bits::BitReader;
use flac_lite::format::ChannelConfig;
use flac_lite::frame::{FrameHeader, StreamDefaults};
use flac_lite::residual::decode_residual;
use flac_lite::subframe::{PredictorState, SubframeType};

/// Decode exactly one frame with the vendored i64 path — the twin of
/// `driver::decode_one` (same seek-table offset walk, same table cross-check,
/// same fresh cursor; only the width beneath differs).
pub fn decode_one_wide(
    clip: &SpikeClip,
    frame_index: usize,
    left: &mut [i32],
    right: &mut [i32],
    state: &mut [PredictorState; 2],
) -> Result<usize, DriverError> {
    let frame = &clip.frames[frame_index];
    let max = usize::from(clip.max_blocksize);
    if left.len() < max {
        return Err(DriverError::BufferTooSmall {
            need: max,
            have: left.len(),
        });
    }
    if clip.channels == 2 && right.len() < max {
        return Err(DriverError::BufferTooSmall {
            need: max,
            have: right.len(),
        });
    }
    let offset = match usize::try_from(frame.offset) {
        Ok(offset) => offset,
        Err(_) => {
            return Err(DriverError::FrameOffsetOutOfRange {
                frame: frame_index,
                offset: usize::MAX,
                len: clip.region.len(),
            });
        }
    };
    let region = clip
        .region
        .get(offset..)
        .ok_or(DriverError::FrameOffsetOutOfRange {
            frame: frame_index,
            offset,
            len: clip.region.len(),
        })?;

    let defaults = StreamDefaults {
        sample_rate_hz: clip.sample_rate_hz,
        bits_per_sample: clip.bits_per_sample,
    };
    let mut reader = BitReader::new(region);
    let header = FrameHeader::parse(&mut reader, &defaults).map_err(DriverError::Decode)?;
    if header.blocksize != usize::from(frame.blocksize) {
        return Err(DriverError::MetadataMismatch {
            frame: frame_index,
            table: usize::from(frame.blocksize),
            header: header.blocksize,
        });
    }
    decode_frame_i64(&mut reader, &header, left, right, state).map_err(DriverError::Decode)
}

/// Vendored i64 `frame::decode_frame`: the control flow is the core's; only the
/// arithmetic below is wide.
fn decode_frame_i64(
    reader: &mut BitReader<'_>,
    header: &FrameHeader,
    left: &mut [i32],
    right: &mut [i32],
    state: &mut [PredictorState; 2],
) -> Result<usize, flac_lite::Error> {
    let blocksize = header.blocksize;
    let slots = usize::from(header.channels.subframe_count());

    if left.len() < blocksize || (slots == 2 && right.len() < blocksize) {
        return Err(flac_lite::Error::InvalidField);
    }
    let side_slot = match header.channels {
        ChannelConfig::RightSide => Some(0),
        ChannelConfig::LeftSide | ChannelConfig::MidSide => Some(1),
        ChannelConfig::Independent { .. } => None,
    };
    let slot_bits = |slot: usize| -> u8 {
        (u32::from(header.bits_per_sample) + u32::from(side_slot == Some(slot))) as u8
    };

    decode_subframe_i64(
        reader,
        blocksize,
        slot_bits(0),
        &mut state[0],
        &mut left[..blocksize],
    )?;
    if slots == 2 {
        decode_subframe_i64(
            reader,
            blocksize,
            slot_bits(1),
            &mut state[1],
            &mut right[..blocksize],
        )?;
    }
    let right_span = if slots == 2 { blocksize } else { 0 };
    decorrelate_i64(
        header.channels,
        blocksize,
        &mut left[..blocksize],
        &mut right[..right_span],
    )?;

    reader.byte_align();
    let _crc16_hi = reader.read_u8()?;
    let _crc16_lo = reader.read_u8()?;
    Ok(blocksize)
}

/// Vendored i64 `subframe::decode_subframe`: shared field readers and residual
/// walk; i64 integrators and pad.
fn decode_subframe_i64(
    reader: &mut BitReader<'_>,
    blocksize: usize,
    sample_bits: u8,
    state: &mut PredictorState,
    out: &mut [i32],
) -> Result<SubframeType, flac_lite::Error> {
    if out.len() != blocksize {
        return Err(flac_lite::Error::InvalidField);
    }
    let kind = SubframeType::parse(reader)?;
    let order = kind.order();
    if order > blocksize {
        return Err(flac_lite::Error::InvalidField);
    }
    let wasted = reader.read_wasted_bits()?;
    let sub_bits = u32::from(sample_bits)
        .checked_sub(wasted)
        .filter(|&b| b > 0 && b <= 32)
        .ok_or(flac_lite::Error::InvalidField)?;
    let subframe_bits = sub_bits as u8;
    state.fill(reader, order, subframe_bits, 0)?;
    for (i, slot) in state.warm_up[..order].iter().rev().enumerate() {
        out[i] = *slot;
    }
    match kind {
        SubframeType::Constant => {
            let v = reader.read_signed(sub_bits)?;
            out.fill(v);
        }
        SubframeType::Verbatim => {
            for sample in out.iter_mut() {
                *sample = reader.read_signed(sub_bits)?;
            }
        }
        SubframeType::Fixed(order_u8) => {
            decode_residual(reader, blocksize, order, &mut out[order..])?;
            integrate_fixed_i64(order_u8, state, &mut out[order..])?;
        }
        SubframeType::Lpc { order: _order_u8 } => {
            let precision_minus_one = reader.read_bits(4)?;
            if precision_minus_one == 0b1111 {
                return Err(flac_lite::Error::InvalidField);
            }
            let precision = precision_minus_one + 1;
            let shift = reader.read_signed(5)?;
            if shift < 0 {
                return Err(flac_lite::Error::InvalidField);
            }
            let mut coeffs = [0i32; flac_lite::MAX_LPC_ORDER];
            for c in coeffs.iter_mut().take(order) {
                *c = reader.read_signed(precision)?;
            }
            decode_residual(reader, blocksize, order, &mut out[order..])?;
            integrate_lpc_i64(&coeffs[..order], shift as i8, state, &mut out[order..])?;
        }
    }
    pad_block_i64(out, wasted);
    Ok(kind)
}

/// §9.2.2's decoder multiply in i64 (the pre-6a-6 form).
fn pad_block_i64(out: &mut [i32], wasted: u32) {
    if wasted == 0 {
        return;
    }
    for s in out.iter_mut() {
        *s = ((i64::from(*s)) << wasted) as i32;
    }
}

/// FIXED integration with i64 accumulators (the pre-6a-6 form).
fn integrate_fixed_i64(
    order: u8,
    warm_up: &PredictorState,
    residual: &mut [i32],
) -> Result<(), flac_lite::Error> {
    let order = usize::from(order);
    if order > flac_lite::MAX_FIXED_ORDER {
        return Err(flac_lite::Error::UnsupportedPredictorOrder);
    }
    if warm_up.len < order {
        return Err(flac_lite::Error::InvalidField);
    }
    let w = &warm_up.warm_up;
    let mut d1;
    let mut d2 = 0i64;
    let mut d3 = 0i64;
    let mut out = 0i64;
    match order {
        0 => return Ok(()),
        1 => {
            d1 = i64::from(w[0]);
        }
        2 => {
            d1 = i64::from(w[0]) - i64::from(w[1]);
            out = i64::from(w[0]);
        }
        3 => {
            d2 = i64::from(w[0]) - 2 * i64::from(w[1]) + i64::from(w[2]);
            d1 = i64::from(w[0]) - i64::from(w[1]);
            out = i64::from(w[0]);
        }
        _ => {
            d3 = i64::from(w[0]) - 3 * i64::from(w[1]) + 3 * i64::from(w[2]) - i64::from(w[3]);
            d2 = i64::from(w[0]) - 2 * i64::from(w[1]) + i64::from(w[2]);
            d1 = i64::from(w[0]) - i64::from(w[1]);
            out = i64::from(w[0]);
        }
    }
    for sample in residual.iter_mut() {
        let r = i64::from(*sample);
        match order {
            1 => {
                d1 += r;
                out = d1;
            }
            2 => {
                d1 += r;
                out += d1;
            }
            3 => {
                d2 += r;
                d1 += d2;
                out += d1;
            }
            _ => {
                d3 += r;
                d2 += d3;
                d1 += d2;
                out += d1;
            }
        }
        *sample = out as i32;
    }
    Ok(())
}

/// LPC dot product in i64 with the algebraic `>> shift` (the pre-6a-6 form).
fn integrate_lpc_i64(
    coefficients: &[i32],
    shift: i8,
    warm_up: &PredictorState,
    residual: &mut [i32],
) -> Result<(), flac_lite::Error> {
    if shift < 0 {
        return Err(flac_lite::Error::InvalidField);
    }
    let order = coefficients.len();
    if order > flac_lite::MAX_LPC_ORDER {
        return Err(flac_lite::Error::UnsupportedPredictorOrder);
    }
    if warm_up.len < order {
        return Err(flac_lite::Error::InvalidField);
    }
    let shift = u32::from(shift as u8);
    if shift >= 64 {
        return Err(flac_lite::Error::InvalidField);
    }
    let w = &warm_up.warm_up;
    for i in 0..residual.len() {
        let mut acc = 0i64;
        for (j, &c) in coefficients.iter().enumerate() {
            let past = if j < i {
                i64::from(residual[i - 1 - j])
            } else {
                i64::from(w[j - i])
            };
            acc += i64::from(c) * past;
        }
        residual[i] = ((acc >> shift) + i64::from(residual[i])) as i32;
    }
    Ok(())
}

/// Stereo decorrelation with i64 intermediates (the pre-6a-6 form).
fn decorrelate_i64(
    config: ChannelConfig,
    blocksize: usize,
    left: &mut [i32],
    right: &mut [i32],
) -> Result<(), flac_lite::Error> {
    match config {
        ChannelConfig::Independent { channels: 1 } => {
            if left.len() < blocksize {
                return Err(flac_lite::Error::InvalidField);
            }
            Ok(())
        }
        ChannelConfig::Independent { .. } => {
            if left.len() < blocksize || right.len() < blocksize {
                return Err(flac_lite::Error::InvalidField);
            }
            Ok(())
        }
        ChannelConfig::LeftSide => {
            if left.len() < blocksize || right.len() < blocksize {
                return Err(flac_lite::Error::InvalidField);
            }
            for (l, s) in left[..blocksize].iter_mut().zip(&mut right[..blocksize]) {
                let (l0, s0) = (*l, *s);
                *l = l0;
                *s = (i64::from(l0) - i64::from(s0)) as i32;
            }
            Ok(())
        }
        ChannelConfig::RightSide => {
            if left.len() < blocksize || right.len() < blocksize {
                return Err(flac_lite::Error::InvalidField);
            }
            for (s, r) in left[..blocksize].iter_mut().zip(&mut right[..blocksize]) {
                let (s0, r0) = (*s, *r);
                *s = (i64::from(s0) + i64::from(r0)) as i32;
                *r = r0;
            }
            Ok(())
        }
        ChannelConfig::MidSide => {
            if left.len() < blocksize || right.len() < blocksize {
                return Err(flac_lite::Error::InvalidField);
            }
            for (m, s) in left[..blocksize].iter_mut().zip(&mut right[..blocksize]) {
                let (m0, s0) = (*m, *s);
                let m_ext = (i64::from(m0) << 1) | (i64::from(s0) & 1);
                *m = ((m_ext + i64::from(s0)) >> 1) as i32;
                *s = ((m_ext - i64::from(s0)) >> 1) as i32;
            }
            Ok(())
        }
    }
}
