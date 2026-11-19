//! ULA2 tables-blob parser and canonical Huffman decoder.
//!
//! Mirrors `uli_tables_parse` / `uli_huff_build` in the C reference. The
//! `Huffman` layout intentionally keeps the 10-bit first-level LUT: it is an
//! implementation detail (not part of the bitstream) and can be narrowed later
//! if a product needs the EWRAM back.

use core::fmt;

use crate::Error;
use crate::bitstream::BitReader;
use crate::format::{
    CT_PAIR, CT_QUAD, CT_ZERO, ENC_ESCK_MAX, ENC_PAIR_RBITS_MAX, LUTB, MAXK, MAXLEN,
};
use crate::frame::Ops;

/// One canonical Huffman table.
#[derive(Clone, Copy)]
pub struct Huffman {
    len: [u8; 256],
    lut: [u16; 1 << LUTB],
    first: [u16; MAXLEN + 1],
    count: [u16; MAXLEN + 1],
    offs: [u16; MAXLEN + 1],
    sym: [u16; 256],
    nsym: u16,
}

impl fmt::Debug for Huffman {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Huffman")
            .field("nsym", &self.nsym)
            .field("count", &self.count)
            .finish_non_exhaustive()
    }
}

impl Huffman {
    pub const ZERO: Self = Self {
        len: [0; 256],
        lut: [0; 1 << LUTB],
        first: [0; MAXLEN + 1],
        count: [0; MAXLEN + 1],
        offs: [0; MAXLEN + 1],
        sym: [0; 256],
        nsym: 0,
    };

    fn build_into(&mut self, lens: &[u8], nsym: usize) -> Result<(), Error> {
        if nsym > 256 || lens.len() < nsym {
            return Err(Error::BadTables);
        }
        *self = Self::ZERO;
        self.nsym = nsym as u16;
        for s in 0..nsym {
            let l = lens[s] as usize;
            if l > MAXLEN {
                return Err(Error::BadTables);
            }
            self.len[s] = l as u8;
            if l != 0 {
                self.count[l] = self.count[l].checked_add(1).ok_or(Error::BadTables)?;
            }
        }

        let mut code = 0u32;
        let mut idx = 0u16;
        let mut next = [0u16; MAXLEN + 1];
        for l in 1..=MAXLEN {
            code = (code + if l > 1 { self.count[l - 1] as u32 } else { 0 }) << 1;
            self.first[l] = code as u16;
            next[l] = code as u16;
            self.offs[l] = idx;
            idx = idx.checked_add(self.count[l]).ok_or(Error::BadTables)?;
        }
        if idx as usize > 256 {
            return Err(Error::BadTables);
        }

        for l in 1..=MAXLEN {
            let mut o = self.offs[l] as usize;
            for s in 0..nsym {
                if self.len[s] as usize != l {
                    continue;
                }
                if o >= 256 {
                    return Err(Error::BadTables);
                }
                self.sym[o] = s as u16;
                o += 1;
                let c = next[l] as usize;
                next[l] = next[l].wrapping_add(1);
                if l <= LUTB {
                    let sh = LUTB - l;
                    let base = c << sh;
                    let n = 1usize << sh;
                    let end = base.checked_add(n).ok_or(Error::BadTables)?;
                    if end > self.lut.len() {
                        return Err(Error::BadTables);
                    }
                    let e = ((s as u16) << 5) | l as u16;
                    for slot in &mut self.lut[base..end] {
                        *slot = e;
                    }
                }
            }
        }
        Ok(())
    }

    fn decode(&self, br: &mut BitReader<'_>, ops: &mut Ops) -> Result<u16, Error> {
        let idx = br.peek(LUTB as u8)? as usize;
        let e = self.lut[idx];
        if e != 0 {
            br.skip((e & 31) as u8)?;
            return Ok(e >> 5);
        }
        for l in (LUTB + 1)..=MAXLEN {
            let c = br.peek(l as u8)?.wrapping_sub(self.first[l] as u32);
            if c < self.count[l] as u32 {
                br.skip(l as u8)?;
                ops.long_codes = ops.long_codes.wrapping_add(1);
                let i = self.offs[l] as usize + c as usize;
                return self.sym.get(i).copied().ok_or(Error::BadTables);
            }
        }
        Err(Error::BadBitstream)
    }
}

/// Raw views of one Huffman table for the GBA product's hot decoder.
///
/// Exposed only under the `product` feature. The slices are the table's own
/// storage; the product indexes them with `get_unchecked` after the canonical
/// table parser has validated every offset.
#[cfg(feature = "product")]
#[derive(Clone, Copy, Debug)]
pub struct HuffmanParts<'a> {
    pub lut: &'a [u16],
    pub first: &'a [u16],
    pub count: &'a [u16],
    pub offs: &'a [u16],
    pub sym: &'a [u16],
}

