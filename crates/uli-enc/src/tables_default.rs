//! The default ULI2 code tables: the format's class parameters, plus lengths derived
//! from them.
//!
//! `ctype`/`rbits`/`esck`/`mu` are **properties of the format's default table set**, not
//! of any one implementation: which classes exist, how wide their raw fields are, how far
//! an escape may run, and the geometric model each class's code is fitted to. The lengths
//! are not tabulated anywhere — they are computed from that model here, by
//! [`huff_lengths`] over a geometric distribution per class, because a pasted length table
//! would be 5,000 numbers nobody could check. Computing them also makes them auditable:
//! change the model and the lengths move for a reason.
//!
//! The **provenance** of the four parameter rows is an external source: they are written
//! down in the format's initial implementation, and this file transcribes the integers with
//! a provenance note rather than deriving them. `tests/fixtures/uli2_default_tables.bin` is
//! the whole blob as an independent producer emitted it, and `tables_golden.rs` compares
//! against it byte for byte — which is the only way to know that the derived lengths are
//! the *format's* lengths and not merely self-consistent ones.
//!
//! `uli-lite` deliberately has no write side, so the encoder owns the length tables, the
//! canonical code assignment and the blob layout. [`DefaultTables::to_blob`] reproduces the
//! geometry that is arithmetic inside `uli_lite::CodeTables::parse_into`; a wrong offset
//! there fails the parse, which is the point of the round-trip test — but see the paragraph
//! on `to_blob` for what a round-trip structurally cannot see.

use uli_lite::format::{CT_PAIR, CT_QUAD, CT_ZERO, MAXK, MAXLEN};

/// The default set's class types: one ZERO, two QUAD, eleven PAIR.
const CTYPE: [u8; 14] = [0, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2];
/// Raw-field widths per class. Zero for everything that codes small magnitudes cheaply;
/// rising to 5 on the widest class, where a sparse raw field beats an escape.
const RBITS: [u8; 14] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5];
/// Escape widths per class: how many Exp-Golomb low bits an escape carries.
const ESCK: [u8; 14] = [0, 0, 0, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1];
/// The geometric model each class's code lengths are fitted to.
///
/// Only used to *derive* the lengths; the format has no `mu` field, so it never reaches the
/// blob. Stored as integer millionths rather than `f64` so the parameter set is exact and
/// reviewable at a glance — the distribution arithmetic itself is floating point and runs
/// once per table build, not per frame.
const MU_MICRO: [i64; 14] = [
    0,
    120_000,
    450_000,
    800_000,
    1_300_000,
    2_000_000,
    3_000_000,
    4_500_000,
    6_500_000,
    10_000_000,
    20_000_000,
    40_000_000,
    80_000_000,
    160_000_000,
];

/// The number of classes the default tables declare.
pub const K: usize = 14;

/// One canonical Huffman codebook: a length per symbol and the code assigned by
/// the canonical construction `uli_lite`'s parser mirrors.
#[derive(Clone, Debug, Default)]
pub struct Codes {
    pub len: Vec<u8>,
    pub code: Vec<u32>,
}

impl Codes {
    /// Canonical codes from lengths, identical to the C `canon` and to
    /// `uli_lite::tables::Huffman::build_into`'s assignment: counts per length,
    /// `code = (code + count[l-1]) << 1` walked upwards, symbols of one length
    /// numbered in ascending symbol order.
    pub fn from_lengths(len: &[u8]) -> Self {
        let mut count = [0u32; MAXLEN + 1];
        for &l in len {
            if l != 0 {
                count[l as usize] += 1;
            }
        }
        let mut next = [0u32; MAXLEN + 1];
        let mut code = 0u32;
        for l in 1..=MAXLEN {
            code = (code + if l > 1 { count[l - 1] } else { 0 }) << 1;
            next[l] = code;
        }
        let codes = len
            .iter()
            .map(|&l| {
                if l == 0 {
                    0
                } else {
                    let c = next[l as usize];
                    next[l as usize] += 1;
                    c
                }
            })
            .collect();
        Self {
            len: len.to_vec(),
            code: codes,
        }
    }

    pub fn symbol_len(&self, symbol: usize) -> u8 {
        self.len.get(symbol).copied().unwrap_or(0)
    }

