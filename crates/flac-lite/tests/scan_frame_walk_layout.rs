//! Item 4b, witnessed against the generated stream table: what a frame walk
//! that indexes NOTHING must see on files `flac` 1.5.0 wrote, frame by frame.
//!
//! `src/scan.rs`'s unit tests are hand-built on purpose — they can only speak
//! about state machines. The claims that a walk gets real encoder bytes right
//! belong here, against rows no Rust code produced:
//!
//! ```text
//! frame_region_len                 the region a frame walk is handed
//! frame_offsets / frame_bytes      every frame's start, and its OWN length, the
//!                                  latter derived twice in the generator (by
//!                                  walking its subfields, and by the framing
//!                                  distance to the next frame / EOF)
//! frame_header_bits                that same frame's header width, from the
//!                                  independent parser — see the table's own
//!                                  comment: bytes*8 = header + body + gap
//! frame_footer_gaps_bits           the padding+CRC-16 tail: 16..23 bits, never
//!                                  a constant 16
//! frame_blocksizes                 per-frame sample count; the tail is short,
//!                                  and s3's are 3000/1801, values NO size
//!                                  table contains
//! frame_numbers                    the sequence a walk must refuse if it slips
//! frame_sample_size_codes          each frame's own stated depth (`0b110` = 24
//!                                  for s4): the depth boundary is the
//!                                  fixture's own claim, not this crate's guess
//! scan_frame_starts_*              the candidate-filter ladder counted over a
//!                                  whole region by the generator: sync shape
//!                                  only, fields without the header CRC, strict,
//!                                  and strict with the number-form rule
//!                                  relaxed. On these five regions all four legs
//!                                  equal the frame count — the regions hold no
//!                                  false candidates, and the table says so. The
//!                                  measurement where the legs separate (1223
//!                                  candidates for 979 real frames) is in longer
//!                                  material, recorded in `src/lib.rs`. Note
//!                                  also that the generator's filter accepts
//!                                  every sample-size code, so its count for s4
//!                                  is 10 while this crate's is 0: see
//!                                  `the_depth_boundary_applies_to_locating_
//!                                  frames_as_much_as_to_decoding_them`.
//! frame_exact_count_from_declarations
//!                                  what the stream's OWN declarations can say
//!                                  about where its audio ends — `none` where
//!                                  they cannot
//! pcm_interleaved_values / pcm_fnv1a64 / pcm_md5 / pcm_bytes_len
//!                                  the audio, from `flac -d`
//! ```
//!
//! Nothing here is an expectation the crate emitted. Per-frame lengths come from
//! an independent second reading of the format (a Python walk over each frame's
//! own subfields, which never looks at the next frame), the offsets and the
//! strict filter's counts come from `flac`'s own bytes being re-parsed, and the
//! PCM pins are the reference DECODER's output hashed with FNV-1a-64 — the same
//! hash the generator and the Rust suites reimplement independently.
//!
//! The failure cases here are NOT fixtures. A truncated stream, a duplicated
//! frame, a region that begins mid-frame and a region with trailing bytes are
//! built by cutting and re-joining the fixture bytes in this file, for the
//! reason tests/README.md gives: the fixtures earn their place only by being
//! things a real encoder emitted, and a half-frame is not one.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use flac_lite::frame::{self, StreamDefaults};
use flac_lite::scan::{self, ConsumedFrame, Stats};
use flac_lite::stream;
use flac_lite::{Error, PredictorState};

// ---------------------------------------------------------------------------
// The fixtures, embedded. The table names each file; an entry has to exist here
// for it to be read at all, so a sixth fixture fails this suite rather than
// being silently skipped — the same fail-closed rule `stream_metadata_layout.rs`
// (item 4a's suite) applies, and the reason is the same: a fixture that arrives
// by accident witnesses nothing.
// ---------------------------------------------------------------------------

fn fixture_bytes(name: &str) -> &'static [u8] {
    match name {
        "s1_plain_8k_mono.flac" => include_bytes!("fixtures/s1_plain_8k_mono.flac"),
        "s2_seekable_96k_stereo.flac" => include_bytes!("fixtures/s2_seekable_96k_stereo.flac"),
        "s3_wide_48001hz_b3000.flac" => include_bytes!("fixtures/s3_wide_48001hz_b3000.flac"),
        "s4_24bit_8k_mono.flac" => include_bytes!("fixtures/s4_24bit_8k_mono.flac"),
        "s5_unseekable_total_unknown.flac" => {
            include_bytes!("fixtures/s5_unseekable_total_unknown.flac")
        }
        other => panic!(
            "the table names a fixture this test does not embed: {other}\n\
             add it to fixture_bytes() (and, if it witnesses a new rule, add an \
             assertion) rather than dropping it from the table."
        ),
    }
}

/// One stream: its table rows, and the bytes those rows were produced from.
struct Vector {
    name: String,
    rows: BTreeMap<String, String>,
    bytes: &'static [u8],
}

impl Vector {
    fn raw(&self, key: &str) -> String {
        self.rows
            .get(key)
            .unwrap_or_else(|| panic!("{}: no `{key}` row in the table", self.name))
            .clone()
    }

    fn num(&self, key: &str) -> u64 {
        self.raw(key).parse().unwrap_or_else(|_| {
            panic!(
                "{}: `{key}` is not a number: {:?}",
                self.name,
                self.raw(key)
            )
        })
    }

    fn has(&self, key: &str) -> bool {
        self.rows.contains_key(key)
    }

    /// A per-frame row as a list, length-checked against the frame count the
    /// moment it is read, so a malformed row fails at the row and not in the
    /// middle of a walk.
    fn per_frame(&self, key: &str) -> Vec<u64> {
        let items: Vec<u64> = self
            .raw(key)
            .split_whitespace()
            .map(|v| {
                v.parse()
                    .unwrap_or_else(|_| panic!("{}: `{key}` item {v:?} is not a number", self.name))
            })
            .collect();
        let n = self.num("frame_count") as usize;
        assert_eq!(
            items.len(),
            n,
            "{}: `{key}` has {} entries for {} frames",
            self.name,
            items.len(),
            n
        );
        items
    }

    /// Region-relative versions of the file-absolute `frame_offsets`. A walk is
    /// handed a region, so its offsets live in that region's own coordinates;
    /// this is where the two coordinate systems are reconciled, once.
    fn region_offsets(&self, base: usize) -> Vec<usize> {
        self.per_frame("frame_offsets")
            .iter()
            .map(|o| {
                (*o as usize).checked_sub(base).unwrap_or_else(|| {
                    panic!(
                        "{}: frame offset {o} is before the frame region at {base}",
                        self.name
                    )
                })
            })
            .collect()
    }

    fn fixture(&self) -> &'static [u8] {
        self.bytes
    }

    /// A fixture is 24-bit exactly when its streaminfo says so — the one class
    /// of stream this suite must not ask the DECODER to carry while still
    /// asking the FRAME layer about it (separate questions; see
    /// `the_24_bit_fixture_is_refused_at_its_depth_and_not_at_its_framing`).
    fn bits_per_sample(&self) -> u64 {
        self.num("streaminfo_bits_per_sample")
    }

    fn channels(&self) -> usize {
        self.num("streaminfo_channels") as usize
    }
}

fn load_vectors() -> Vec<Vector> {
    let table = include_str!("stream_vectors.txt");
    let mut out: Vec<Vector> = Vec::new();
    for (lineno, line) in table.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        if key == "stream_vector" {
            let name = rest.trim().to_string();
            out.push(Vector {
                bytes: fixture_bytes(&format!("{name}.flac")),
                name,
                rows: BTreeMap::new(),
            });
            continue;
        }
        let v = out.last_mut().unwrap_or_else(|| {
            panic!("line {}: {key:?} outside a stream_vector block", lineno + 1)
        });
        // `block …` and `mutation*` rows are item 4a's subject and are read by
        // its own suite; storing them here would only invite a last-one-wins
        // collision on a key this suite never resolves.
        if key == "block" || key.starts_with("mutation") {
            continue;
        }
        v.rows.insert(key.to_string(), rest.trim().to_string());
    }
    assert!(!out.is_empty(), "the table names no streams");
    out
}

/// FNV-1a 64-bit over bytes, same constants as the generator and the existing
/// suites: two independent implementations meeting on the same bytes.
fn fnv1a64(state: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *state ^= *b as u64;
        *state = state.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

/// Accumulates the samples a walk wrote, in the order the format interleaves
/// them, so the walk's audio can be hashed against `pcm_fnv1a64` — the hash of
/// the reference decoder's whole-file output.
struct PcmPin {
    hash: u64,
    values: usize,
}

impl PcmPin {
    fn new() -> Self {
        PcmPin {
            hash: 0xcbf4_3ce9_5de6_84b7,
            values: 0,
        }
    }

    /// The frame's samples AS THE STREAM EMITS THEM: left then (if the frame
    /// has a second subframe) right, per sample, each stored as the reference
    /// decoder stores it — little-endian, 16-bit. Only the 16-bit fixtures
    /// carry PCM rows, and the test that reads them pins the width
    /// (`pcm_bytes_len == 2 × pcm_interleaved_values`), so this hash cannot be
    /// quietly comparing a different encoding than the row was made of.
    fn push(&mut self, frame: &ConsumedFrame, left: &[i32], right: &[i32]) {
        let stereo = frame.header.channels.subframe_count() == 2;
        for i in 0..frame.samples {
            let l = (left[i] as u16).to_le_bytes();
            fnv1a64(&mut self.hash, &l);
            self.values += 1;
            if stereo {
                let r = (right[i] as u16).to_le_bytes();
                fnv1a64(&mut self.hash, &r);
                self.values += 1;
            }
        }
    }
}

/// Allocates caller buffers as large as this stream's own declarations allow a
/// frame to be — the caller side of `Frames`' contract, from the stream's own
/// numbers rather than from a per-fixture constant a new fixture would break.
fn caller_buffers(v: &Vector, facts: &stream::StreamFacts) -> (Vec<i32>, Vec<i32>) {
    let declared = (v
        .num("streaminfo_maximum_blocksize")
        .max(v.num("streaminfo_minimum_blocksize"))) as usize;
    let bound = declared
        .max(facts.info.max_blocksize as usize)
        .max(v.per_frame("frame_blocksizes")[0] as usize)
        .max(1);
    (vec![0i32; bound], vec![0i32; bound])
}

/// The stream's own defaults, from the table's streaminfo rows — the caller
/// side of `Frames::region`, whose defaults are the caller's to supply. Built
/// from ground truth, not from `StreamInfo::as_defaults`, so a parser that got
/// the streaminfo fields wrong would be caught here as well as in 4a's suite.
fn table_defaults(v: &Vector) -> StreamDefaults {
    StreamDefaults {
        sample_rate_hz: v.num("streaminfo_sample_rate_hz") as u32,
        bits_per_sample: v.num("streaminfo_bits_per_sample") as u8,
    }
}

/// A stream whose whole frame region is in hand: the region a `Frames::stream`
/// walk is pointed at, and the base offset to add to convert its region-relative
/// offsets into file offsets.
fn frame_region(v: &Vector, facts: &stream::StreamFacts) -> &'static [u8] {
    &v.fixture()[facts.frame_data_offset..]
}