#[cfg(feature = "product")]
impl Huffman {
    pub fn parts(&self) -> HuffmanParts<'_> {
        HuffmanParts {
            lut: &self.lut,
            first: &self.first,
            count: &self.count,
            offs: &self.offs,
            sym: &self.sym,
        }
    }

    /// The LUT base pointer, for the product's hot decoder.
    ///
    /// The GBA hot path keeps **one** pointer per active table rather than a
    /// [`HuffmanParts`] value: five slices are ten live words, and that pressure
    /// spilled the decoder's window to the stack (Step 1 measured 218 stack
    /// accesses in `dec_chan`). `lut_ptr` plus a `&Huffman` for the cold slow
    /// path is two.
    pub fn lut_ptr(&self) -> *const u16 {
        self.lut.as_ptr()
    }
}

/// What [`CodeTables::premise`] counted, so a caller can require the check had a
/// subject instead of trusting a loop that may have run over no class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PremiseCensus {
    pub zero: usize,
    pub pair: usize,
    pub quad: usize,
    pub max_pair_rbits: u8,
    pub max_esck: u8,
}

/// Which part of the closed-world premise a class failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PremiseViolationKind {
    /// The tables declare more classes than [`MAXK`].
    KTooLarge,
    /// A `ctype` that is none of ZERO / QUAD / PAIR — an error, not a skip.
    Ctype,
    /// `rbits` above [`ENC_PAIR_RBITS_MAX`] on a PAIR class.
    PairRbits,
    /// `esck` above [`ENC_ESCK_MAX`].
    Esck,
}

/// The first class that fails the encoder's closed-world premise (E-R4).
///
/// `uli-lite` is `no_std` and allocation-free, so the host caller formats this
/// rather than receiving a message; [`fmt::Display`] still renders it verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PremiseViolation {
    pub class: usize,
    pub kind: PremiseViolationKind,
    pub value: u8,
}

impl fmt::Display for PremiseViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            PremiseViolationKind::KTooLarge => {
                write!(f, "class count k={} exceeds MAXK={MAXK}", self.value)
            }
            PremiseViolationKind::Ctype => write!(
                f,
                "class {} has ctype={}, which is not ZERO({CT_ZERO}) / QUAD({CT_QUAD}) / \
                 PAIR({CT_PAIR})",
                self.class, self.value
            ),
            PremiseViolationKind::PairRbits => write!(
                f,
                "class {} is PAIR with rbits={} > {ENC_PAIR_RBITS_MAX}: a raw pair digit \
                 costs rbits + 1 bits after the group's refill, so the encoder must refuse \
                 it (the 16-bit window itself only breaks at rbits > 15, and the table \
                 parser accepts 16 — E-R4 is the tighter bound by choice)",
                self.class, self.value
            ),
            PremiseViolationKind::Esck => write!(
                f,
                "class {} has esck={} > {ENC_ESCK_MAX}: the escape field is \
                 t = 2z + 1 + esck with z <= 5, so the 16-bit window's own limit is \
                 esck <= 5 — E-R4 is the tighter bound by choice",
                self.class, self.value
            ),
        }
    }
}

/// Parsed per-album code tables.
#[derive(Clone)]
pub struct CodeTables {
    pub k: usize,
    pub ctype: [u8; MAXK],
    pub rbits: [u8; MAXK],
    pub esck: [u8; MAXK],
    pub dq: [[i8; 16]; MAXK],
    /// `rq[c][m] = 16 * m - dq[c][m]` for `m < 16`, precomputed once per album
    /// so the hot dequant is a load instead of a multiply and a clamp
    /// (PR C-3a). Values stay in `i16`: `16 * 15 + 128` is the extreme.
    pub rq: [[i16; 16]; MAXK],
    /// `dq[c][15]`, the shape reused for escaped magnitudes `m >= 16`, where
    /// the reference computes `16 * m - dq[c][15]`.
    pub dq15: [i8; MAXK],
    cb: [Huffman; MAXK + 1],
    cls: [Huffman; MAXK],
}

impl fmt::Debug for CodeTables {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodeTables")
            .field("k", &self.k)
            .field("ctype", &&self.ctype[..self.k.min(MAXK)])
            .field("rbits", &&self.rbits[..self.k.min(MAXK)])
            .field("esck", &&self.esck[..self.k.min(MAXK)])
            .finish_non_exhaustive()
    }
}

impl CodeTables {
    pub const fn new() -> Self {
        Self {
            k: 0,
            ctype: [0; MAXK],
            rbits: [0; MAXK],
            esck: [0; MAXK],
            dq: [[0; 16]; MAXK],
            rq: [[0; 16]; MAXK],
            dq15: [0; MAXK],
            cb: [Huffman::ZERO; MAXK + 1],
            cls: [Huffman::ZERO; MAXK],
        }
    }