    pub fn symbol_code(&self, symbol: usize) -> u32 {
        self.code.get(symbol).copied().unwrap_or(0)
    }
}

/// The default encoder tables, ready to code frames and to serialise.
#[derive(Clone, Debug)]
pub struct DefaultTables {
    pub k: usize,
    pub ctype: [u8; MAXK],
    pub rbits: [u8; MAXK],
    pub esck: [u8; MAXK],
    pub dq: [[i8; 16]; MAXK],
    /// Band-context codebooks: `cb[context]` codes the class of the next band.
    pub cb: [Codes; MAXK + 1],
    /// Per-class coefficient codebooks (81 QUAD symbols or 256 PAIR symbols).
    pub cls: [Codes; MAXK],
}

impl DefaultTables {
    /// The class's geometric-model mean, which is what its Huffman table was built
    /// to fit. Not written into the blob (the format has no `mu` field), so it is
    /// only available here — which is exactly where S3's class choice needs it:
    /// matching the model's mean to a band's mean magnitude is a far better guess
    /// than "lowest class with enough headroom".
    pub fn mu(&self, class: usize) -> f64 {
        MU_MICRO[class] as f64 / 1e6
    }
    /// Build the default tables: the parameter rows, and lengths derived from them.
    pub fn build() -> Self {
        let mut t = Self {
            k: K,
            ctype: [0; MAXK],
            rbits: [0; MAXK],
            esck: [0; MAXK],
            dq: [[0; 16]; MAXK],
            cb: std::array::from_fn(|_| Codes::default()),
            cls: std::array::from_fn(|_| Codes::default()),
        };
        t.ctype[..K].copy_from_slice(&CTYPE);
        t.rbits[..K].copy_from_slice(&RBITS);
        t.esck[..K].copy_from_slice(&ESCK);
        for c in 0..K {
            // `tables_default` leaves `dq[c][0]` at zero and sets 1..16 to 3.
            for i in 1..16 {
                t.dq[c][i] = 3;
            }
            let mu = MU_MICRO[c] as f64 / 1e6;
            match t.ctype[c] {
                CT_QUAD => {
                    let p = geom(mu, 400);
                    let d = [p[0], p[1], 1.0 - p[0] - p[1]];
                    let mut p4 = [0.0f64; 81];
                    for (s, v) in p4.iter_mut().enumerate() {
                        *v = d[s / 27] * d[(s / 9) % 3] * d[(s / 3) % 3] * d[s % 3] * 1e6;
                    }
                    t.cls[c] = Codes::from_lengths(&huff_lengths(&p4, 0.5));
                }
                CT_PAIR => {
                    let p = geom(mu, 5000);
                    let blk = 1usize << t.rbits[c];
                    let mut ph = [0.0f64; 16];
                    let mut s15 = 0.0;
                    for j in 0..15 {
                        for i in 0..blk {
                            ph[j] += p[j * blk + i];
                        }
                        s15 += ph[j];
                    }
                    ph[15] = if 1.0 - s15 > 1e-9 { 1.0 - s15 } else { 1e-9 };
                    let mut p2 = [0.0f64; 256];
                    for (s, v) in p2.iter_mut().enumerate() {
                        *v = ph[s >> 4] * ph[s & 15] * 1e6;
                    }
                    t.cls[c] = Codes::from_lengths(&huff_lengths(&p2, 0.5));
                }
                CT_ZERO => {}
                _ => {}
            }
        }
        // All context tables are the same. The model has no distribution over contexts —
        // there is no evidence that one band's class predicts the next's beyond the trivial
        // — so a uniform distribution over the k+1 symbols is the honest choice, and it has
        // a consequence worth knowing: every class symbol costs the same from any context,
        // which is what makes per-band class choice exact (see `quant::quantise_chan_rd`).
        let ones = [1.0f64; MAXK + 1];
        let cb = Codes::from_lengths(&huff_lengths(&ones[..=K], 0.5));
        for a in 0..=K {
            t.cb[a] = cb.clone();
        }
        t
    }