fn file_offset(facts: &stream::StreamFacts, region_offset: usize) -> u64 {
    facts.frame_data_offset as u64 + region_offset as u64
}

// ---------------------------------------------------------------- the walk

/// The suite's main test: a `Frames` walk over a whole `.flac` file, with no
/// index, no byte stride, no declared stop, reproducing every per-frame row the
/// generator derived independently — offsets, lengths, header widths, sample
/// counts and frame numbers — and ending on the region's last byte.
///
/// The rows being matched are not the crate's: `frame_offsets` and the strict
/// filter's counts come from re-parsing `flac`'s own bytes in Python, and
/// `frame_bytes` / `frame_header_bits` / `frame_footer_gaps_bits` come from
/// walking each frame's subfields without ever looking at the next frame. The
/// generator refuses to write the table unless those two derivations of each
/// length agree; this test asks whether a Rust walk that never looks at the
/// next frame either agrees with them.
#[test]
fn a_frame_walk_with_no_index_reproduces_every_frame_row() {
    for v in load_vectors() {
        // The 24-bit fixture is refused by this crate's DECODER at the depth
        // gate; its framing is witnessed by
        // `the_24_bit_fixture_is_refused_at_its_depth_not_at_its_framing`.
        if v.bits_per_sample() == 24 {
            continue;
        }
        let bytes = v.fixture();
        let facts =
            stream::parse(bytes).unwrap_or_else(|e| panic!("{}: stream::parse: {e:?}", v.name));
        assert_eq!(
            facts.frame_data_offset as u64,
            v.num("frame_data_offset"),
            "{}: frame region does not begin where the table says",
            v.name
        );
        let region = frame_region(&v, &facts);
        assert_eq!(
            region.len() as u64,
            v.num("frame_region_len"),
            "{}: the region a frame walk is handed is the wrong size",
            v.name
        );

        let offs = v.per_frame("frame_offsets");
        let frame_len = v.per_frame("frame_bytes");
        let header_bits = v.per_frame("frame_header_bits");
        let blocksizes = v.per_frame("frame_blocksizes");
        let gaps = v.per_frame("frame_footer_gaps_bits");
        let numbers = v.per_frame("frame_numbers");
        let n = v.num("frame_count") as usize;

        // What the stream's own declarations can say about where its audio
        // ends, from the row that says what the DECLARATIONS can say — never
        // from the walk.
        let declared = v.raw("frame_exact_count_from_declarations");
        if declared == "none" {
            assert_eq!(facts.info.exact_frame_count(), None);
        } else {
            assert_eq!(
                declared.parse::<u64>().unwrap() as usize,
                facts.info.exact_frame_count().unwrap(),
                "{}: the table derived the frame count from the stream's \
                 declarations by a different arithmetic than the crate",
                v.name
            );
        }

        let (mut left, mut right) = caller_buffers(&v, &facts);
        let mut walk = scan::Frames::stream(bytes, &facts);
        assert_eq!(walk.region_len(), region.len());
        assert_eq!(walk.cursor(), 0, "{}: a walk starts at 0", v.name);
        assert!(!walk.at_end());

        let mut state: [PredictorState; 2] = Default::default();
        let mut pcm = PcmPin::new();
        for i in 0..n {
            let f = walk
                .next_frame(&mut left, &mut right, &mut state)
                .unwrap_or_else(|e| panic!("{}: frame {} of {n}: {e:?}", v.name, i))
                .unwrap_or_else(|| {
                    panic!(
                        "{}: the table has {n} frames; the walk ended at frame \
                         {i} with {} bytes of the region still unread",
                        v.name,
                        region.len() - walk.cursor()
                    )
                });
            assert_eq!(
                file_offset(&facts, f.offset),
                offs[i],
                "{}: frame {i} begins at {}, the table says {}",
                v.name,
                file_offset(&facts, f.offset),
                offs[i]
            );
            assert_eq!(
                f.bytes as u64, frame_len[i],
                "{}: frame {} is {} bytes by the walk, {} by the table",
                v.name, i, f.bytes, frame_len[i]
            );
            assert_eq!(
                f.header.header_bits as u64, header_bits[i],
                "{}: frame {i}'s header is {} bits, the table says {}",
                v.name, f.header.header_bits, header_bits[i]
            );
            assert_eq!(
                f.samples as u64, blocksizes[i],
                "{}: frame {} carries {} samples, the table says {}",
                v.name, i, f.samples, blocksizes[i]
            );
            assert_eq!(
                f.number, numbers[i],
                "{}: frame {i} is numbered {}, the table says {}",
                v.name, f.number, numbers[i]
            );
            // The frame ends itself: its own header, its own subframe bodies,
            // the padding to the byte boundary, and its 16-bit footer. The three
            // rows add up (see the table's own comment), which is what lets a
            // reader reconstruct the part no row stores — the bodies' bit
            // length — and see that it is positive, i.e. that the frame really
            // contains samples and not just a header and a footer.
            let body_bits = (f.bytes as u64 * 8)
                .checked_sub(header_bits[i] + gaps[i])
                .unwrap_or_else(|| {
                    panic!(
                        "{}: frame {i} is {} bytes = {} bits, which is less \
                         than its own header ({} bits) plus its footer gap \
                         ({} bits)",
                        v.name,
                        f.bytes,
                        f.bytes * 8,
                        header_bits[i],
                        gaps[i]
                    )
                });
            assert!(
                body_bits >= blocksizes[i] * v.channels() as u64,
                "{}: frame {i} holds {} bits of subframe body for {} samples \
                 across {} channel(s)",
                v.name,
                body_bits,
                blocksizes[i],
                v.channels()
            );
            pcm.push(&f, &left, &right);
        }

        // The end. Nothing declared this position: for a stream whose total is
        // known the cross-check below agrees, and
        // `the_unseekable_stream_is_walked_to_the_end_of_its_bytes_and_nothing_else`
        // is the case where no declaration exists to agree with.
        let end = walk
            .next_frame(&mut left, &mut right, &mut state)
            .expect("a complete walk ends cleanly, not with an error");
        assert!(end.is_none(), "{}: the walk did not end", v.name);
        assert!(walk.at_end());
        assert_eq!(
            walk.cursor(),
            region.len(),
            "{}: the walk stopped {} bytes short of the region's end",
            v.name,
            region.len() - walk.cursor()
        );
        let total_samples: usize = blocksizes.iter().map(|b| *b as usize).sum();
        assert_eq!(
            walk.decoded(),
            Stats {
                frames: n,
                samples: total_samples
            },
            "{}: the walk's own tally disagrees with the table",
            v.name
        );

        // The audio itself: `pcm_*` rows are `flac -d`'s output, so this is the
        // whole-file decode of a walk that indexed nothing, pinned against the
        // reference decoder byte for byte.
        if v.has("pcm_fnv1a64") {
            assert_eq!(
                v.num("pcm_interleaved_values"),
                total_samples as u64 * v.channels() as u64,
                "{}: the table's PCM value count does not match its own frame \
                 rows — a frame walk's output has no other definition",
                v.name
            );
            assert_eq!(
                pcm.values,
                total_samples * v.channels(),
                "{}: the walk wrote {} interleaved values for {} frames",
                v.name,
                pcm.values,
                n
            );
            assert_eq!(
                v.num("pcm_bytes_len"),
                pcm.values as u64 * 2,
                "{}: 16-bit PCM is two bytes a value",
                v.name
            );
            assert_eq!(
                pcm.hash,
                v.raw("pcm_fnv1a64").parse().expect("pcm_fnv1a64"),
                "{}: the walk's PCM hashes differently from `flac -d`",
                v.name
            );
        }
    }
}

// ---------------------------------------------------------------- the filter