    /// Parse and build the tables blob. On failure the contents are undefined
    /// but the object remains safe to drop/reuse.
    pub fn parse_into(&mut self, blob: &[u8]) -> Result<(), Error> {
        *self = Self::new();
        let k = *blob.first().ok_or(Error::BadTables)? as usize;
        if !(2..=MAXK).contains(&k) {
            return Err(Error::BadTables);
        }
        let need = 1usize
            .checked_add(3 * k)
            .and_then(|v| v.checked_add((k + 1) * (k + 1)))
            .and_then(|v| v.checked_add(k * (81 + 256 + 16)))
            .ok_or(Error::BadTables)?;
        if blob.len() < need {
            return Err(Error::BadTables);
        }

        self.k = k;
        let mut p = 1;
        self.ctype[..k].copy_from_slice(&blob[p..p + k]);
        p += k;
        self.rbits[..k].copy_from_slice(&blob[p..p + k]);
        p += k;
        self.esck[..k].copy_from_slice(&blob[p..p + k]);
        p += k;
        for c in 0..k {
            if self.rbits[c] > 16 || self.esck[c] > 24 {
                return Err(Error::BadTables);
            }
        }

        for c in 0..=k {
            self.cb[c].build_into(&blob[p..p + k + 1], k + 1)?;
            p += k + 1;
        }

        let lq = p;
        let lp = lq + 81 * k;
        p += 337 * k;
        for c in 0..k {
            match self.ctype[c] {
                CT_QUAD => self.cls[c].build_into(&blob[lq + 81 * c..lq + 81 * (c + 1)], 81)?,
                CT_PAIR => self.cls[c].build_into(&blob[lp + 256 * c..lp + 256 * (c + 1)], 256)?,
                CT_ZERO => self.cls[c] = Huffman::ZERO,
                _ => self.cls[c] = Huffman::ZERO,
            }
        }

        for c in 0..k {
            for j in 0..16 {
                self.dq[c][j] = blob[p] as i8;
                p += 1;
            }
        }
        // PR C-3a: the dequant shape `16 * m - dq[c][m]` is fixed per album, so
        // compute it once here instead of per nonzero coefficient.
        for c in 0..k {
            for j in 0..16 {
                self.rq[c][j] = 16 * j as i16 - self.dq[c][j] as i16;
            }
            self.dq15[c] = self.dq[c][15];
        }
        Ok(())
    }

    /// Check the tables against the encoder's **closed-world premise** (E-R4):
    /// `rbits <= ENC_PAIR_RBITS_MAX` on every PAIR class and
    /// `esck <= ENC_ESCK_MAX` on every class, ZERO included (stricter than the
    /// decoder needs, and cheap).
    ///
    /// The parser is deliberately looser (`rbits <= 16`, `esck <= 24`) because it
    /// also reads other producers' blobs; nothing downstream of the encoder
    /// rejects a table that breaks the product reader, so the encoder owns the
    /// bound and calls this before emitting a byte. Read-side validation only:
    /// this adds no write path to the decoder.
    pub fn premise(&self) -> Result<PremiseCensus, PremiseViolation> {
        if self.k > MAXK {
            return Err(PremiseViolation {
                class: self.k,
                kind: PremiseViolationKind::KTooLarge,
                value: self.k.min(u8::MAX as usize) as u8,
            });
        }

        let mut census = PremiseCensus {
            zero: 0,
            pair: 0,
            quad: 0,
            max_pair_rbits: 0,
            max_esck: 0,
        };

        for i in 0..self.k {
            match self.ctype[i] {
                CT_ZERO => census.zero += 1,
                CT_QUAD => census.quad += 1,
                CT_PAIR => {
                    census.pair += 1;
                    // The raw digit is read only on PAIR, which is exactly why
                    // the class id has to be the real one.
                    if self.rbits[i] > ENC_PAIR_RBITS_MAX {
                        return Err(PremiseViolation {
                            class: i,
                            kind: PremiseViolationKind::PairRbits,
                            value: self.rbits[i],
                        });
                    }
                    census.max_pair_rbits = census.max_pair_rbits.max(self.rbits[i]);
                }
                other => {
                    return Err(PremiseViolation {
                        class: i,
                        kind: PremiseViolationKind::Ctype,
                        value: other,
                    });
                }
            }
            // A ZERO class never escapes, but binding esck everywhere is the
            // cheap half of E-R4 and keeps the rule one sentence long.
            if self.esck[i] > ENC_ESCK_MAX {
                return Err(PremiseViolation {
                    class: i,
                    kind: PremiseViolationKind::Esck,
                    value: self.esck[i],
                });
            }
            census.max_esck = census.max_esck.max(self.esck[i]);
        }

        Ok(census)
    }

    pub fn class_table(&self, class: usize) -> &Huffman {
        &self.cls[class]
    }

    pub fn band_table(&self, context: usize) -> &Huffman {
        &self.cb[context]
    }

    pub(crate) fn huff_cb(
        &self,
        context: usize,
        br: &mut BitReader<'_>,
        ops: &mut Ops,
    ) -> Result<u16, Error> {
        self.cb.get(context).ok_or(Error::BadFrame)?.decode(br, ops)
    }

    pub(crate) fn huff_cls(
        &self,
        class: usize,
        br: &mut BitReader<'_>,
        ops: &mut Ops,
    ) -> Result<u16, Error> {
        self.cls.get(class).ok_or(Error::BadFrame)?.decode(br, ops)
    }
}