    /// The tables blob, in `uli_lite::CodeTables::parse_into`'s geometry:
    /// `1 + 3k + (k+1)^2 + k*(81 + 256 + 16)` bytes, zero-padded to 4.
    ///
    /// Each class carries **one** coefficient row. The reader picks it by `ctype`
    /// (`parse_into` reads `len_q` for QUAD and `len_p` for PAIR and never the
    /// other), and the row it does not read is zeros — that is a **format invariant**, not
    /// a cosmetic choice: every committed corpus carries zeros there, and a writer that put
    /// the other shape's lengths in its place produced a 1,053-byte divergence that no
    /// round-trip could see, because `parse_into` skips exactly those bytes. The witness is
    /// `tests/fixtures/uli2_default_tables.bin`.
    pub fn to_blob(&self) -> Vec<u8> {
        let k = self.k;
        let mut out = Vec::with_capacity(self.tables_bytes());
        out.push(k as u8);
        out.extend_from_slice(&self.ctype[..k]);
        out.extend_from_slice(&self.rbits[..k]);
        out.extend_from_slice(&self.esck[..k]);
        for a in 0..=k {
            let len = &self.cb[a].len;
            out.extend_from_slice(&len[..=k]);
        }
        // The one row each class contributes, and only for its own `ctype`. The
        // width check inside `coeff_row` is the guard at the root of the row leak: a PAIR
        // class's 256 lengths passed to an 81-byte row used to be silently
        // truncated by `write_row`, which is how the leak happened in the first
        // place and how it stayed invisible.
        for c in 0..k {
            let row = self.coeff_row(c, CT_QUAD, 81);
            write_row(&mut out, row, 81);
        }
        for c in 0..k {
            let row = self.coeff_row(c, CT_PAIR, 256);
            write_row(&mut out, row, 256);
        }
        for c in 0..k {
            for j in 0..16 {
                out.push(self.dq[c][j] as u8);
            }
        }
        while out.len() & 3 != 0 {
            out.push(0);
        }
        out
    }

    /// The class's `len_q` row (shape `CT_QUAD`, 81 wide) or `len_p` row
    /// (`CT_PAIR`, 256 wide) — or nothing, when the class is some other shape.
    ///
    /// A class that is not this shape contributes zeros, because `parse_into`
    /// reads one row per class and selects it by `ctype`; see [`Self::to_blob`].
    /// The width assertion is not belt-and-braces: it is what makes the leak
    /// structurally impossible rather than merely absent today, since a row whose
    /// length exceeds the slot gets truncated, not rejected.
    fn coeff_row(&self, class: usize, shape: u8, width: usize) -> &[u8] {
        if self.ctype[class] != shape {
            return &[];
        }
        let len = &self.cls[class].len;
        assert!(
            len.len() == width,
            "class {class} is ctype {shape} but its codebook holds {} lengths, not \
             {width}: the coefficient rows are fixed-size, so a short or long row is a \
             table built for the other shape — the row leak, caught one step earlier",
            len.len()
        );
        len
    }

    /// The serialised blob's length, padding included.
    pub fn tables_bytes(&self) -> usize {
        let k = self.k;
        let raw = 1 + 3 * k + (k + 1) * (k + 1) + k * (81 + 256 + 16);
        (raw + 3) & !3
    }
}

/// Write exactly `n` bytes of `len`, zero-padded: the fixed-size rows of the
/// tables blob. A `len` longer than the row is a bug, not padding — truncating it
/// silently is what let one shape's lengths stand inside the other shape's slot.
fn write_row(out: &mut Vec<u8>, len: &[u8], n: usize) {
    assert!(
        len.len() <= n,
        "a {n}-byte row cannot hold {} lengths without truncating them",
        len.len()
    );
    for i in 0..n {
        out.push(len.get(i).copied().unwrap_or(0));
    }
}

/// A geometric distribution with mean `mu`: `th = mu / (1 + mu)`, `p[k] = (1 - th)·th^k`.
///
/// A magnitude's coded size is roughly geometric — small levels are common, each step up is
/// rarer by a constant factor — so one parameter per class is enough to fit a family of
/// rates, and `mu` is that factor expressed as a mean.
fn geom(mu: f64, nmax: usize) -> Vec<f64> {
    let th = mu / (1.0 + mu);
    (0..=nmax).map(|k| (1.0 - th) * th.powf(k as f64)).collect()
}