/// `scan::next_frame_start` is the crate's only place that SEARCHES for a frame,
/// and it is a recovery path, not a way to enumerate a stream. This test asks
/// what its filter is worth when it does: run it over an entire frame region and
/// compare the positions it accepts with the positions the table says frames
/// actually start at.
///
/// The 24-bit fixture is NOT in this loop, and the reason is a fact about this
/// crate rather than about the format: the candidate filter below IS
/// [`frame::FrameHeader::parse`], profile checks included, so at the depth
/// boundary locating frames stops with decoding them. That is worth its own
/// test, not a `continue` hidden in a loop: see
/// `the_depth_boundary_applies_to_locating_frames_as_much_as_to_decoding_them`.
#[test]
fn the_frame_start_filter_finds_exactly_the_frames_a_stream_has() {
    for v in load_vectors() {
        if v.bits_per_sample() == 24 {
            continue;
        }
        let base = v.num("frame_data_offset") as usize;
        let region = &v.fixture()[base..];
        assert_eq!(
            region.len() as u64,
            v.num("frame_region_len"),
            "{}: the frame region is the wrong size",
            v.name
        );
        let defaults = table_defaults(&v);

        // Every header these streams actually emit fits the window the scan is
        // allowed to read, and inside the bounds `frame` derives from the format
        // rather than inside a range copied from what our own packer emits. The
        // 80-bit header belongs to `s3_wide_48001hz_b3000.flac`; 48 bits is the
        // narrowest real header in the table.
        let widths = v.num("frame_count"); // keep the next asserts honest about N
        assert!(widths >= 1);
        for (i, w) in v.per_frame("frame_header_bits").iter().enumerate() {
            assert!(
                *w >= frame::MIN_HEADER_BITS as u64 && *w <= frame::MAX_ACCEPTED_HEADER_BITS as u64,
                "{}: frame {i}'s {}-bit header is outside the bounds the frame \
                 module derives from the format ({}..={})",
                v.name,
                w,
                frame::MIN_HEADER_BITS,
                frame::MAX_ACCEPTED_HEADER_BITS
            );
            assert!(
                (*w as usize) <= frame::MAX_HEADER_BYTES * 8,
                "{}: frame {i}'s header needs {} bytes; the candidate window is \
                 {} — the window would have to be widened, not the format",
                v.name,
                (w + 7) / 8,
                frame::MAX_HEADER_BYTES
            );
        }

        let mut found: Vec<usize> = Vec::new();
        let mut at = 0usize;
        while let Some(p) = scan::next_frame_start(region, at, &defaults) {
            assert!(
                p >= at,
                "{}: a search from {at} returned a start behind itself at {p}",
                v.name
            );
            found.push(p);
            at = p + 1;
        }

        // The claim: our strict filter accepts exactly the real frame starts, in
        // order, and nothing else anywhere in the region.
        let want = v.region_offsets(base);
        assert_eq!(
            found,
            want,
            "{}: the strict filter found {} frame starts; the stream has {} \
             frames at {want:?}",
            v.name,
            found.len(),
            v.num("frame_count")
        );
        assert_eq!(
            found.len() as u64,
            v.num("scan_frame_starts_strict"),
            "{}: the crate's filter and the generator's strict filter disagree",
            v.name
        );

        // The filter ladder, from the Rust side. `scan_frame_starts_fields_only`
        // and `scan_frame_starts_sync_shape_only` count the WIDER tests a player
        // might be tempted to run — field checks without the header's CRC-8, and
        // the bare two-byte sync shape. Our filter is at least as strict as both,
        // so its count can only be smaller. On these five fixtures every leg
        // gives the frame count (the generator asserts that set inclusion before
        // it writes the table), and these two assertions are equalities today;
        // they are written as inequalities because the day a fixture contains a
        // false start, they stop being equalities and this test keeps passing
        // for the right reason. Longer material is where the legs part company:
        // `tests/stream_vectors.txt`'s own header records sync shape
        // over-matching by 1.3x to 12x (204 candidates for 157 frames), which is
        // the measurement that says a player may not treat sync shape as frame
        // location — and `src/scan.rs`'s `Frames` never does.
        assert!(
            (found.len() as u64) <= v.num("scan_frame_starts_fields_only"),
            "{}: {} starts passed the crate's strict filter but the field-only \
             filter found {}, a superset by construction",
            v.name,
            found.len(),
            v.num("scan_frame_starts_fields_only")
        );
        assert!(
            (found.len() as u64) <= v.num("scan_frame_starts_sync_shape_only"),
            "{}: the sync-shape-only filter found {} candidates for {} strict \
             starts",
            v.name,
            v.num("scan_frame_starts_sync_shape_only"),
            found.len()
        );
    }
}

// ------------------------------------------------------- the depth boundary

/// A 24-bit stream is the one class of stream this crate refuses, and the
/// refusal happens in three places at once — which is only defensible if it is
/// the SAME decision. This test pins that: the stream is refused at its entry
/// (item 4a's rule), its frames are refused by a walk handed the region, and the
/// reason the frame-start filter finds none of them is the same rule, not a
/// framing failure.
///
/// The table is the counter-evidence to "framing failed": it describes this
/// stream's frames completely — every length, and they tile the region exactly —
/// and the generator's own strict filter locates every one of them. The frames
/// are walkable; this crate's *profile* declines them. A build that accepted
/// 24-bit samples would not need a new framing layer, and this test is where a
/// reader finds out that the two questions are separate.
#[test]
fn the_depth_boundary_applies_to_locating_frames_as_much_as_to_decoding_them() {
    let vs = load_vectors();
    let wide: Vec<&Vector> = vs.iter().filter(|v| v.bits_per_sample() == 24).collect();
    assert_eq!(
        wide.len(),
        1,
        "the table is expected to carry exactly one stream above this crate's \
         depth; found {} — a second one changes what this test witnesses",
        wide.len()
    );
    let v = wide[0];
    let base = v.num("frame_data_offset") as usize;
    let region = &v.fixture()[base..];

    // Framing, independently: the frames tile the region and are all
    // structurally present. (These are row-vs-row statements, which is what
    // makes them evidence about the stream rather than about this crate.)
    let lens = v.per_frame("frame_bytes");
    assert_eq!(
        lens.iter().sum::<u64>(),
        v.num("frame_region_len"),
        "{}: the frames' own lengths total {}, the region is {}",
        v.name,
        lens.iter().sum::<u64>(),
        v.num("frame_region_len")
    );
    assert_eq!(
        v.num("scan_frame_starts_strict"),
        v.num("frame_count"),
        "{}: the reference filter locates {} starts for {} frames",
        v.name,
        v.num("scan_frame_starts_strict"),
        v.num("frame_count")
    );

    // This crate, at the file level: refused before a single frame is read.
    let e = stream::parse(v.fixture())
        .expect_err("a 24-bit stream is outside this crate's encode profile");
    assert!(
        matches!(e, Error::UnsupportedSampleSize),
        "{}: stream::parse said {e:?}",
        v.name
    );

    // And at the frame level. The frames STATE their depth: §9.1's sample-size
    // code here is 0b110 (24-bit) in every one of them, per the table's
    // `frame_sample_size_codes` row, which generation refuses to write if a
    // frame's statement contradicts its own streaminfo. So no caller-supplied
    // default can talk this crate into reading them — the candidate filter finds
    // none of the ten starts the reference filter finds, and a walk refuses its
    // first frame without consuming it.
    let codes = v.per_frame("frame_sample_size_codes");
    assert!(
        codes.iter().all(|&c| c == 0b110),
        "{}: the frames' sample-size codes are {:?}, not all 0b110 — this test \
         argues that the depth is stated in the header, so it has to be stated",
        v.name,
        codes
    );
    let defaults = table_defaults(v);
    assert_eq!(defaults.bits_per_sample, 24, "from the table's streaminfo");
    assert_eq!(
        scan::next_frame_start(region, 0, &defaults),
        None,
        "{}: this crate's frame-start filter located at least one of the {}          
 frames the reference filter finds",
        v.name,
        v.num("frame_count")
    );

    let mut left = vec![0i32; v.num("streaminfo_maximum_blocksize") as usize];
    let mut right: Vec<i32> = Vec::new();
    let mut state: [PredictorState; 2] = Default::default();
    let mut walk = scan::Frames::region(region, &defaults, scan::StreamBounds::NOTHING);
    let e = walk
        .next_frame(&mut left, &mut right, &mut state)
        .expect_err("a 24-bit frame is not a frame this crate will decode");
    assert!(
        matches!(e, scan::Error::Decode(Error::UnsupportedSampleSize)),
        "{}: the walk said {e:?}",
        v.name
    );
    assert_eq!(
        walk.cursor(),
        0,
        "the refusal must not have consumed anything: the caller still has the whole region to do something else with"
    );
    assert!(
        left.iter().all(|&x| x == 0),
        "a refused frame wrote no sample"
    );

    // And the measured reach of `StreamDefaults`: for depth, these five streams
    // never use it. No frame of any fixture carries code `0b000` ("the depth is
    // only in STREAMINFO"), so across every real encode in this suite the value
    // a profile check sees is the frame's own statement, and a caller's defaults
    // are not in the argument. That is a fact about what libFLAC writes, not
    // about what FLAC allows — code `0b000` is legal, `StreamInfo::as_defaults`
    // exists for it, and both directions of that path are witnessed on real
    // mutated headers in `frame_header_layout.rs`
    // (`mutation_accepts_the_from_stream_depth_code_via_defaults`, which also
    // refuses an out-of-profile DEFAULT). If a future fixture ever does borrow
    // its depth, this assertion stops being an all-true statement over the codes
    // and that other suite becomes the place the mixed case has to be re-read.
    for w in vs.iter() {
        assert!(
            !w.per_frame("frame_sample_size_codes")
                .iter()
                .any(|&c| c == 0b000),
            "{}: a frame states its depth as `0b000` (from streaminfo); the claim this loop witnesses has changed and the depth boundary needs re-reading, not a relaxed assertion",
            w.name
        );
    }
}

// -------------------------------------------------------- the caller's side