/// Huffman code lengths for a distribution: add a frequency floor, merge the two smallest
/// nodes until one tree is left, and if any leaf lands deeper than `MAXLEN`, soften the
/// distribution (`√c + floor`) and re-merge.
///
/// Two details decide the output and both are stated rather than incidental. The **floor**
/// keeps a symbol that never occurs from being uncodeable — a length-0 symbol cannot be
/// written, and the difference between "rare" and "impossible" matters once tables are
/// trained on a finite sample. The **re-merge** exists because a two-minimum merge on a very
/// skewed distribution degenerates into a chain; softening compresses the tail without
/// changing the ordering. Ties go to the lowest node index, so the merge is deterministic
/// and the same distribution always yields the same lengths.
fn huff_lengths(counts: &[f64], floor: f64) -> Vec<u8> {
    let n = counts.len();
    let mut c: Vec<f64> = counts.iter().map(|v| v + floor).collect();
    let mut len = vec![0u8; n];
    if n == 1 {
        len[0] = 1;
        return len;
    }
    loop {
        let mut w = vec![0.0f64; 2 * n];
        let mut parent = vec![-1i32; 2 * n];
        let mut active = vec![false; 2 * n];
        for i in 0..n {
            w[i] = c[i];
            active[i] = true;
        }
        let mut nn = n;
        for _ in 0..n - 1 {
            let mut a: i32 = -1;
            let mut b: i32 = -1;
            for i in 0..nn {
                if !active[i] {
                    continue;
                }
                if a < 0 || w[i] < w[a as usize] {
                    b = a;
                    a = i as i32;
                } else if b < 0 || w[i] < w[b as usize] {
                    b = i as i32;
                }
            }
            active[a as usize] = false;
            active[b as usize] = false;
            w[nn] = w[a as usize] + w[b as usize];
            parent[a as usize] = nn as i32;
            parent[b as usize] = nn as i32;
            parent[nn] = -1;
            active[nn] = true;
            nn += 1;
        }
        let mut max = 0u8;
        for i in 0..n {
            let mut d = 0u8;
            let mut p = i as i32;
            while parent[p as usize] >= 0 {
                d += 1;
                p = parent[p as usize];
            }
            len[i] = d;
            max = max.max(d);
        }
        if max <= MAXLEN as u8 {
            return len;
        }
        for v in c.iter_mut() {
            *v = v.sqrt() + floor;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uli_lite::CodeTables;

    #[test]
    fn the_blob_parses_back_to_the_same_rows() {
        let t = DefaultTables::build();
        let blob = t.to_blob();
        assert_eq!(blob.len(), t.tables_bytes());
        assert_eq!(blob.len(), 5212, "the measured corpus tables region");

        let mut parsed = CodeTables::new();
        parsed.parse_into(&blob).expect("default tables must parse");
        assert_eq!(parsed.k, K);
        assert_eq!(&parsed.ctype[..K], &CTYPE);
        assert_eq!(&parsed.rbits[..K], &RBITS);
        assert_eq!(&parsed.esck[..K], &ESCK);
        for c in 0..K {
            assert_eq!(&parsed.dq[c], &t.dq[c]);
        }
        parsed
            .premise()
            .expect("the default tables are inside E-R4 by construction");
    }

    /// The invariant the row leak broke, asserted on the bytes rather than on the model.
    ///
    /// `parse_into` selects a class's coefficient row by `ctype` and never reads the other
    /// one, so a round-trip is blind to whatever sits there — which is how every test in this
    /// crate passed while 1,053 bytes disagreed with the format. Every committed corpus
    /// carries zeros in the unused row, so this is an invariant, not a preference.
    #[test]
    fn the_unused_coefficient_row_of_every_class_is_zero() {
        let t = DefaultTables::build();
        let blob = t.to_blob();
        let k = t.k;
        let len_q = 1 + 3 * k + (k + 1) * (k + 1);
        let len_p = len_q + 81 * k;

        for c in 0..k {
            let q = &blob[len_q + 81 * c..len_q + 81 * (c + 1)];
            let p = &blob[len_p + 256 * c..len_p + 256 * (c + 1)];
            let (live, unused, unused_name) = match t.ctype[c] {
                CT_QUAD => (q, p, "len_p"),
                CT_PAIR => (p, q, "len_q"),
                // A ZERO class has no coefficient row at all: both must be empty,
                // and the live/unused pair below has no subject here.
                _ => {
                    assert!(
                        q.iter().all(|&x| x == 0) && p.iter().all(|&x| x == 0),
                        "class {c} is ZERO, so neither of its rows may carry lengths"
                    );
                    continue;
                }
            };
            assert!(
                live.iter().any(|&x| x != 0),
                "class {c} has an all-zero live row, which would leave the unused-row check comparing nothing"
            );
            assert!(
                unused.iter().all(|&x| x == 0),
                "class {c} leaks its other shape's lengths into {unused_name}; the reader picks the row by ctype and never sees this, which is exactly why it reached a committed test unnoticed once already"
            );
        }
    }

    #[test]
    fn every_default_codebook_is_a_complete_prefix_code() {
        let t = DefaultTables::build();
        for a in 0..=K {
            assert_kraft(&t.cb[a], K + 1);
        }
        for c in 0..K {
            match t.ctype[c] {
                CT_QUAD => assert_kraft(&t.cls[c], 81),
                CT_PAIR => assert_kraft(&t.cls[c], 256),
                CT_ZERO => {}
                _ => {}
            }
        }
    }

    fn assert_kraft(codes: &Codes, n: usize) {
        let mut used = vec![];
        for s in 0..n {
            let l = codes.symbol_len(s);
            assert!(l >= 1 && l as usize <= MAXLEN, "symbol {s} has len {l}");
            used.push((l, codes.symbol_code(s)));
        }
        // Canonical construction plus a positive weight for every symbol makes
        // the code complete: sum of 2^-len is exactly 1.
        let sum: f64 = used.iter().map(|&(l, _)| 1.0 / (1u64 << l) as f64).sum();
        assert!(
            (sum - 1.0).abs() < 1e-12,
            "incomplete code: kraft sum {sum}"
        );
        // Codes are in range for their length.
        for &(l, c) in &used {
            assert!(c < (1u32 << l), "code {c} does not fit in {l} bits");
        }
    }

    #[test]
    fn remerge_brings_a_pathological_distribution_back_under_maxlen() {
        // Fibonacci weights over 256 leaves: the plain two-min merge puts the
        // rarest leaf at depth ~255 (a chain, not a tree). The sqrt+floor loop
        // must bring every length back to <= MAXLEN.
        let mut counts = vec![1.0f64; 256];
        for i in 2..256 {
            counts[i] = counts[i - 1] + counts[i - 2];
        }
        let len = huff_lengths(&counts, 0.5);
        assert_eq!(len.len(), 256);
        assert!(
            len.iter().all(|&l| l >= 1 && l as usize <= MAXLEN),
            "max length {:?} exceeds MAXLEN after the re-merge",
            len.iter().max()
        );
        let sum: f64 = len.iter().map(|&l| 1.0 / (1u64 << l) as f64).sum();
        assert!(
            (sum - 1.0).abs() < 1e-12,
            "incomplete code: kraft sum {sum}"
        );
    }

    #[test]
    fn the_canonical_codes_decode_as_uli_lite_reads_them() {
        // The decoder's lookup compares `peek(l) - first[l]` against counts; a
        // code written here must therefore be `first[len] + rank among symbols of
        // that length`. Rebuild the same assignment independently and compare.
        let t = DefaultTables::build();
        for c in 0..K {
            if t.ctype[c] == CT_ZERO {
                continue;
            }
            let n = if t.ctype[c] == CT_QUAD { 81 } else { 256 };
            let mut first = [0u32; MAXLEN + 1];
            let mut count = [0u32; MAXLEN + 1];
            for s in 0..n {
                count[t.cls[c].symbol_len(s) as usize] += 1;
            }
            let mut code = 0u32;
            for l in 1..=MAXLEN {
                code = (code + if l > 1 { count[l - 1] } else { 0 }) << 1;
                first[l] = code;
            }
            let mut rank = [0u32; MAXLEN + 1];
            for s in 0..n {
                let l = t.cls[c].symbol_len(s) as usize;
                assert_eq!(
                    t.cls[c].symbol_code(s),
                    first[l] + rank[l],
                    "class {c} symbol {s} is not at the canonical position"
                );
                rank[l] += 1;
            }
        }
    }
}