/// `Frames` is a pull walk over buffers the caller owns, so the caller needs a
/// rule for how big they have to be — and this crate has two different rules,
/// each from a different source: a stream that DECLARES a maximum block size can
/// be sized for in advance, and the requirement is checked before a single bit
/// is read; a frame whose size only the header knows (a range, a packed blob, a
/// stream that declares nothing) can only be checked after the header parses.
/// Both are witnessed here against real encoder bytes.
///
/// The last part is the one that matters most for a real-time player: the final
/// frame of a track is legitimately short, and a walk that wrote through its
/// own block size would be writing past the audio into whatever the caller's
/// buffer happens to hold.
#[test]
fn the_buffers_a_walk_asks_for_come_from_the_stream_and_from_the_frame() {
    const SENTINEL: i32 = 0x5A5A_5A5A; // outside every supported sample depth
    let vs = load_vectors();

    // (a) A declared maximum, checked before any bit is read.
    for v in vs.iter().filter(|v| v.bits_per_sample() != 24) {
        let facts = stream::parse(v.fixture()).unwrap();
        let declared = v.num("streaminfo_maximum_blocksize") as usize;
        assert_eq!(
            facts.info.max_blocksize as usize, declared,
            "{}: the crate's reading of the declared maximum disagrees with \
             metaflac's row",
            v.name
        );
        let mut left = vec![SENTINEL; declared - 1];
        let mut right = vec![SENTINEL; declared - 1];
        let mut state: [PredictorState; 2] = Default::default();
        let mut walk = scan::Frames::stream(v.fixture(), &facts);
        let e = walk
            .next_frame(&mut left, &mut right, &mut state)
            .expect_err("a buffer one sample short of the declared maximum");
        assert!(
            matches!(e, scan::Error::BufferTooSmall { need, have }
                     if need == declared && have == declared - 1),
            "{}: {e:?}",
            v.name
        );
        assert_eq!(walk.cursor(), 0, "the refusal consumed nothing");
        assert!(
            left.iter().all(|&x| x == SENTINEL) && right.iter().all(|&x| x == SENTINEL),
            "{}: a refused frame wrote into the caller's buffers",
            v.name
        );
    }

    // (b) A stereo frame's SECOND buffer is a per-frame requirement, because
    // how many subframes a frame carries is a property of the frame header, not
    // of the stream's channel count. s2's first frame carries 4608 samples in
    // each of two subframes; `right` empty is the caller's error, and the walk
    // says so with the number that frame asked for.
    let stereo = vs
        .iter()
        .find(|v| v.channels() == 2)
        .expect("the table must contain a stereo stream");
    let facts = stream::parse(stereo.fixture()).unwrap();
    let first_blocksize = stereo.per_frame("frame_blocksizes")[0] as usize;
    let mut left = vec![SENTINEL; first_blocksize];
    let mut right: Vec<i32> = Vec::new();
    let mut state: [PredictorState; 2] = Default::default();
    let mut walk = scan::Frames::stream(stereo.fixture(), &facts);
    let e = walk
        .next_frame(&mut left, &mut right, &mut state)
        .expect_err("a stereo frame needs a second buffer");
    assert!(
        matches!(e, scan::Error::BufferTooSmall { need, have }
                 if need == first_blocksize && have == 0),
        "{}: {e:?}",
        stereo.name
    );
    assert_eq!(walk.cursor(), 0);
    assert!(
        left.iter().all(|&x| x == SENTINEL),
        "{}: the refusal wrote into `left`",
        stereo.name
    );

    // (c) The short final frame writes its own window and nothing beyond it.
    // The buffers are as large as the stream's declared maximum — the size a
    // caller was allowed to prepare in advance — and the walk is run to the end
    // with the same predictor state it would have in a real playback session.
    for v in vs.iter().filter(|v| v.bits_per_sample() != 24) {
        let facts = stream::parse(v.fixture()).unwrap();
        let declared = v.num("streaminfo_maximum_blocksize") as usize;
        let blocksizes = v.per_frame("frame_blocksizes");
        let tail = *blocksizes.last().expect("a frame region has a last frame");
        if tail as usize == declared {
            // A stream with no short tail cannot witness this rule; saying so
            // out loud is better than quietly testing nothing.
            continue;
        }
        let mut left = vec![SENTINEL; declared];
        let mut right = vec![SENTINEL; declared];
        let mut state: [PredictorState; 2] = Default::default();
        let mut walk = scan::Frames::stream(v.fixture(), &facts);
        for i in 0..(v.num("frame_count") as usize - 1) {
            walk.next_frame(&mut left, &mut right, &mut state)
                .unwrap_or_else(|e| panic!("{}: frame {i}: {e:?}", v.name));
        }
        // Erase everything the earlier frames wrote, so what survives is only
        // what the last frame itself put there.
        left.iter_mut().for_each(|x| *x = SENTINEL);
        right.iter_mut().for_each(|x| *x = SENTINEL);
        let last = walk
            .next_frame(&mut left, &mut right, &mut state)
            .expect("the last real frame")
            .expect("the table says this stream has another frame");
        assert_eq!(
            last.samples as u64, tail,
            "{}: the last frame's size",
            v.name
        );
        assert!(
            left[..tail as usize].iter().all(|&x| x != SENTINEL),
            "{}: the tail frame wrote {} samples; {} of them are still the \
             sentinel",
            v.name,
            tail,
            left[..tail as usize]
                .iter()
                .filter(|&&x| x == SENTINEL)
                .count()
        );
        assert!(
            left[tail as usize..].iter().all(|&x| x == SENTINEL),
            "{}: the tail frame wrote past its own {}-sample window into {} \
             samples of buffer it was not asked to touch",
            v.name,
            tail,
            left[tail as usize..]
                .iter()
                .filter(|&&x| x != SENTINEL)
                .count()
        );
        if v.channels() == 2 {
            assert!(
                right[tail as usize..].iter().all(|&x| x == SENTINEL),
                "{}: the tail frame wrote past its window in `right` too",
                v.name
            );
        }
        // …and the walk still ends on the region's last byte.
        assert!(
            walk.next_frame(&mut left, &mut right, &mut state)
                .expect("clean end")
                .is_none()
        );
        assert_eq!(walk.cursor(), v.num("frame_region_len") as usize);
    }
}

// --------------------------------------------------- what a stream can't say

/// One of these five streams was encoded through a pipe, so its STREAMINFO
/// carries the format's "unknown" sentinels: no sample count, no MD5. It is
/// otherwise indistinguishable — same audio, same frames, same bytes from the
/// first frame on — and `flac -t` says it is a valid file.
///
/// That combination is the reason item 4b exists, and it is why a frame walk may
/// not be built around a sample count: for this stream there is no count to
/// build one around. `frame_exact_count_from_declarations none` is the row that
/// says so, and the walk below ends because it ran out of bytes, which is the
/// only end condition every stream has in common.
#[test]
fn an_unseekable_stream_is_walked_to_the_end_of_its_bytes_and_nothing_else() {
    let vs = load_vectors();
    let unknown: Vec<&Vector> = vs
        .iter()
        .filter(|v| v.raw("streaminfo_total_samples") == "0")
        .collect();
    assert_eq!(
        unknown.len(),
        1,
        "the table is expected to carry exactly one stream that declares no \
         sample count; found {}",
        unknown.len()
    );
    let v = unknown[0];
    let facts = stream::parse(v.fixture()).expect("an unseekable stream is a valid stream");

    // What it does not say, in both readings: the crate's, and the row's.
    assert!(!facts.info.total_samples_known());
    assert_eq!(facts.info.total_samples_declared(), None);
    assert_eq!(facts.info.exact_frame_count(), None);
    assert_eq!(v.raw("frame_exact_count_from_declarations"), "none");
    let bounds = scan::StreamBounds::of_stream_info(&facts.info);
    assert_eq!(
        bounds.samples, None,
        "a stream whose total_samples is the sentinel must not put a number in \
         the walk's cross-check"
    );
    assert_eq!(bounds.frames, None);
    // What it still says: a maximum block size, so a caller can still prepare
    // its buffers in advance — the boundary between "unknown" and "unsaid" is
    // per field, not all-or-nothing.
    assert_eq!(
        bounds.max_blocksize,
        Some(v.num("streaminfo_maximum_blocksize") as usize)
    );

    let (mut left, mut right) = caller_buffers(v, &facts);
    let mut state: [PredictorState; 2] = Default::default();
    let mut pcm = PcmPin::new();
    let mut walk = scan::Frames::stream(v.fixture(), &facts);
    let offs = v.region_offsets(facts.frame_data_offset);
    let blocksizes = v.per_frame("frame_blocksizes");
    for (i, off) in offs.iter().enumerate() {
        let f = walk
            .next_frame(&mut left, &mut right, &mut state)
            .unwrap_or_else(|e| panic!("{}: frame {i}: {e:?}", v.name))
            .expect("the table names this frame");
        assert_eq!(f.offset, *off);
        assert_eq!(f.samples as u64, blocksizes[i]);
        pcm.push(&f, &left, &right);
    }
    assert!(
        walk.next_frame(&mut left, &mut right, &mut state)
            .expect("no error: nothing was declared, so nothing could disagree")
            .is_none()
    );
    assert_eq!(
        walk.cursor(),
        v.num("frame_region_len") as usize,
        "the walk stopped somewhere other than the end of the bytes"
    );
    let total: usize = blocksizes.iter().map(|b| *b as usize).sum();
    assert_eq!(walk.decoded().frames, offs.len());
    assert_eq!(
        walk.decoded().samples,
        total,
        "the audio's length came only from counting the frames that were walked"
    );
    assert_eq!(
        pcm.hash,
        v.raw("pcm_fnv1a64").parse().expect("pcm_fnv1a64"),
        "{}: the walk's PCM does not match the reference decoder's",
        v.name
    );

    // The same audio in a container that DOES declare its length: the reference
    // decoder's hash is the same number, which is what lets this test say the
    // walk's end condition was the bytes in both cases rather than the
    // declaration in one and luck in the other.
    let twin = vs
        .iter()
        .find(|w| {
            w.raw("streaminfo_total_samples") != "0" && w.raw("pcm_fnv1a64") == v.raw("pcm_fnv1a64")
        })
        .expect(
            "no second fixture carries the same audio; this test's claim that a \
             declaration and its absence produce the same walk needs the pair",
        );
    assert!(
        v.num("frame_region_len") == twin.num("frame_region_len")
            && v.raw("frame_blocksizes") == twin.raw("frame_blocksizes"),
        "{} and {} no longer carry the same frames",
        v.name,
        twin.name
    );
}

// --------------------------------------------------------------- recovering

/// A walk that loses its frame boundary — a range handed to it that began in the
/// middle of a frame, bytes that arrived damaged, a seek that landed on the
/// wrong byte — has two options: stop, or find where the next real frame begins
/// and continue. This is `Frames::resync`, measured against the positions the
/// table says the frames are at.
///
/// The point of the test is not that recovery is possible but what it costs and
/// what it does not guess: the frame that was in progress is abandoned (its
/// samples are gone), the walk re-anchors its frame numbering on the frame it
/// actually found, and every frame after it is the frame the table describes.
#[test]
fn a_walk_that_lost_its_frame_boundary_recovers_onto_the_next_real_frame() {
    let vs = load_vectors();
    let v = vs
        .iter()
        .find(|v| v.name == "s1_plain_8k_mono")
        .expect("s1_plain_8k_mono is missing from the table");
    let facts = stream::parse(v.fixture()).unwrap();
    let region = frame_region(v, &facts);
    let offs = v.region_offsets(facts.frame_data_offset);
    let numbers = v.per_frame("frame_numbers");
    let blocksizes = v.per_frame("frame_blocksizes");
    let frame_len = v.per_frame("frame_bytes");

    // Enter one byte INSIDE frame 3. A range like this is what a careless seek
    // or a damaged stream actually hands a player.
    let entered_at = offs[3] + 1;
    let slice = &region[entered_at..];
    let mut left = vec![0i32; v.num("streaminfo_maximum_blocksize") as usize];
    let mut right: Vec<i32> = Vec::new();
    let mut state: [PredictorState; 2] = Default::default();

    // A range's bounds are the caller's, and the honest bounds for a hand-cut
    // range are none: nothing here is declared about where this slice's audio
    // begins or ends.
    let mut walk = scan::Frames::region(
        slice,
        &table_defaults(v),
        scan::StreamBounds {
            max_blocksize: Some(v.num("streaminfo_maximum_blocksize") as usize),
            ..scan::StreamBounds::NOTHING
        },
    );
    let e = walk
        .next_frame(&mut left, &mut right, &mut state)
        .expect_err("bytes from the middle of a frame are not a frame header");
    assert!(matches!(e, scan::Error::Decode(Error::FrameSync)), "{e:?}");
    assert_eq!(walk.cursor(), 0, "the refusal left the cursor where it was");

    // Recover. The position found is frame 4's real start, and the walk is left
    // positioned ON that frame, so its number becomes the next one expected: the
    // frame the walk was in the middle of is the one that gets abandoned. An
    // anchor set to the number *after* this frame would reject the very frame
    // resync landed on — which is the bug this test was written around, and why
    // a position assertion and a number assertion have to appear together.
    let found = walk
        .resync()
        .expect("the rest of this region contains real frames");
    assert_eq!(
        found + entered_at,
        offs[4],
        "resync stopped at {found}, which is not the next real frame start"
    );
    assert_eq!(
        walk.expect_number(),
        Some(numbers[4]),
        "after landing on frame {} the walk should expect that frame's own number, not the one after it",
        numbers[4]
    );

    // Finish it. Every remaining frame matches the table's rows for frames
    // 4..N, and the sample total is exactly what those frames carry — no more,
    // because the earlier frames were never in this slice.
    let mut walked = 0usize;
    for i in 4..offs.len() {
        let f = walk
            .next_frame(&mut left, &mut right, &mut state)
            .unwrap_or_else(|e| panic!("frame {i} after recovery: {e:?}"))
            .expect("the table names this frame");
        assert_eq!(f.offset + entered_at, offs[i]);
        assert_eq!(f.number, numbers[i]);
        assert_eq!(f.samples as u64, blocksizes[i]);
        assert_eq!(f.bytes as u64, frame_len[i]);
        walked += 1;
    }
    assert!(
        walk.next_frame(&mut left, &mut right, &mut state)
            .expect("the slice ends cleanly: it holds whole frames from here on")
            .is_none()
    );
    assert_eq!(walk.cursor(), slice.len(), "the walk did not reach the end");
    assert_eq!(walk.decoded().frames, walked);
    let recovered_samples: usize = blocksizes[4..].iter().map(|b| *b as usize).sum();
    assert_eq!(walk.decoded().samples, recovered_samples);
}

// -------------------------------------------------- a stream that stops short

/// A truncated file is the failure a player actually meets — a card read too
/// fast, a download that did not finish, an archive member whose length was
/// rounded down. The walk has to distinguish two things a caller cannot tell
/// apart from the outside: **the region ended in the middle of a frame**, which
/// is a framing fact and is reported as one, and **the region ended on a frame
/// boundary but the stream declared more audio than it holds**, which is a
/// contradiction between the bytes and the file's own claims.
///
/// The second is only reportable when the stream made a claim. A stream that
/// declares no sample count cannot be caught being short — and this test says so
/// explicitly rather than arranging fixtures until the answer is always yes.
#[test]
fn a_truncated_region_says_which_kind_of_short_it_is() {
    let vs = load_vectors();
    let v = vs
        .iter()
        .find(|v| v.name == "s1_plain_8k_mono")
        .expect("s1_plain_8k_mono is missing from the table");
    let facts = stream::parse(v.fixture()).unwrap();
    let region = frame_region(v, &facts);
    let offs = v.region_offsets(facts.frame_data_offset);
    let lens = v.per_frame("frame_bytes");
    let blocksizes = v.per_frame("frame_blocksizes");
    let n = offs.len();

    // (a) Cut in the middle of a frame. Every frame before the cut is walked and
    // matches the rows; the frame the cut lands inside fails *as a frame*, at
    // its own start, and the walk reports how far it got.
    let k = n - 2;
    let cut = offs[k] + (lens[k] / 2) as usize;
    let head = region.get(..cut).expect("a real region has a real prefix");
    let (mut left, mut right) = caller_buffers(v, &facts);
    let mut state: [PredictorState; 2] = Default::default();
    let mut walk = scan::Frames::region(
        head,
        &table_defaults(v),
        scan::StreamBounds {
            max_blocksize: Some(v.num("streaminfo_maximum_blocksize") as usize),
            ..scan::StreamBounds::NOTHING
        },
    );
    for i in 0..k {
        walk.next_frame(&mut left, &mut right, &mut state)
            .unwrap_or_else(|e| panic!("frame {i} before the cut: {e:?}"));
    }
    assert_eq!(walk.decoded().frames, k);
    let e = walk
        .next_frame(&mut left, &mut right, &mut state)
        .expect_err("a frame cut in half is not a frame");
    assert!(
        matches!(e, scan::Error::Decode(_)),
        "a frame whose bytes run past the end of the region must be reported as \
         a frame failure, not as anything else: {e:?}"
    );
    assert_eq!(
        walk.cursor(),
        offs[k],
        "the walk should be sitting on the frame that failed, which is the \
         position a caller can act on (resync, or stop)"
    );
    assert!(
        offs[k] + lens[k] as usize > head.len(),
        "the table says this frame is {} bytes from {}; the region ends at {}; \
         a walk that accepted it would be inventing bytes",
        lens[k],
        offs[k],
        head.len()
    );
    // No silent extra samples: what the walk reports it produced is exactly the
    // frames that were whole.
    let whole: usize = blocksizes[..k].iter().map(|b| *b as usize).sum();
    assert_eq!(walk.decoded().samples, whole);
}

// ------------------------------------------------------- no byte stride fits

/// Nothing in a FLAC frame region is periodic. The table's own rows say so: the
/// frames are different lengths, their starts are not an arithmetic sequence, and
/// the short one is the last — the tail of a track, which is where a fixed-stride
/// reader falls off the end of the audio and into whatever follows it in memory.
///
/// The second half of this test is the one to read if the first looks obvious.
/// A frame's length is derived from its own fields; a reader that instead trusted
/// its derivation would be silently correct on a well-formed file and silently
/// wrong on a damaged one. So one byte is removed from the end of a real frame —
/// a corruption far smaller than any a player has to survive — and the walk's
/// behavior is pinned: it does not shrink the frame to fit, it consumes the frame
/// its own fields describe, and the next thing it touches is one byte into the
/// following frame, where it stops and says so.
#[test]
fn no_byte_stride_describes_a_real_region_and_a_frame_one_byte_short_is_still_a_frame() {
    let vs = load_vectors();

    // The rows themselves: lengths differ, starts are not evenly spaced, and the
    // shortest frame is the last one. (If this ever failed, "walk it by a
    // constant stride" would be a legitimate optimisation for that stream and
    // this whole test would want re-arguing.)
    for v in vs.iter().filter(|v| v.bits_per_sample() != 24) {
        let facts = stream::parse(v.fixture()).unwrap();
        let offs = v.region_offsets(facts.frame_data_offset);
        let lens = v.per_frame("frame_bytes");
        if offs.len() < 2 {
            continue;
        }
        // The frames TILE the region: each frame's own byte length is exactly
        // the distance to the next frame's start, and the last one's length
        // reaches the end of the region. That is what a walk consumes — a
        // per-frame length, not a stride.
        for i in 1..offs.len() {
            assert_eq!(
                offs[i],
                offs[i - 1] + lens[i - 1] as usize,
                "{}: frame {} is {} bytes and frame {} starts at {}; the rows \
                 disagree about where the bytes are",
                v.name,
                i - 1,
                lens[i - 1],
                i,
                offs[i]
            );
        }
        // A constant stride, on the other hand, is not a thing any of these
        // regions has. Some consecutive frames here happen to compress to the
        // same size (s2's first two are both 7823 bytes — the fixtures are
        // deterministic encodes of deterministic sources), so a stride can land
        // on frame starts by luck. What it can never do is end where the region
        // ends: the last frame is always shorter than the first, because it
        // holds the end of the track, and taking any one frame's length as the
        // rule for all of them puts a fixed-stride reader past the last byte.
        assert!(
            *lens.last().unwrap() < lens[0],
            "{}: the last frame is not shorter than the first",
            v.name
        );
        let region_len = v.num("frame_region_len") as usize;
        assert_eq!(
            offs.last().map(|&o| o + *lens.last().unwrap() as usize),
            Some(region_len),
            "{}: the last frame's own length should reach the end of the region",
            v.name
        );
        assert_ne!(
            offs[0] + offs.len() * lens[0] as usize,
            region_len,
            "{}: {} frames at {} bytes each is {} bytes; the region is {}; a \
             fixed-stride walk's idea of where this stream ends is not where it \
             ends",
            v.name,
            offs.len(),
            lens[0],
            offs[0] + offs.len() * lens[0] as usize,
            region_len
        );
    }

    // One byte removed from the end of a real frame.
    let v = vs
        .iter()
        .find(|v| v.name == "s1_plain_8k_mono")
        .expect("s1_plain_8k_mono is missing from the table");
    let facts = stream::parse(v.fixture()).unwrap();
    let region = frame_region(v, &facts);
    let offs = v.region_offsets(facts.frame_data_offset);
    let lens = v.per_frame("frame_bytes");
    let numbers = v.per_frame("frame_numbers");
    let mut misframed: Vec<u8> = Vec::with_capacity(region.len());
    misframed.extend_from_slice(region.get(..lens[0] as usize - 1).expect("a real frame"));
    misframed.extend_from_slice(
        region
            .get(lens[0] as usize..)
            .expect("the rest of the region"),
    );
    assert_eq!(misframed.len() + 1, region.len());

    let (mut left, mut right) = caller_buffers(v, &facts);
    let mut state: [PredictorState; 2] = Default::default();
    // The region is hand-cut, so it goes through the region entry point — with
    // this stream's OWN bounds, to show that the declared-count cross-check is
    // not what catches a misframed byte: the walk refuses before it ever gets to
    // compare numbers.
    let bounds = scan::StreamBounds::of_stream_info(&facts.info);
    assert_eq!(
        bounds.samples,
        Some(v.num("streaminfo_total_samples")),
        "s1 does declare its length; this test needs the check to be live"
    );
    let mut walk = scan::Frames::region(&misframed[..], &table_defaults(v), bounds);
    match walk.next_frame(&mut left, &mut right, &mut state) {
        // A build that verifies footers sees the missing byte immediately: the
        // frame's CRC-16 no longer matches the bytes it covers. That is the
        // right answer, and it is why the footer exists.
        Err(scan::Error::Decode(Error::FrameCrc)) => {}
        // A build without footer verification still knows where the frame ends,
        // because a frame's length comes from its own fields. It accepts frame 0
        // — its fields are intact — and lands **one byte into frame 1**, which
        // is exactly the misalignment the missing byte caused. Refusing to
        // "shrink" the frame to fit is not a limitation: the frame's fields are
        // the record of what it holds, and a reader that guessed shorter would
        // be reading audio that was never encoded.
        Ok(Some(f)) => {
            assert_eq!(f.offset, 0);
            assert_eq!(f.number, numbers[0]);
            assert_eq!(
                f.bytes as u64, lens[0],
                "the walk should have taken the length the frame's own fields \
                 describe, not the length the region happens to offer it"
            );
            // The walk advanced by the 117 bytes frame 0's own fields describe.
            // Deleting a byte ahead of it moved every later frame one byte
            // EARLIER, so the walk is standing one byte inside frame 1 — which
            // is the whole point of a one-byte deletion: the walk's arithmetic
            // was right about the frame it read, and wrong about where the next
            // one is, exactly as a reader's would be.
            assert_eq!(
                walk.cursor(),
                offs[1],
                "the walk should have advanced by frame 0's own {} bytes; frame \
                 1 used to start at {} and now starts at {}",
                lens[0],
                offs[1],
                offs[1] - 1
            );
            let e = walk
                .next_frame(&mut left, &mut right, &mut state)
                .expect_err("one byte into a frame is not a frame header");
            assert!(
                matches!(e, scan::Error::Decode(Error::FrameSync)),
                "the walk should refuse at the misaligned position: {e:?}"
            );
            assert_eq!(
                walk.cursor(),
                offs[1],
                "and it should leave the caller that position to act on"
            );
            assert_eq!(walk.decoded().frames, 1);
            assert_eq!(
                walk.decoded().samples as u64,
                v.per_frame("frame_blocksizes")[0],
                "the samples the walk says it produced are the frames it actually accepted"
            );
        }
        other => panic!(
            "expected either a footer refusal or a one-byte misalignment; got {:?}",
            other.map(|f| f.map(|f| f.offset))
        ),
    }

    // The mistake this test nearly made, kept because it is the one mistake the
    // two entry points let a caller make, and because the symptom is not what a
    // reader would guess. `Frames::stream` takes a whole file and slices to its
    // frame data itself; `Frames::region` takes a slice that already begins at a
    // frame. Hand the first the second's argument and the region is sliced
    // *again*: the walk starts 86 bytes into the bytes the caller meant it to
    // have, finds the middle of a frame there, and refuses — reporting a cursor
    // of 0, which is a correct answer about the region it chose. That is the
    // honest shape of this API: positions mean positions in a region, so a
    // caller that wants file positions takes them from
    // [`crate::stream::StreamFacts::frame_data_offset`], which is what
    // [`file_offset`] does throughout this suite.
    let mut confused = scan::Frames::stream(&misframed[..], &facts);
    let e = confused
        .next_frame(&mut left, &mut right, &mut state)
        .expect_err("a slice handed to the whole-file entry point is sliced again");
    assert!(matches!(e, scan::Error::Decode(Error::FrameSync)), "{e:?}");
    assert_eq!(
        confused.cursor(),
        0,
        "the refusal is at the start of the region `Frames::stream` chose, which \
         is 86 bytes into the slice the caller passed"
    );
    assert_eq!(
        confused.region_len() as u64,
        misframed.len() as u64 - v.num("frame_data_offset"),
        "…and the region it chose is the slice the caller passed minus \
         `frame_data_offset` bytes — visible to a caller that asks, in \
         `region_len`, and here one byte short of the {} bytes a correctly \
         paired call would have been given",
        v.num("frame_region_len")
    );
}

// ----------------------------------------------------- a frame out of order

/// A walk that is following frame numbers has a rule that no decoder elsewhere
/// in this crate has: a frame whose number is not the next one is not accepted —
/// before its samples are decoded, before any sample is written, and before the
/// region's own declarations get a chance to be consulted.
///
/// That ordering is the whole test. A duplicated frame's *audio* is perfectly
/// valid: decoding it would produce sound, and a player that decoded first would
/// play the same two seconds twice and report a success. §9.1.5's numbering is
/// what tells a reader that a stream repeated itself, and item 4b's framing rule
/// (§6: the bytes are the stream) means the walk cannot use "I ran out of
/// declared audio" as an excuse to stop first.
#[test]
fn a_frame_that_is_not_the_next_one_in_the_sequence_is_refused_before_it_writes() {
    let vs = load_vectors();
    let v = vs
        .iter()
        .find(|v| v.name == "s1_plain_8k_mono")
        .expect("s1_plain_8k_mono is missing from the table");
    let facts = stream::parse(v.fixture()).unwrap();
    let region = frame_region(v, &facts);
    let offs = v.region_offsets(facts.frame_data_offset);
    let lens = v.per_frame("frame_bytes");
    let numbers = v.per_frame("frame_numbers");
    let blocksizes = v.per_frame("frame_blocksizes");
    let n = offs.len();

    // A region that is the real frames followed by a copy of the first one. The
    // copy is not garbage: it is the encoder's own bytes, header CRC and frame
    // footer CRC included, so nothing about it is invalid except its position.
    let mut repeated: Vec<u8> = Vec::new();
    repeated.extend_from_slice(region);
    repeated.extend_from_slice(region.get(..lens[0] as usize).expect("a real frame"));

    // The bounds say nothing about this — 10 frames, 1200 samples — and the walk
    // will satisfy all of it and still refuse. That is deliberate: the refusal
    // must not depend on the arithmetic, because for most real streams the
    // arithmetic will be satisfied by a duplicated frame of audio.
    let bounds = scan::StreamBounds::of_stream_info(&facts.info);
    assert_eq!(bounds.frames, Some(n));
    assert_eq!(bounds.samples, Some(v.num("streaminfo_total_samples")));

    let (mut left, mut right) = caller_buffers(v, &facts);
    let mut state: [PredictorState; 2] = Default::default();
    let mut pcm = PcmPin::new();
    let mut walk = scan::Frames::region(&repeated, &table_defaults(v), bounds);
    for i in 0..n {
        let f = walk
            .next_frame(&mut left, &mut right, &mut state)
            .unwrap_or_else(|e| panic!("frame {i}: {e:?}"))
            .expect("the table names this frame");
        assert_eq!(f.number, numbers[i]);
        assert_eq!(f.samples as u64, blocksizes[i]);
        pcm.push(&f, &left, &right);
    }
    // Snapshot the caller's buffers and the walk's counts, so the refusal below
    // can be shown to have touched neither.
    let left_at_refusal = left.clone();
    let right_at_refusal = right.clone();
    let stats = walk.decoded();
    assert_eq!(stats.frames, n);
    assert_eq!(
        stats.samples,
        blocksizes.iter().map(|b| *b as usize).sum::<usize>()
    );

    let e = walk
        .next_frame(&mut left, &mut right, &mut state)
        .expect_err("a repeat of frame 0 is not frame 10");
    match e {
        scan::Error::FrameOutOfSequence {
            offset,
            expected,
            found,
        } => {
            assert_eq!(offset, offs[n - 1] + lens[n - 1] as usize);
            assert_eq!(expected, numbers[n - 1] + 1);
            assert_eq!(found, numbers[0]);
        }
        other => panic!(
            "expected a sequence refusal with the three numbers a \
             caller needs to say what happened; got {other:?}"
        ),
    }
    assert_eq!(
        walk.cursor(),
        offs[n - 1] + lens[n - 1] as usize,
        "the walk should still be sitting between the last accepted frame and \
         the one it refused"
    );
    assert_eq!(walk.decoded(), stats, "a refused frame changed no counts");
    assert_eq!(left, left_at_refusal, "…and wrote no samples");
    assert_eq!(right, right_at_refusal, "…in either buffer");

    // The refused frame is a real frame: `next_frame_start` finds it, because
    // the candidate filter is about whether the bytes are a frame header, which
    // they are. Sequence is the walk's business, not the filter's.
    assert_eq!(
        scan::next_frame_start(&repeated, walk.cursor(), &table_defaults(v)),
        Some(walk.cursor()),
        "the refused frame should be a locatable frame start"
    );
}

// ------------------------------------------------------------- the end, twice

/// What happens at the end of a region is where item 4b's two rules meet, and
/// the three cases below are different enough that they need naming separately.
///
/// 1. **Trailing bytes after the last frame.** Not a shorter stream: a region
///    whose bytes do not end where the frames end. The walk refuses the leftover
///    bytes as a frame, at a cursor that names them, and the declared-count
///    check never runs — which is the right order. A file that lies about its
///    length is a framing problem, and a player that found out about it from an
///    arithmetic mismatch would have been reading frames it shouldn't trust.
/// 2. **A region that ends on a frame boundary but holds less audio than the
///    file declared.** The framing is perfect; the *file* is short. Only the
///    stream's own declaration can say so, and `finish` says it with the
///    numbers: what was declared, what was decoded, how many frames it took, and
///    how long the region was.
/// 3. **The same truncation, on the stream that declares nothing.** The walk
///    ends cleanly, and the answer is "fewer samples than you expected". Nothing
///    hid that failure and nothing invented a check: the format gives an
///    unseekable stream no way to know it is short, and item 4b's job is to
///    report what the bytes support, not to guess what the caller meant.
#[test]
fn at_the_end_of_a_region_the_walk_reports_what_the_bytes_and_claims_support() {
    let vs = load_vectors();

    // ---- 1. trailing bytes ------------------------------------------------
    let v = vs
        .iter()
        .find(|v| v.name == "s1_plain_8k_mono")
        .expect("s1_plain_8k_mono is missing from the table");
    let facts = stream::parse(v.fixture()).unwrap();
    let region = frame_region(v, &facts);
    let offs = v.region_offsets(facts.frame_data_offset);
    let lens = v.per_frame("frame_bytes");
    let n = offs.len();
    let mut padded: Vec<u8> = Vec::new();
    padded.extend_from_slice(region);
    padded.extend_from_slice(&[0u8; 7]); // not a frame header, not enough bytes for one

    let (mut left, mut right) = caller_buffers(v, &facts);
    let mut state: [PredictorState; 2] = Default::default();
    let mut walk = scan::Frames::region(
        &padded,
        &table_defaults(v),
        scan::StreamBounds::of_stream_info(&facts.info),
    );
    for i in 0..n {
        walk.next_frame(&mut left, &mut right, &mut state)
            .unwrap_or_else(|e| panic!("frame {i}: {e:?}"));
    }
    assert_eq!(walk.cursor() as u64, v.num("frame_region_len"));
    let e = walk
        .next_frame(&mut left, &mut right, &mut state)
        .expect_err("seven bytes that are not a frame are not the end of a stream");
    assert!(
        matches!(e, scan::Error::Decode(Error::FrameSync)),
        "the leftover bytes should be refused as a frame: {e:?}"
    );
    assert_eq!(
        walk.cursor() as u64,
        v.num("frame_region_len"),
        "the refusal should name the first byte that is not part of a frame"
    );
    assert_eq!(
        walk.region_len() as u64,
        v.num("frame_region_len") + 7,
        "…and the region is longer than the frames it holds"
    );
    assert_eq!(
        walk.decoded().samples as u64,
        v.num("streaminfo_total_samples"),
        "the audio the walk got matches the declaration exactly — the \
         declaration had nothing to complain about, and did not get the chance"
    );

    // ---- 2. a region that ends cleanly, holding less than was declared -----
    let short = region
        .get(..offs[n - 1])
        .expect("a real prefix of a region");
    let mut walk = scan::Frames::region(
        short,
        &table_defaults(v),
        scan::StreamBounds::of_stream_info(&facts.info),
    );
    for i in 0..(n - 1) {
        walk.next_frame(&mut left, &mut right, &mut state)
            .unwrap_or_else(|e| panic!("frame {i}: {e:?}"));
    }
    assert!(
        walk.at_end(),
        "a region cut at a frame boundary should leave the walk exactly at its end"
    );
    let e = walk
        .next_frame(&mut left, &mut right, &mut state)
        .expect_err("the file declared more audio than this region holds");
    match e {
        scan::Error::DeclaredSampleCountMismatch {
            declared,
            decoded,
            frames,
            region_len,
        } => {
            assert_eq!(declared, v.num("streaminfo_total_samples"));
            assert_eq!(
                decoded,
                lens[..]
                    .iter()
                    .zip(v.per_frame("frame_blocksizes").iter())
                    .take(n - 1)
                    .map(|(_, b)| *b as usize)
                    .sum::<usize>()
            );
            assert_eq!(frames, n - 1);
            assert_eq!(region_len, offs[n - 1]);
        }
        other => panic!("expected a declared-count mismatch carrying its numbers; got {other:?}"),
    }

    // ---- 3. the same cut, on the stream that declares nothing -------------
    let u = vs
        .iter()
        .find(|x| x.raw("streaminfo_total_samples") == "0")
        .expect("an unseekable fixture is required for this case");
    let ufacts = stream::parse(u.fixture()).unwrap();
    let uregion = frame_region(u, &ufacts);
    let uoffs = u.region_offsets(ufacts.frame_data_offset);
    let ublocks = u.per_frame("frame_blocksizes");
    let un = uoffs.len();
    // The table's own statement about this container: nothing in it says how
    // many frames its audio holds. That is what makes case 3 unreportable, and
    // it is written down here rather than left to inference.
    assert_eq!(u.raw("frame_exact_count_from_declarations"), "none");
    let ushort = uregion
        .get(..uoffs[un - 1])
        .expect("a real prefix of a region");
    let (mut ul, mut ur) = caller_buffers(u, &ufacts);
    let mut state: [PredictorState; 2] = Default::default();
    let mut walk = scan::Frames::region(
        ushort,
        &table_defaults(u),
        scan::StreamBounds::of_stream_info(&ufacts.info),
    );
    for i in 0..(un - 1) {
        walk.next_frame(&mut ul, &mut ur, &mut state)
            .unwrap_or_else(|e| panic!("{}: frame {i}: {e:?}", u.name));
    }
    assert!(
        walk.next_frame(&mut ul, &mut ur, &mut state)
            .expect("no error is possible here: this stream declares nothing")
            .is_none()
    );
    assert_eq!(walk.cursor(), ushort.len());
    let kept: u64 = ublocks.iter().take(un - 1).sum();
    let lost = *ublocks.last().expect("a last frame");
    assert_eq!(walk.decoded().samples as u64, kept);
    assert_eq!(walk.decoded().frames, un - 1);
    // No contradiction was reported, because there was nothing to contradict.
    // The walk's answer is "{} samples, ending cleanly", and the frame it lost
    // ({}) is invisible to a container that says nothing about its own length.
    // A decoder that invented a check here would be reporting the caller's
    // expectation rather than the bytes; a player that needs to know whether a
    // stream is complete asks for a seekable encode, which is exactly why this
    // fixture exists next to its seekable twin rather than replacing it.
    eprintln!(
        "{}: an unseekable stream cut at a frame boundary is indistinguishable \
         from a complete one — {} samples in {} frames, clean end, {} samples' \
         worth of audio actually missing",
        u.name,
        kept,
        un - 1,
        lost
    );
}

/// Where a footer mistake would land, asked of the crate's own filter.
///
/// §12 closes a frame with zero-padding to a byte boundary and *then* the
/// CRC-16, so a walk that treated the footer as exactly 16 bits is short by the
/// padding on every frame the table counts in `footer_padding_frames` — 23 of
/// the 35 frames in these five fixtures. The table records what such a reader
/// would then find standing where it stands, two ways: at its **bit** cursor's
/// position (short by the padding) and at its **byte** cursor's position if it
/// rounded a non-byte-aligned end *down* instead of up (`end - 1`). Both rows
/// are here because the answers decide whether a framing error is refused or
/// followed: the strict filter accepts none of those positions, so a
/// mis-counted footer stops a walk rather than sending it into a phantom frame —
/// and the `*_sync_shape` rows say whether the two-byte pattern matches there,
/// which is the case a player that trusted its own eyes would have taken.
///
/// The bit-level questions can only be asked by the generator: this crate's
/// filter takes byte offsets, and a position 1..7 bits into a byte is not one of
/// them. So the bit-level rows are asserted here for *consistency* — they are
/// zero, and the number of positions they describe is the number of frames whose
/// footer tail is longer than 16 bits — and the byte-level question is asked of
/// `scan` directly, which is the cross-implementation half: the count of
/// positions where the table says a strict header parse succeeds has to equal the
/// count this crate's filter actually accepts.
#[test]
fn a_miscounted_footer_lands_where_the_frame_filter_refuses_it() {
    let vectors = load_vectors();
    let mut shape_matching_but_field_refusing = 0usize;

    for v in &vectors {
        let base = v.num("frame_data_offset") as usize;
        let region = &v.fixture()[base..];
        let offs = v.region_offsets(base);
        let lens: Vec<usize> = v
            .per_frame("frame_bytes")
            .iter()
            .map(|b| *b as usize)
            .collect();
        let gaps = v.per_frame("frame_footer_gaps_bits");
        let n = offs.len();

        // The table against itself: the padding count is the count of frames
        // whose footer tail is longer than the CRC alone, and a byte-truncation
        // position only exists where a following frame does.
        let padded = gaps.iter().filter(|g| **g != 16).count() as u64;
        assert_eq!(
            padded,
            v.num("footer_padding_frames"),
            "{}: {padded} frames have padding after their last body bit, the \
             table counts {} — one of the two is wrong",
            v.name,
            v.num("footer_padding_frames")
        );
        assert_eq!(
            (n - 1) as u64,
            v.num("footer_byte_truncation_positions"),
            "{}: {} byte-truncation positions counted, {} exist",
            v.name,
            v.num("footer_byte_truncation_positions"),
            n - 1
        );
        assert!(
            v.num("footer_slip_positions_sync_shape") <= padded,
            "{}: the sync-shape count ({}) cannot exceed the number of \
             positions it is counted over ({padded})",
            v.name,
            v.num("footer_slip_positions_sync_shape")
        );
        assert_eq!(
            v.num("footer_slip_positions_strict_headers"),
            0,
            "{}: the strict reading found a frame header where a \
             constant-16-bit-footer walk would stand — the prose in `scan.rs`, \
             `lib.rs` and FLAC.md that says these positions are refused is \
             false for this fixture set and has to be re-read, not relaxed",
            v.name
        );
        if v.num("footer_slip_positions_sync_shape") > 0 {
            shape_matching_but_field_refusing += v.num("footer_slip_positions_sync_shape") as usize;
        }

        // Where our own filter stands at the byte-truncation position: it must
        // refuse that position and report the real frame's start. For a
        // 24-bit-encoded fixture our filter locates nothing, so the same
        // question is asked at the depth-boundary test instead; here the rows
        // above are still asserted, which is why the skip is below them.
        if v.bits_per_sample() == 24 {
            continue;
        }
        let defaults = table_defaults(v);
        let mut accepted_by_us = 0usize;
        for i in 0..n - 1 {
            let end = offs[i] + lens[i];
            assert_eq!(end, offs[i + 1], "{}: the region does not tile", v.name);
            let probe = end - 1;
            match scan::next_frame_start(region, probe, &defaults) {
                Some(p) if p == probe => accepted_by_us += 1,
                Some(p) => assert_eq!(
                    p,
                    offs[i + 1],
                    "{}: from one byte short of frame {i}'s end the search \
                     found a start at {p}, which is neither that position nor \
                     the next frame's ({})",
                    v.name,
                    offs[i + 1]
                ),
                None => panic!(
                    "{}: from {probe} the search found nothing, though frame \
                     {} starts at {}",
                    v.name,
                    i + 1,
                    offs[i + 1]
                ),
            }
        }
        assert_eq!(
            accepted_by_us as u64,
            v.num("footer_byte_truncation_strict_headers"),
            "{}: this crate's filter accepted {} of those positions, the \
             generator's strict reading accepted {} — two readings of the same \
             bytes, and they disagree",
            v.name,
            accepted_by_us,
            v.num("footer_byte_truncation_strict_headers")
        );
    }

    // A position whose two bytes look like a frame sync and whose fields do not
    // is the reason the filter has a field leg rather than only a CRC: at those
    // positions a sync-trusting walk invents a frame and a field-checking one
    // refuses. One exists in this fixture set (in the 24-bit fixture's frame
    // region) — the row says where, and this assertion is the part that goes
    // stale rather than silent if a future fixture set stops containing one.
    assert!(
        shape_matching_but_field_refusing >= 1,
        "no position in the current fixture set is sync-shaped but \
         field-refusing; the argument in `scan.rs`'s module docs about the \
         field leg of the filter is no longer witnessed by these files"
    );
}

/// The frame-count claim is checked, and it is checked **after** the sample-count
/// claim. Two things are pinned here, and neither is what the end-of-stream rule
/// looks like from its description.
///
/// First, the arm exists and fires. It was written in the same commit as its
/// sibling and described in the same sentence of the module docs, and deleting it
/// changed nothing — 137 unit tests and 11 integration tests stayed green, which
/// is how I found out it had no witness. The format makes that arm hard to reach
/// on self-consistent bytes: a stream that declares one block size and a sample
/// count has derived both numbers from the same arithmetic, so a region that
/// matches its samples matches its frames. The arm fires when the **claim** and
/// the **region** disagree, which is a caller's mistake — bounds parsed from one
/// stream handed to a region taken from another, a packed blob whose header does
/// not belong to it — and that is a case only a test can stage.
///
/// Second, the order. When both claims are contradicted, the caller is told about
/// the sample count, not the frame count: it is the claim with numbers a caller
/// can act on (how much audio is missing), and it is the one the format itself
/// states. A caller that expects the frame diagnosis will be looking at the wrong
/// field, so the precedence is asserted rather than left to the source order.
#[test]
fn the_frame_count_claim_is_checked_after_the_sample_count_claim() {
    let vs = load_vectors();
    let mut staged = 0usize;

    for v in &vs {
        // Explicit, not hidden. `s4` is a valid 24-bit encode and `stream::parse`
        // refuses it at the capability boundary, so there is no walk of it whose
        // end could be staged; the refusal is asserted here rather than skipped,
        // and witnessed in full by `stream_metadata_layout` and the
        // depth-boundary test in this file.
        let facts = match stream::parse(v.fixture()) {
            Ok(facts) => facts,
            Err(e) => {
                assert_eq!(
                    v.bits_per_sample(),
                    24,
                    "{}: a fixture that does not parse is not explained by its                      depth — something else changed",
                    v.name
                );
                assert!(
                    matches!(e, Error::UnsupportedSampleSize),
                    "{}: the parse refusal should be the depth gate, got {e:?}",
                    v.name
                );
                continue;
            }
        };
        let decodeable = v.bits_per_sample() != 24;
        let real = scan::StreamBounds::of_stream_info(&facts.info);

        // What the bounds layer does with 4a's arithmetic, in both arms: a claim
        // where the stream states enough, and `None` where it does not. The row
        // is the generator's own statement (`ceil(total / fixed blocksize)` where
        // the stream declares both, proven against the real frame count).
        let row = v.raw("frame_exact_count_from_declarations");
        if row == "none" {
            assert_eq!(
                real.frames, None,
                "{}: the stream states too little to derive a frame count, and \
                 the bounds layer invented one anyway",
                v.name
            );
            assert_eq!(
                real.samples,
                facts.info.total_samples_declared(),
                "{}: the sample claim should be exactly what the stream states",
                v.name
            );
            continue;
        }
        let derived: usize = row
            .parse()
            .expect("a numeric frame-count row must be a number");
        assert_eq!(
            derived,
            v.num("frame_count") as usize,
            "{}: the derived frame count is not the real one — the generator \
             claims it proves this row",
            v.name
        );
        assert_eq!(
            real.frames,
            Some(derived),
            "{}: 4a derived {} frames and this layer dropped the claim",
            v.name,
            derived
        );
        if !decodeable {
            continue;
        }
        staged += 1;

        let base = facts.frame_data_offset as usize;
        let region = &v.fixture()[base..];
        let offs = v.region_offsets(base);
        let lens = v.per_frame("frame_bytes");
        let blocks = v.per_frame("frame_blocksizes");
        let n = offs.len();
        let declared_total = v.num("streaminfo_total_samples");
        let total_samples: u64 = blocks.iter().sum();
        assert_eq!(
            total_samples, declared_total,
            "{}: the frame block sizes do not sum to the declared sample count",
            v.name
        );

        // Control: with the stream's own claims, these bytes end cleanly. Without
        // this there is no way to say the errors below came from the claim rather
        // than from the region.
        let (mut left, mut right) = caller_buffers(v, &facts);
        let mut state: [PredictorState; 2] = Default::default();
        let mut walk = scan::Frames::region(region, &table_defaults(v), real);
        for i in 0..n {
            walk.next_frame(&mut left, &mut right, &mut state)
                .unwrap_or_else(|e| panic!("{}: frame {i}: {e:?}", v.name));
        }
        assert!(
            matches!(walk.next_frame(&mut left, &mut right, &mut state), Ok(None)),
            "{}: with its own claims the walk should end without complaint",
            v.name
        );
        assert_eq!(walk.decoded().samples as u64, declared_total);

        // The arm, staged: the same bytes, the same sample claim (and it is
        // satisfied), and a frame claim one short — what `streaminfo` says when it
        // belongs to a stream that is not quite this one. The walk decodes all
        // n frames, the sample check passes, and the frame claim is what fails.
        let claimed = derived - 1;
        let wrong_frames = scan::StreamBounds {
            frames: Some(claimed),
            ..real
        };
        let mut walk = scan::Frames::region(region, &table_defaults(v), wrong_frames);
        for i in 0..n {
            walk.next_frame(&mut left, &mut right, &mut state)
                .unwrap_or_else(|e| panic!("{}: frame {i}: {e:?}", v.name));
        }
        assert_eq!(
            walk.decoded().samples as u64,
            declared_total,
            "{}: the sample claim has to be satisfied for this to be the frame \
             arm and not the sample arm",
            v.name
        );
        let e = walk
            .next_frame(&mut left, &mut right, &mut state)
            .expect_err("a frame claim the region contradicts must not end quietly");
        match e {
            scan::Error::DeclaredFrameCountMismatch {
                declared,
                decoded,
                region_len,
            } => {
                assert_eq!(
                    (declared, decoded, region_len),
                    (claimed, n, v.num("frame_region_len") as usize),
                    "{}: the frame-count diagnosis should carry the claim, the \
                     count the region supports, and the region's length",
                    v.name
                );
            }
            other => panic!("{}: expected the frame-count arm, got {other:?}", v.name),
        }

        // The order: cut the region at a frame boundary and hand it the stream's
        // own claims, both of which are now contradicted — {} samples short of
        // the declared total, and one frame short of the derived count. The
        // caller gets the sample diagnosis, because that is the one that names how
        // much audio is missing.
        let short = &region[..offs[n - 1]];
        let kept: u64 = blocks.iter().take(n - 1).sum();
        let mut walk = scan::Frames::region(short, &table_defaults(v), real);
        for i in 0..(n - 1) {
            walk.next_frame(&mut left, &mut right, &mut state)
                .unwrap_or_else(|e| panic!("{}: frame {i}: {e:?}", v.name));
        }
        let e = walk
            .next_frame(&mut left, &mut right, &mut state)
            .expect_err("both claims are contradicted here");
        match e {
            scan::Error::DeclaredSampleCountMismatch {
                declared,
                decoded,
                frames,
                region_len,
            } => {
                assert_eq!(
                    (declared, decoded as u64, frames, region_len),
                    (declared_total, kept, n - 1, offs[n - 1]),
                    "{}: the sample diagnosis should carry its own numbers",
                    v.name
                );
                assert_ne!(
                    frames, derived,
                    "{}: the frame claim ({derived}) was contradicted too — the \
                     sample arm answers first, which is the precedence this test \
                     exists to pin",
                    v.name
                );
            }
            other => panic!(
                "{}: expected the sample-count arm to answer first, got {other:?}",
                v.name
            ),
        }
    }

    assert!(
        staged >= 1,
        "no fixture in the table states enough to derive a frame count; the \
         claim-staging half of this test is no longer exercised and the arm it \
         witnesses would go untested again"
    );
}
