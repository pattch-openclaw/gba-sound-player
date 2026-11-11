//! Split-ladder item 4a — the stream layer (RFC 9639 §6, §8.1, §8.2) read out
//! of real `libFLAC` files.
//!
//! Table: `tests/stream_vectors.txt` (regenerate:
//! `scripts/gen_stream_vectors.sh`). Six committed fixtures under
//! `tests/fixtures/`, each byte-for-byte what `flac` 1.5.0 wrote. Every
//! expectation in that table comes from `metaflac --list` — the reference
//! tool's own block list and field values — cross-checked at generation time
//! against an independent bit-level reading of §8 in Python, and the frame rows
//! come from the strict frame filter in `scripts/frame_vectors.py`. **None of it
//! comes from the code under test here.** That is the point: this suite is the
//! third independent reading of the same spec, and the two others were written
//! before it and are pinned to each other.
//!
//! What each fixture was chosen to witness, and where the assertion lives:
//!
//!   * `s1_plain_8k_mono` — the two-block container (`STREAMINFO` first, a real
//!     empty `VORBIS_COMMENT` carrying the last-block flag): the minimal ordinary
//!     case, and the case where `exact_frame_count()` is computable and agrees
//!     with the table's `frame_count`;
//!   * `s2_seekable_96k_stereo` — four blocks (`STREAMINFO`, `SEEKTABLE`,
//!     `VORBIS_COMMENT`, `PADDING`), every real block type the encoder emits: a
//!     block walk that must skip by length rather than interpret, and a
//!     `frame_data_offset` 176 bytes in. Its table is the smallest real §8.5 one
//!     there is — a single point, for sample 0, with a stream offset of 0 — which
//!     is what makes the base of those offsets visible: the point carries 0 while
//!     the frame it names is 176 bytes into the file;
//!   * `s3_wide_48001hz_b3000` — a real 80-bit frame header: the first frame of
//!     a real stream whose width is outside this packer's 48..=72-bit profile,
//!     which is why `frame`'s old `debug_assert!((48..=72))` had to become
//!     derived bounds (`MIN_HEADER_BITS`/`MAX_ACCEPTED_HEADER_BITS`);
//!   * `s4_24bit_8k_mono` — a valid 24-bit stream the reference decoder
//!     verifies: the capability boundary sits at a named variant
//!     (`UnsupportedSampleSize`) while the container itself still walks fine;
//!   * `s5_unseekable_total_unknown` — the same audio fed as raw PCM through a
//!     pipe: `total_samples`, both frame sizes and the MD5 are all "not known",
//!     and the frame count is still 10. Item 4b's end-of-stream rule cannot be
//!     "count frames from `total_samples`", and this is the fixture that says so;
//!   * `s6_seekpoints_8k_mono` — the multi-point §8.5 table `metaflac
//!     --add-seekpoint` writes: eight entries, seven real and one placeholder,
//!     requested out of order and sorted by the tool, one request snapped down to
//!     the containing frame boundary, one beyond the end dropped, and one point
//!     naming the **short last frame** of a fixed-blocksize stream (368 samples
//!     where `max_blocksize` says 512). A seek table with one point tests the
//!     container; a table whose points land on the middle and the end of the
//!     audio is the only thing in this repo that can say whether a reader found
//!     the right *frame* — see
//!     `every_seek_table_is_read_as_the_tools_and_the_frame_walk_render_it`
//!     here, and
//!     `a_seek_point_starts_a_walk_that_finishes_the_stream_at_the_right_sample`
//!     in item 4b's suite, where the same points drive real walks.
//!
//! The module-level unit tests in `src/stream.rs` cover the *rules* no encoder
//! here produces (a chain that never sets the last flag, a duplicate
//! `STREAMINFO`, type 127, a 120-bit synthetic header, and the seek-table shapes
//! the reference tools accept and this crate declines: a second table, a
//! placeholder among the real points, points out of order). This file covers the
//! bytes a real encoder does produce. Both are needed: one is a rule the format
//! states, the other is a rule the format actually ships.
//!
//! Two of the rows in the table are not readings of the container at all, and
//! one test each exists to pin them (`the_frame_number_form_and_scan_censuses_
//! are_pinned_on_every_fixture`, `a_frame_number_in_a_longer_form_is_refused_
//! here_and_accepted_there`): the frame-number **form** censuses
//! (`frame_number_forms_*`, `scan_frame_starts_strict`,
//! `scan_frame_starts_all_forms_allowed`), which measure what refusing a
//! non-minimal coded number costs on real bytes; and `mutation*`, one of `s1`'s
//! own headers re-spelled with both CRCs recomputed, which *this* crate refuses
//! and `flac -d` accepts. The reason for that divergence is in
//! `bits::read_utf8_coded`'s docs; the numbers for it are here.

use std::collections::BTreeMap;

use flac_lite::Error;
use flac_lite::bits::{BitReader, crc8};
use flac_lite::format::ChannelConfig;
use flac_lite::frame::{FrameHeader, MAX_ACCEPTED_HEADER_BITS, MIN_HEADER_BITS, StreamDefaults};
use flac_lite::stream::{self, Blocks, MAGIC, TYPE_STREAMINFO};

// ---------------------------------------------------------------------------
// The fixtures, embedded. The table names each file; an entry has to exist here
// for it to be read at all, so a sixth fixture fails this suite rather than
// being silently skipped — the fail-closed rule `gen_stream_vectors.sh` applies
// to its own expectations, applied to the harness side.
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
        "s6_seekpoints_8k_mono.flac" => include_bytes!("fixtures/s6_seekpoints_8k_mono.flac"),
        other => panic!(
            "the table names a fixture this test does not embed: {other}\n\
             add it to fixture_bytes() (and, if it witnesses a new rule, add an \
             assertion) rather than dropping it from the table."
        ),
    }
}

/// FNV-1a 64, mirroring `scripts/frame_vectors.py: fnv1a64` (and the spike
/// crate's Rust copy). Two independent implementations of one simple hash
/// meeting on committed bytes: if a fixture is edited, re-encoded by a new
/// libFLAC, or truncated by a checkout, this pin says so before any field
/// assertion silently starts reading different bytes.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xCBF4_3CE9_5DE6_84B7;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

// ---------------------------------------------------------------------------
// Table reading (host-side test code; std is fine here — nothing in here is
// compiled into a GBA artifact).
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Block {
    number: usize,
    block_type: u8,
    type_name: String,
    is_last: bool,
    length: usize,
}

#[derive(Debug)]
struct Vector {
    label: String,
    rows: BTreeMap<String, String>,
    blocks: Vec<Block>,
    widths: Vec<usize>,
}

impl Vector {
    fn raw(&self, key: &str) -> &str {
        self.rows
            .get(key)
            .unwrap_or_else(|| panic!("{}: table has no {key:?} row", self.label))
            .as_str()
    }
    fn num(&self, key: &str) -> u64 {
        self.raw(key).parse().unwrap_or_else(|e| {
            panic!(
                "{}: {key:?} = {:?} is not a number: {e}",
                self.label,
                self.raw(key)
            )
        })
    }
    fn md5(&self) -> [u8; 16] {
        let text = self.raw("streaminfo_md5");
        let mut out = [0u8; 16];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)
                .unwrap_or_else(|e| panic!("{}: bad md5 row {text:?}: {e}", self.label));
        }
        out
    }
    fn fixture(&self) -> &'static [u8] {
        fixture_bytes(self.raw("fixture"))
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
            out.push(Vector {
                label: rest.trim().to_string(),
                rows: BTreeMap::new(),
                blocks: Vec::new(),
                widths: Vec::new(),
            });
            continue;
        }
        let v = out.last_mut().unwrap_or_else(|| {
            panic!("line {}: {key:?} outside a stream_vector block", lineno + 1)
        });
        let rest = rest.trim();
        if key == "block" {
            // `block N type T(NAME) last <bool> length <n>`
            let t: Vec<&str> = rest.split_whitespace().collect();
            let parse_shape = |t: &[&str]| -> Option<Block> {
                let number = t.get(0)?.parse().ok()?;
                if t.get(1) != Some(&"type") || t.len() != 7 || t[3] != "last" || t[5] != "length" {
                    return None;
                }
                let (code, name) = t[2].split_once('(')?;
                Some(Block {
                    number,
                    block_type: code.parse().ok()?,
                    type_name: name.strip_suffix(')')?.to_string(),
                    is_last: match t[4] {
                        "true" => true,
                        "false" => false,
                        _ => return None,
                    },
                    length: t[6].parse().ok()?,
                })
            };
            v.blocks.push(
                parse_shape(&t).unwrap_or_else(|| {
                    panic!("line {}: cannot read block row {rest:?}", lineno + 1)
                }),
            );
            continue;
        }
        if key == "frame_header_widths_bits" {
            v.widths = rest
                .split_whitespace()
                .map(|w| w.parse().expect("non-numeric header width"))
                .collect();
            continue;
        }
        if v.rows.insert(key.to_string(), rest.to_string()).is_some() {
            panic!("line {}: duplicate {key:?} row", lineno + 1);
        }
    }
    out
}

/// The table, the fixtures, and the crate's own parser — the three things this
/// suite is about, in one place.
///
/// Panics if `parse` refuses a fixture it should read. It should not read
/// exactly one: the fixture whose declared bit depth is a capability this
/// decoder does not have (`s4`), which is why every test that walks *all* the
/// vectors asks the table which fixture that is first — a blanket skip would
/// hide a real regression just as well as a missing fixture would.
fn parsed(v: &Vector) -> stream::StreamFacts {
    stream::parse(v.fixture()).unwrap_or_else(|e| {
        panic!(
            "{}: stream::parse refused a fixture it should read: {e:?}",
            v.label
        )
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The channel-assignment reading the crate reaches, rendered the way
/// `frame_vectors.py` labels it (subframe count + decorrelation name), so a
/// table row and a `ChannelConfig` can be compared without either side being
/// re-derived by hand.
fn channel_shape(channels: ChannelConfig) -> (usize, String) {
    match channels {
        ChannelConfig::Independent { channels } => (usize::from(channels), "none".to_string()),
        ChannelConfig::LeftSide => (2, "left-side".to_string()),
        ChannelConfig::RightSide => (2, "side-right".to_string()),
        ChannelConfig::MidSide => (2, "mid-side".to_string()),
    }
}

// ---------------------------------------------------------------------------
// 1. The container: magic, block walk, and every §8.2 field, against metaflac.
// ---------------------------------------------------------------------------

#[test]
fn the_stream_layer_agrees_with_the_reference_tool_on_every_fixture() {
    let vectors = load_vectors();
    assert_eq!(
        vectors.len(),
        6,
        "the table must cover the six fixtures the generator is defined to \
         produce — and a fixture the table names that this suite does not read \
         is a fixture whose facts are not being checked"
    );
    for v in &vectors {
        let bytes = v.fixture();
        // The fixture identity, before anything reads it: a fixture that changed
        // under the table would otherwise show up as a dozen field mismatches.
        assert_eq!(
            bytes.len(),
            v.num("fixture_len") as usize,
            "{}: length",
            v.label
        );
        assert_eq!(
            fnv1a64(bytes),
            v.num("fixture_fnv1a64"),
            "{}: FNV-1a-64 pin — the committed bytes no longer match the bytes \
             the generator wrote them from",
            v.label
        );
        assert_eq!(&bytes[..MAGIC.len()], &MAGIC, "{}: magic", v.label);

        // One fixture is a refusal rather than a read: the depth it declares is a
        // capability this decoder does not have, so there is no `StreamInfo` to
        // compare field by field. Which fixture that is comes from the table, so
        // the skip cannot quietly widen to cover a regression; the refusal itself
        // is asserted here and in full by the 24-bit test below.
        let facts = match stream::parse(bytes) {
            Ok(f) => f,
            Err(e) => {
                assert_eq!(
                    v.num("streaminfo_bits_per_sample"),
                    24,
                    "{v:?}: a fixture this suite cannot read"
                );
                assert_eq!(
                    e,
                    Error::UnsupportedSampleSize,
                    "{}: refusal class",
                    v.label
                );
                assert_eq!(
                    stream::frame_data_offset(bytes)
                        .unwrap_or_else(|e| panic!("{}: {e:?}", v.label)),
                    v.num("frame_data_offset") as usize,
                    "{}: the container facts outlive the capability refusal",
                    v.label
                );
                continue;
            }
        };

        // §8.1: the walk, block by block, from the real header bytes. `parse`
        // reports only a count; a caller that wants the list drives `Blocks`,
        // and this is where that interface is checked against metaflac's list.
        let mut walk = Blocks::new(bytes).unwrap_or_else(|e| panic!("{}: walk: {e:?}", v.label));
        let mut seen = Vec::new();
        while let Some(b) = walk.next_block().unwrap() {
            seen.push(b);
        }
        assert_eq!(seen.len(), v.blocks.len(), "{}: block count", v.label);
        assert_eq!(
            facts.block_count,
            v.blocks.len(),
            "{}: reported count",
            v.label
        );
        assert_eq!(
            seen.len(),
            v.num("block_count") as usize,
            "{}: metaflac count row",
            v.label
        );

        let mut previous_end = MAGIC.len();
        for (i, (b, want)) in seen.iter().zip(&v.blocks).enumerate() {
            assert_eq!(
                want.number, i,
                "{}: the table numbers its blocks 0..n",
                v.label
            );
            assert_eq!(b.offset, previous_end, "{}: blocks are contiguous", v.label);
            assert_eq!(b.block_type, want.block_type, "{}: block type", v.label);
            assert_eq!(b.is_last, want.is_last, "{}: last-block flag", v.label);
            assert_eq!(b.length, want.length, "{}: block length", v.label);
            assert_eq!(
                b.span().unwrap().0,
                b.offset + 4,
                "{}: header is 4 bytes",
                v.label
            );
            assert_eq!(
                b.payload(bytes).map(|p| p.len()),
                Some(b.length),
                "{}: payload length",
                v.label
            );
            previous_end = b.after().unwrap();
        }
        // The chain ends exactly where the frames begin — the number item 4a
        // exists to produce, reached a second way (the walk's own cursor).
        assert_eq!(
            previous_end, facts.frame_data_offset,
            "{}: end of chain",
            v.label
        );
        assert_eq!(
            walk.cursor(),
            facts.frame_data_offset,
            "{}: walk cursor",
            v.label
        );
        assert_eq!(
            facts.frame_data_offset,
            v.num("frame_data_offset") as usize,
            "{}: frame_data_offset",
            v.label
        );

        // §8.2 Table 3, every field, against metaflac's own numbers.
        let info = &facts.info;
        assert_eq!(
            info.min_blocksize as u64,
            v.num("streaminfo_minimum_blocksize"),
            "{}: min blocksize",
            v.label
        );
        assert_eq!(
            info.max_blocksize as u64,
            v.num("streaminfo_maximum_blocksize"),
            "{}: max blocksize",
            v.label
        );
        assert_eq!(
            info.min_framesize,
            v.num("streaminfo_minimum_framesize") as u32,
            "{}: min framesize",
            v.label
        );
        assert_eq!(
            info.max_framesize,
            v.num("streaminfo_maximum_framesize") as u32,
            "{}: max framesize",
            v.label
        );
        assert_eq!(
            info.sample_rate_hz,
            v.num("streaminfo_sample_rate_hz") as u32,
            "{}: sample rate",
            v.label
        );
        assert_eq!(
            info.channels as u64,
            v.num("streaminfo_channels"),
            "{}: channels",
            v.label
        );
        assert_eq!(
            info.bits_per_sample,
            v.num("streaminfo_bits_per_sample") as u32 as u8,
            "{}: bits per sample",
            v.label
        );
        assert_eq!(
            info.total_samples,
            v.num("streaminfo_total_samples"),
            "{}: total samples",
            v.label
        );
        assert_eq!(info.md5, v.md5(), "{}: MD5 signature", v.label);
        // …and the sentinel is compared to the table's row, not assumed: four
        // fixtures carry a real signature, `s5` carries the all-zero one.
        assert_eq!(
            info.md5_known(),
            v.md5() != [0u8; 16],
            "{}: MD5 set/unset",
            v.label
        );

        // The §8.2 sentinels, read as sentinels rather than defaulted.
        let total_known = info.total_samples != 0;
        assert_eq!(
            info.total_samples_known(),
            total_known,
            "{}: total-samples sentinel",
            v.label
        );
        assert_eq!(
            info.md5_known(),
            info.md5 != [0u8; 16],
            "{}: MD5 sentinel",
            v.label
        );
        assert_eq!(
            info.frame_sizes_known(),
            info.min_framesize != 0 && info.max_framesize != 0,
            "{}: frame-size sentinels",
            v.label
        );
        assert_eq!(
            info.is_fixed_blocksize(),
            info.min_blocksize == info.max_blocksize,
            "{}: fixed-blocksize test (§8.2)",
            v.label
        );

        // A skipped block is still readable: a Vorbis comment block whose
        // payload is exactly the vendor string the generator recorded, which is
        // what "walked past by length" has to leave intact for a caller that
        // does care. (The four fixtures with a comment block all carry the
        // encoder's own vendor string and no comments — `--no-padding` and no
        // metadata options, libFLAC's own output.)
        for b in seen.iter().filter(|b| b.block_type == 4) {
            let payload = b.payload(bytes).unwrap();
            let vendor_len =
                u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
            assert_eq!(
                payload.len(),
                vendor_len + 4 + 4,
                "{}: comment block should hold a vendor string and no comments",
                v.label
            );
            assert_eq!(
                std::str::from_utf8(&payload[4..4 + vendor_len]).unwrap(),
                v.raw("encoder_vendor"),
                "{}: vendor string",
                v.label
            );
            assert_eq!(
                &payload[payload.len() - 4..],
                &[0, 0, 0, 0],
                "{}: comment count",
                v.label
            );
        }
    }
}

#[test]
fn the_first_block_is_the_streaminfo_in_every_real_stream() {
    // §8.2's MUST, witnessed on encoder output rather than on assembled bytes:
    // every fixture's block 0 is the 34-byte streaminfo, and the walk's own
    // type test agrees with the table's name column.
    for v in &load_vectors() {
        let want = &v.blocks[0];
        assert_eq!(
            want.block_type, TYPE_STREAMINFO,
            "{}: block 0 type",
            v.label
        );
        assert_eq!(want.type_name, "STREAMINFO", "{}: block 0 name", v.label);
        assert_eq!(want.length, 34, "{}: streaminfo length", v.label);
        assert!(
            !want.is_last || v.blocks.len() == 1,
            "{}: streaminfo carries the last flag only when it is the only block",
            v.label
        );
        let mut walk = Blocks::new(v.fixture()).unwrap();
        let first = walk.next_block().unwrap().unwrap();
        assert_eq!(first.block_type, TYPE_STREAMINFO);
        assert_eq!(first.length, stream::STREAMINFO_LEN);
        assert_eq!(
            first.payload(v.fixture()).unwrap().len(),
            stream::STREAMINFO_LEN
        );
    }
}

// ---------------------------------------------------------------------------
// 2. The number item 4a exists to produce: a byte offset the frame layer can
//    be pointed at, plus the `StreamDefaults` it needs to resolve there.
// ---------------------------------------------------------------------------

#[test]
fn the_reported_offset_is_where_the_frame_layer_can_parse_the_first_frame() {
    for v in &load_vectors() {
        let bytes = v.fixture();
        let facts = match stream::parse(bytes) {
            Ok(f) => f,
            // The 24-bit fixture: its depth is a capability this decoder does
            // not have, so there are no defaults to build. Asserted in full by
            // the 24-bit test below; here, the offset itself is still a fact of
            // the container, which is what `frame_data_offset` is for.
            Err(Error::UnsupportedSampleSize) => {
                let offset = stream::frame_data_offset(bytes).unwrap();
                assert_eq!(
                    offset,
                    v.num("frame_data_offset") as usize,
                    "{}: offset without streaminfo",
                    v.label
                );
                continue;
            }
            Err(e) => panic!("{}: {e:?}", v.label),
        };
        let offset = facts.frame_data_offset;
        assert!(
            offset < bytes.len(),
            "{}: a fixture has frames after its metadata",
            v.label
        );

        // The hex row is the real bytes at the real offset — the table cannot
        // be describing a different position than the one the crate reports.
        let width_bits = v.num("first_frame_header_bits") as usize;
        let header_bytes = &bytes[offset..offset + width_bits / 8];
        assert_eq!(
            hex(header_bytes),
            v.raw("first_frame_hex"),
            "{}: first frame bytes",
            v.label
        );

        // And the crate's own frame parser, pointed at that offset with the
        // defaults the stream layer produced, reads the row the strict Python
        // filter read — two implementations of §9.1 meeting on real bytes.
        let defaults = facts.info.as_defaults();
        assert_eq!(
            defaults.sample_rate_hz,
            v.num("streaminfo_sample_rate_hz") as u32,
            "{}: default rate",
            v.label
        );
        assert_eq!(
            defaults.bits_per_sample as u64,
            v.num("streaminfo_bits_per_sample"),
            "{}: default depth",
            v.label
        );

        let mut reader = BitReader::new(&bytes[offset..]);
        let header = FrameHeader::parse(&mut reader, &defaults)
            .unwrap_or_else(|e| panic!("{}: frame at {offset}: {e:?}", v.label));
        assert_eq!(header.header_bits, width_bits, "{}: header width", v.label);
        assert_eq!(
            reader.bit_position(),
            width_bits,
            "{}: cursor lands on the first subframe bit",
            v.label
        );
        assert_eq!(
            v.num("first_frame_number"),
            header.number,
            "{}: frame number",
            v.label
        );
        assert_eq!(
            v.num("first_frame_blocksize") as usize,
            header.blocksize,
            "{}: block size",
            v.label
        );
        assert_eq!(
            v.num("first_frame_sample_rate_hz") as u32,
            header.sample_rate.hz(defaults.sample_rate_hz),
            "{}: resolved sample rate",
            v.label
        );
        assert_eq!(
            v.num("first_frame_bits_per_sample") as u8,
            header.bits_per_sample,
            "{}: resolved depth",
            v.label
        );
        assert_eq!(
            (
                v.num("first_frame_subframes") as usize,
                v.raw("first_frame_decorrelation").to_string()
            ),
            channel_shape(header.channels),
            "{}: channel assignment",
            v.label
        );
    }
}

#[test]
fn the_last_frame_is_short_where_the_stream_says_it_is_and_ends_the_region() {
    // §8.2: "The last frame MUST have a block size equal to or less than the
    // maximum block size" — and for a fixed-blocksize stream the short tail is
    // exactly what the sample count leaves over. Asserting that arithmetic
    // against a real last frame is the cheapest way to show both the count and
    // the tail are the same fact, and it is the arithmetic item 4b's
    // end-of-stream rule has to survive.
    for v in &load_vectors() {
        let bytes = v.fixture();
        let Some(facts) = stream::parse(bytes)
            .ok()
            .filter(|_| v.num("streaminfo_total_samples") > 0)
        else {
            continue; // the 24-bit and unknown-total fixtures are asserted below
        };
        let offset = v.num("last_frame_offset") as usize;
        let width_bits = v.num("last_frame_header_bits") as usize;
        assert!(
            offset < bytes.len(),
            "{}: the last frame is inside the region",
            v.label
        );
        assert_eq!(
            v.num("last_frame_bytes_to_region_end") as usize,
            bytes.len() - offset,
            "{}: the last frame runs to the end of the region",
            v.label
        );

        let defaults = facts.info.as_defaults();
        let mut reader = BitReader::new(&bytes[offset..]);
        let header = FrameHeader::parse(&mut reader, &defaults)
            .unwrap_or_else(|e| panic!("{}: frame at {offset}: {e:?}", v.label));
        assert_eq!(
            header.header_bits, width_bits,
            "{}: last header width",
            v.label
        );
        assert_eq!(
            header.number as u64,
            v.num("last_frame_number"),
            "{}: last frame number",
            v.label
        );
        assert_eq!(
            header.number as u64,
            v.num("frame_count") - 1,
            "{}: frames are numbered from 0, contiguously (§8.2/§9.1.4)",
            v.label
        );
        assert!(
            header.blocksize <= usize::from(facts.info.max_blocksize),
            "{}: the last frame may be short, never longer",
            v.label
        );
        assert_eq!(
            header.blocksize,
            v.num("last_frame_blocksize") as usize,
            "{}: last block size",
            v.label
        );
        if facts.info.is_fixed_blocksize() {
            // The count is the ceiling over the block size (§8.2), and the
            // remainder is the tail — both read off one real last frame.
            assert_eq!(
                facts.info.exact_frame_count(),
                Some(v.num("frame_count") as usize),
                "{}: frame count from §8.2's own fields",
                v.label
            );
            assert_eq!(
                facts.info.total_samples
                    - (v.num("frame_count") - 1) * u64::from(facts.info.max_blocksize),
                header.blocksize as u64,
                "{}: the short tail is the sample count's remainder",
                v.label
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Header width: the census the generator measured, and the bounds the frame
//    layer now derives instead of asserting from an encoder accident.
// ---------------------------------------------------------------------------

#[test]
fn every_real_header_is_inside_the_measured_census_and_the_derived_bounds() {
    let mut widest_fixture = 0usize;
    for v in &load_vectors() {
        let bytes = v.fixture();
        // The frame region's start is a fact even where the declared depth is a
        // capability this decoder does not have (`s4`), so the census needs the
        // offset and not a `StreamInfo` — that is the ordering this suite keeps
        // insisting on.
        let at = stream::frame_data_offset(bytes)
            .unwrap_or_else(|e| panic!("{}: frame_data_offset: {e:?}", v.label));
        assert_eq!(
            at,
            v.num("frame_data_offset") as usize,
            "{}: frame start",
            v.label
        );
        // Defaults come from the table, i.e. from `metaflac`, not from the code
        // under test: for `s4` that is what makes the frame layer's own refusal
        // (below) an independent fact rather than an artifact of `as_defaults`.
        let defaults = StreamDefaults {
            sample_rate_hz: v.num("streaminfo_sample_rate_hz") as u32,
            bits_per_sample: v.num("streaminfo_bits_per_sample") as u8,
        };
        let observed = [
            ("first", v.num("first_frame_header_bits") as usize, at),
            (
                "last",
                v.num("last_frame_header_bits") as usize,
                v.num("last_frame_offset") as usize,
            ),
        ];
        // The table's census rows describe one another — max == max(census),
        // census sorted+deduped — so a parsed width that is not in the census
        // means the fixture changed under the table, not that the crate is
        // wrong. (The generator refuses to write if the census moves.)
        assert_eq!(
            v.widths.iter().max().copied().unwrap_or(0) as u64,
            v.num("max_frame_header_width_bits"),
            "{}: census max",
            v.label
        );
        for (what, width, offset) in observed {
            assert!(
                v.widths.contains(&width),
                "{}: {width}-bit header is not in the census {:?}",
                v.label,
                v.widths
            );
            assert!(
                (MIN_HEADER_BITS..=MAX_ACCEPTED_HEADER_BITS).contains(&width),
                "{}: a {width}-bit header is outside the bounds frame.rs derives",
                v.label
            );
            // Every width in the census is a width the crate's own parser
            // reaches, on the real bytes, at the offset the table reports.
            let mut reader = BitReader::new(&bytes[offset..]);
            match FrameHeader::parse(&mut reader, &defaults) {
                Ok(header) => {
                    assert_eq!(
                        header.header_bits, width,
                        "{}: {what} header width",
                        v.label
                    );
                    assert_eq!(reader.bit_position(), width, "{}: {what} cursor", v.label);
                }
                // `s4` is the one fixture whose headers this crate will not
                // read, and it refuses for the depth the stream declares —
                // never for the width. The width is still measurable off the
                // bytes, which is what the arithmetic below is for.
                Err(e) => {
                    assert_eq!(
                        e,
                        Error::UnsupportedSampleSize,
                        "{}: {what} frame header",
                        v.label
                    );
                    if what == "first" {
                        assert_eq!(
                            header_width_from_hex(v),
                            width,
                            "{}: {what} width off the bytes",
                            v.label
                        );
                    }
                }
            }
        }
        widest_fixture = widest_fixture.max(v.widths.iter().max().copied().unwrap_or(0));
    }
    // The widest header in any committed fixture, and the room the derived
    // bounds leave above it. If a future libFLAC writes wider headers, this
    // assertion is where the gap between "what a packer emits" and "what the
    // format allows" gets re-opened rather than quietly widened.
    assert_eq!(
        widest_fixture, 80,
        "s3_wide_48001hz_b3000 is the widest real header this repo commits"
    );
    assert!(widest_fixture < MAX_ACCEPTED_HEADER_BITS);
    assert!(
        widest_fixture > 72,
        "…and it is outside the 48..=72 profile the old debug_assert assumed — which is why that assertion is gone, not raised"
    );
}

#[test]
fn a_real_stream_carries_a_header_outside_this_packer_s_profile() {
    // `s3_wide_48001hz_b3000`: 48001 Hz has no 4-bit sample-rate code (§9.1.7
    // appends a 16-bit Hz value) and 3000 samples has no 4-bit block-size code
    // (§9.1.6 appends a 16-bit size-minus-1). Both appended forms sit after the
    // coded number, so the header is 32 + 8 + 16 + 16 + 8 = 80 bits. This is
    // the fixture the width rules are stated against: a claim about the format,
    // witnessed by a real stream rather than by a hand-built header.
    let v = load_vectors()
        .into_iter()
        .find(|v| v.label == "s3_wide_48001hz_b3000")
        .expect("the wide-header fixture is in the table");
    let bytes = v.fixture();
    let facts = parsed(&v);
    assert_eq!(
        v.widths,
        vec![80],
        "both frames of this fixture carry the 16-bit forms"
    );

    let mut reader = BitReader::new(&bytes[facts.frame_data_offset..]);
    let header = FrameHeader::parse(&mut reader, &facts.info.as_defaults()).unwrap();
    assert_eq!(header.header_bits, 80);
    assert_eq!(reader.bit_position(), 80);
    assert_eq!(
        header.blocksize, 3000,
        "the 16-bit block-size form, value+1"
    );
    assert_eq!(
        header.sample_rate,
        flac_lite::format::SampleRate::Explicit(48001),
        "the 16-bit Hz form, not a table code"
    );
    assert_eq!(header.number, 0);

    // The second frame of the same stream carries the short tail: 4801 total,
    // one full block of 3000, and 1801 left over — the uncommon form twice, at
    // a width nothing in this project's profile would produce.
    let tail_offset = v.num("last_frame_offset") as usize;
    let mut reader = BitReader::new(&bytes[tail_offset..]);
    let tail = FrameHeader::parse(&mut reader, &facts.info.as_defaults()).unwrap();
    assert_eq!(tail.blocksize, 1801);
    assert_eq!(tail.header_bits, 80);
    assert_eq!(tail.number, 1);
    assert_eq!(
        bytes.len() - tail_offset,
        v.num("last_frame_bytes_to_region_end") as usize
    );
}

// ---------------------------------------------------------------------------
// 4. The two fixtures that are refusals, not decode targets.
// ---------------------------------------------------------------------------

#[test]
fn a_valid_24_bit_stream_refuses_at_the_depth_not_at_the_container() {
    // `s4_24bit_8k_mono` is verified by `flac -t` (the generator insists) and
    // read by `metaflac`; the reference decoder carries 24-bit samples. This
    // one does not — `frame::bits_per_sample` covers 8 and 16 — and the point
    // of asserting it here is *where* it stops:
    //
    //   * the container walk succeeds, and the frame offset is a fact (§8.1
    //     says nothing about bit depth);
    //   * `StreamInfo::parse` refuses at the field that states the depth, with
    //     the capability variant, not the format variant;
    //   * the frame layer reaches the same variant independently, so the
    //     refusal is a property of the decoder and not an artefact of one
    //     module's ordering.
    //
    // A caller that wants to say "this file is fine, my decoder is not" gets
    // the two facts separated: `UnsupportedSampleSize` from the stream layer,
    // and the walk still available.
    let v = load_vectors()
        .into_iter()
        .find(|v| v.label == "s4_24bit_8k_mono")
        .expect("the 24-bit fixture is in the table");
    let bytes = v.fixture();

    assert_eq!(
        stream::parse(bytes).err(),
        Some(Error::UnsupportedSampleSize)
    );
    let offset = stream::frame_data_offset(bytes).unwrap();
    assert_eq!(offset, v.num("frame_data_offset") as usize);

    // The depth is stated by the stream and by the frame, and both name it as
    // unsupported rather than as invalid — the field itself is read correctly.
    let mut walk = Blocks::new(bytes).unwrap();
    let si = walk.next_block().unwrap().unwrap();
    let info = stream::StreamInfo::parse(si.payload(bytes).unwrap());
    assert_eq!(info.err(), Some(Error::UnsupportedSampleSize));
    // …which is only observable because the layout was read: the same block,
    // parsed with the depth replaced by 16, is well-formed at 8000 Hz.
    //
    // Table 3's field order puts the depth at **bits 103..108** — one bit (the
    // last of byte 12) plus the top nibble of byte 13. A field that straddles a
    // byte boundary is the trap: the first draft of this patch wrote byte 17,
    // which is inside the MD5 signature, and the refusal that came back from a
    // block that had just been "patched" is the witness that the *patch* was
    // wrong rather than the parser. (Writing the nibbles the other way round is
    // the second trap — it turns 16-bit into 8-bit, which parses fine. Same
    // lesson as the block-row off-by-one in `load_vectors`: a suite that runs is
    // not a suite that asserts.)
    let mut payload = si.payload(bytes).unwrap().to_vec();
    let depth: u32 = 16 - 1; // Table 3 stores bits-per-sample minus one
    payload[12] = (payload[12] & !0b1) | (((depth >> 4) & 1) as u8); // bit 103 = MSB
    payload[13] = (payload[13] & 0b0000_1111) | (((depth & 0xF) as u8) << 4); // bits 104..108
    let patched = stream::StreamInfo::parse(&payload).unwrap();
    assert_eq!(
        patched.sample_rate_hz,
        v.num("streaminfo_sample_rate_hz") as u32
    );
    assert_eq!(patched.bits_per_sample, 16);

    let defaults = StreamDefaults {
        sample_rate_hz: 8000,
        bits_per_sample: 24,
    };
    let mut reader = BitReader::new(&bytes[offset..]);
    assert_eq!(
        FrameHeader::parse(&mut reader, &defaults).err(),
        Some(Error::UnsupportedSampleSize),
        "the frame layer refuses on its own: code 0b000 resolves through a default of 24"
    );
    assert_eq!(
        header_width_from_hex(&v),
        v.num("first_frame_header_bits") as usize
    );
    assert_eq!(v.raw("first_frame_decorrelation"), "none");
    assert_eq!(v.num("streaminfo_bits_per_sample"), 24);
}

/// Re-measure the first header's width from the fixture bytes, independently of
/// the crate's parser: the fixed 32 bits, the coded number's octet count, the
/// two appended forms the code bits promise, and the CRC. This is the arithmetic
/// the generator's census used, written a third way.
fn header_width_from_hex(v: &Vector) -> usize {
    let bytes = v.fixture();
    let at = v.num("frame_data_offset") as usize;
    let bs_code = (bytes[at + 2] >> 4) & 15;
    let rate_code = bytes[at + 2] & 15;
    // Table 18's lead column counted out of the raw byte, not read from the
    // crate's table — that is the whole point of a third reading. `lead` is at
    // byte 4 because every fixture's fixed fields end on a byte boundary there.
    let lead = bytes[at + 4];
    let ones = (0..8u32).take_while(|i| lead & (0x80 >> i) != 0).count();
    let octets = match ones {
        0 => 1,
        // A continuation octet cannot start a number; 0xFF is the all-ones lead
        // RFC 9639 leaves unused. Neither is a frame start, so the census row
        // for this fixture would be measuring a non-frame.
        1 | 8 => panic!(
            "{}: {lead:02X} at the frame start is not a coded number",
            v.label
        ),
        n => n,
    };
    // §9.1.6 and §9.1.7: the uncommon block-size and sample-rate codes append
    // their value *after* the coded number; every other code is self-contained.
    let appended = match bs_code {
        0b0110 => 1,
        0b0111 => 2,
        _ => 0,
    } + match rate_code {
        0b1100 => 1,
        0b1101 | 0b1110 => 2,
        _ => 0,
    };
    32 + octets * 8 + appended * 8 + 8
}

// ---------------------------------------------------------------------------
// 5. The fixture that declares nothing.
// ---------------------------------------------------------------------------

#[test]
fn a_real_stream_can_declare_nothing_about_its_own_length() {
    // `s5_unseekable_total_unknown`: the same audio as `s1`, encoded from raw
    // PCM through a pipe. §8.2's three "a value of 0 signifies that the value
    // is not known" sentinels are all present — total samples, both frame
    // sizes, and the MD5 — and the file is still a valid FLAC stream that
    // `flac -t` accepts. It contains ten frames.
    //
    // The consequence for item 4b is the reason this fixture exists: a decoder
    // that plans to stop at `ceil(total_samples / max_blocksize)` would stop
    // before its first frame, and one that plans to size its buffers from
    // `max_framesize` would size them at zero. "Unknown" has to stay unknown
    // through the stream layer, which is what these assertions are for; the
    // frame offset, by contrast, is known exactly.
    let v = load_vectors()
        .into_iter()
        .find(|v| v.label == "s5_unseekable_total_unknown")
        .expect("the unseekable fixture is in the table");
    let bytes = v.fixture();
    let facts = parsed(&v);
    let info = &facts.info;

    assert_eq!(info.total_samples, 0);
    assert!(!info.total_samples_known());
    assert_eq!(info.min_framesize, 0);
    assert_eq!(info.max_framesize, 0);
    assert!(!info.frame_sizes_known());
    assert_eq!(info.md5, [0u8; 16]);
    assert!(!info.md5_known());
    assert!(!info.md5_known());

    // The stream is fixed-blocksize (§8.2's term) and still has no count: a
    // known block size with an unknown total is not enough information, and
    // offering a guess here would be the mistake this fixture was generated to
    // catch.
    assert!(info.is_fixed_blocksize());
    assert_eq!(info.exact_frame_count(), None);
    assert!(info.max_blocksize >= 16);

    // …while the frames are unambiguously there: ten, numbered 0..9, the last
    // one ending the region, exactly as the two-block container above.
    assert_eq!(facts.frame_data_offset, v.num("frame_data_offset") as usize);
    assert_eq!(v.num("frame_count"), 10);
    let region = &bytes[facts.frame_data_offset..];
    assert!(!region.is_empty());
    let mut reader = BitReader::new(region);
    let first = FrameHeader::parse(&mut reader, &info.as_defaults()).unwrap();
    assert_eq!(first.number, 0);
    assert_eq!(first.blocksize, usize::from(info.max_blocksize));
    let tail_offset = v.num("last_frame_offset") as usize;
    let mut reader = BitReader::new(&bytes[tail_offset..]);
    let tail = FrameHeader::parse(&mut reader, &info.as_defaults()).unwrap();
    assert_eq!(tail.number, 9);
    assert_eq!(
        tail.blocksize, 48,
        "the remainder, with nothing in the stream declaring it"
    );
    assert_eq!(
        bytes.len() - tail_offset,
        region.len() - (tail_offset - facts.frame_data_offset)
    );

    // The container is the same two-block shape as `s1`; only the declared
    // facts differ. Same bytes, same audio, opposite metadata: the difference
    // a decoder must be able to tell.
    let plain = load_vectors()
        .into_iter()
        .find(|v| v.label == "s1_plain_8k_mono")
        .unwrap();
    assert_eq!(facts.frame_data_offset, parsed(&plain).frame_data_offset);
    assert_eq!(facts.info.max_blocksize, parsed(&plain).info.max_blocksize);
    assert_eq!(
        parsed(&plain).info.exact_frame_count(),
        Some(10),
        "…whereas s1 does declare enough to count, and counts the same ten frames"
    );
    assert_ne!(
        info.total_samples_known(),
        parsed(&plain).info.total_samples_known()
    );
}

// ---------------------------------------------------------------------------
// 6. The scan-cost property item 4b depends on.
// ---------------------------------------------------------------------------

#[test]
fn a_non_frame_start_in_the_frame_region_is_not_mistaken_for_a_frame() {
    // The stream layer reports one number — where the frames start — so it is
    // also the layer that can say where they do not. Item 4b's scanner will
    // depend on a wrong byte position being refused cheaply; the cheapest
    // witness available at this layer is the byte immediately after the last
    // frame of a real fixture, where the region has ended.
    for v in &load_vectors() {
        let bytes = v.fixture();
        // Both numbers here come from the container and the table, not from a
        // `StreamInfo`: the fixture whose depth this decoder cannot carry still
        // has a frame region, and refusing it for its depth is a different
        // answer from refusing it for not being a frame — both are refusals,
        // which is all a scanner may conclude from a single position.
        let start =
            stream::frame_data_offset(bytes).unwrap_or_else(|e| panic!("{}: {e:?}", v.label));
        assert_eq!(
            start,
            v.num("frame_data_offset") as usize,
            "{}: frame start",
            v.label
        );
        let defaults = StreamDefaults {
            sample_rate_hz: v.num("streaminfo_sample_rate_hz") as u32,
            bits_per_sample: v.num("streaminfo_bits_per_sample") as u8,
        };
        let region_end =
            v.num("last_frame_offset") as usize + v.num("last_frame_bytes_to_region_end") as usize;
        assert_eq!(
            region_end,
            bytes.len(),
            "{}: the region ends at the last frame's end",
            v.label
        );
        // Nothing after it: an empty tail is end-of-stream, and `FrameSync` is
        // the other answer (PR #88's distinction) — this asserts the first.
        let mut reader = BitReader::new(&bytes[region_end..]);
        assert_eq!(
            FrameHeader::parse(&mut reader, &defaults).err(),
            Some(Error::EndOfStream),
            "{}: no bytes, no frame",
            v.label
        );
        // And a byte that is not a sync code is not a frame either. `fLaC`'s
        // first byte is 0x66, the metadata length bytes are arbitrary, and the
        // second block's type byte is a real near-miss; a scanner has to be
        // able to tell all of them from a frame start.
        for probe in [start - 1, MAGIC.len() + 4] {
            let mut reader = BitReader::new(&bytes[probe..]);
            let err = FrameHeader::parse(&mut reader, &defaults).err();
            assert!(err.is_some(), "{}: byte {probe} parsed as a frame", v.label);
        }
    }
}

// ---------------------------------------------------------------------------
// The frame-number FORM rules: what they cost, and where they diverge from the
// reference decoder. Both come out of the table, and the table got them from
// running things — `frame_number_forms_*`/`scan_frame_starts_*` are censuses the
// generator takes over the fixtures (one of them by a second, independent
// reading of §9.1, in `scripts/stream_vectors.py`), and `mutation*` is a real
// frame header of one fixture rewritten, with `flac -d` run on the result.

/// Parse a hex row from the table back into bytes. The `hex()` helper goes the
/// other way; a mutation row has to be spliced *into* the fixture, so this side
/// of the round trip is needed too.
fn unhex(what: &str) -> Vec<u8> {
    what.split_whitespace()
        .map(|byte| {
            u8::from_str_radix(byte, 16)
                .unwrap_or_else(|e| panic!("{what:?} contains {byte:?}: {e}"))
        })
        .collect()
}

#[test]
fn the_frame_number_form_and_scan_censuses_are_pinned_on_every_fixture() {
    // Refusing a non-minimal coded number (Table 18 gives each value exactly
    // one form) is a *policy* choice, so its cost has to be a measurement
    // rather than an argument. The generator takes two counts per fixture, and
    // both are pinned here.
    for v in &load_vectors() {
        // (a) Nothing real is rejected: every frame number libFLAC wrote in
        // this fixture is in its minimal form. The generator asserts this too —
        // the row is the record, the assertion is the reminder.
        assert_eq!(
            v.raw("frame_number_forms_all_minimal"),
            "true",
            "{}: libFLAC wrote a frame number in a non-minimal form",
            v.label
        );
        assert_eq!(
            v.num("frame_number_forms_checked"),
            v.num("frame_count"),
            "{}: the form census did not cover every frame",
            v.label
        );
        // (b) With the rule in force a §9.1 header scan over the frame region
        // finds exactly the frames — no extra candidate starts. That is the
        // property item 4b's scanner inherits, and it is counted by a reading
        // of §9.1 written independently of this crate and of the frame filter.
        let strict = v.num("scan_frame_starts_strict");
        assert_eq!(
            strict,
            v.num("frame_count"),
            "{}: an independent header scan finds {} starts, not {} frames",
            v.label,
            strict,
            v.num("frame_count")
        );
        // (c) What the rule removes. A rule can only reduce the candidate set,
        // so `lax >= strict` is the structural half; the measured half is the
        // delta, which on every fixture committed here is zero. If a future
        // regeneration ever makes it non-zero that is new information about
        // real audio bytes, to be recorded on purpose in the module docs and
        // FLAC.md — not a threshold to re-tune quietly.
        let lax = v.num("scan_frame_starts_all_forms_allowed");
        assert!(
            lax >= strict,
            "{}: dropping a rule removed a frame start ({lax} < {strict})",
            v.label
        );
        assert_eq!(
            lax - strict,
            0,
            "{}: the minimal-form rule changed the frame-start count ({lax} vs {strict}) — \
             record it deliberately",
            v.label
        );
    }
}

#[test]
fn a_frame_number_in_a_longer_form_is_refused_here_and_accepted_there() {
    // `s1`'s first frame header, rewritten with its number spelled in each
    // longer form, both CRCs recomputed by the generator (see
    // `mutate_number_form` in scripts/stream_vectors.py). The same bytes are fed
    // to two readers: ours, and `flac -d`. They do not agree, and the table
    // says so in as many words.
    let vs = load_vectors();
    let hosts: Vec<&_> = vs
        .iter()
        .filter(|v| v.rows.contains_key("mutation"))
        .collect();
    assert_eq!(
        hosts.len(),
        1,
        "exactly one fixture carries the number-form mutation"
    );
    let v = hosts[0];
    assert_eq!(v.raw("mutation"), "number-form-overlong");

    let at = v.num("mutation_at") as usize;
    assert_eq!(
        at,
        v.num("frame_data_offset") as usize,
        "{}: the mutation sits where the stream layer says frames start",
        v.label
    );
    let bytes = v.fixture();
    let before = unhex(v.raw("mutation_before_hex"));
    assert_eq!(
        &bytes[at..at + before.len()],
        before.as_slice(),
        "{}: the pre-mutation header is not the fixture's own bytes",
        v.label
    );

    let defaults = StreamDefaults {
        sample_rate_hz: v.num("streaminfo_sample_rate_hz") as u32,
        bits_per_sample: v.num("streaminfo_bits_per_sample") as u8,
    };
    // Control: the fixture as committed parses, at the width the table records.
    // The refusals below are about the number's spelling and nothing else.
    let mut reader = BitReader::new(&bytes[at..]);
    let control = FrameHeader::parse(&mut reader, &defaults)
        .unwrap_or_else(|e| panic!("{}: the unmutated header: {e:?}", v.label));
    assert_eq!(
        control.header_bits,
        v.num("first_frame_header_bits") as usize,
        "{}: control header width",
        v.label
    );

    let forms: Vec<u64> = v
        .raw("mutation_forms")
        .split_whitespace()
        .map(|t| t.parse().expect("mutation_forms is not a list of numbers"))
        .collect();
    assert!(!forms.is_empty(), "{}: no forms to try", v.label);
    for form in forms {
        let want = unhex(&format!(
            "{}",
            v.raw(&format!("mutation_form{form}_header_hex"))
        ));
        assert!(want.len() > 1, "a header is more than one byte");
        // The generator's header CRC-8 is this crate's CRC-8 over the same
        // range: the mutated header is sound except for how the number is
        // spelled. (Its frame CRC-16 is recomputed too — that one is what made
        // an earlier measurement here read as a rejection.)
        assert_eq!(
            crc8(&want[..want.len() - 1]),
            want[want.len() - 1],
            "{}/form {form}: the pinned header's CRC-8 byte is not ours",
            v.label
        );
        let mut mutated: Vec<u8> = Vec::with_capacity(bytes.len() + form as usize);
        mutated.extend_from_slice(&bytes[..at]);
        mutated.extend_from_slice(&want);
        mutated.extend_from_slice(&bytes[at + before.len()..]);

        // Our answer: refused. And the refusal happens at the number, not
        // after it: `read_utf8_coded` is atomic at its own position (per-read
        // atomicity, see `frame`'s module docs), so the cursor sits exactly at
        // the start of the coded number — the longer spelling was put back,
        // not consumed and kept. (`MIN_HEADER_BITS` is 32 fixed bits + a
        // 1-octet number + the CRC-8, so subtracting 16 leaves the width of the
        // fixed fields, which is where a frame number begins. A scanner must
        // still re-position by byte offset to try the next candidate; this only
        // says the header was never accepted.)
        let mut reader = BitReader::new(&mutated[at..]);
        assert_eq!(
            FrameHeader::parse(&mut reader, &defaults).err(),
            Some(Error::InvalidField),
            "{}/form {form}: a non-minimal frame number parsed",
            v.label
        );
        assert_eq!(
            reader.bit_position(),
            MIN_HEADER_BITS - 16,
            "{}/form {form}: a refusal left the cursor elsewhere ({} bits)",
            v.label,
            reader.bit_position()
        );

        // The reference decoder's answer, pinned as an exit status from
        // `flac -d` (which decodes and MD5-checks). Not an assumption: if
        // libFLAC's behaviour changes, this row changes with it and the
        // divergence has to be re-argued, not inherited.
        assert_eq!(
            v.num(&format!("mutation_form{form}_reference_exit")),
            0,
            "{}/form {form}: the reference decoder was expected to accept this \
             stream — the divergence is the point of the row",
            v.label
        );
    }
    assert_eq!(
        v.raw("reference_decoder_accepts_nonminimal_forms"),
        "true",
        "{}: the summary row must say what the per-form rows say",
        v.label
    );
}

// ------------------------------------------------------------- the seek table

/// A row of space-separated numbers, read as the list it is. Rows whose length
/// carries meaning (one entry per frame, one per real seek point) have their
/// length checked where they are read, so a short row fails at the row.
fn nums(v: &Vector, key: &str) -> Vec<u64> {
    v.raw(key)
        .split_whitespace()
        .map(|w| {
            w.parse()
                .unwrap_or_else(|e| panic!("{}: {key:?} row has a non-number {w:?}: {e}", v.label))
        })
        .collect()
}

/// The frame-position rows, with the length the frame count says they have.
fn per_frame(v: &Vector, key: &str) -> Vec<u64> {
    let want = v.num("frame_count") as usize;
    let got = nums(v, key);
    assert_eq!(
        got.len(),
        want,
        "{}: {key:?} has {} entries where the stream has {want} frames",
        v.label,
        got.len()
    );
    got
}

/// §8.5 / §8.5.1 read against everything the generator already holds twice: its
/// own fixed-stride parse of the block payload, and `metaflac --list`'s
/// `point N: sample_number=…, stream_offset=…, frame_samples=…` (which renders a
/// placeholder as the bare word `PLACEHOLDER` and prints none of its fields).
/// And against the frame filter's `frame_offsets` / `frame_blocksizes` rows,
/// which came from parsing every frame header in the region — so a point's
/// claims can be compared with where the frames actually are.
///
/// Three of a seek table's facts are not stored in it and have to be derived:
/// how many points there are (from the block's length, because §8.5.1's layout
/// is a fixed 18 bytes and that is the only thing that makes a seek table
/// enumerable), which frame a point names (arithmetic on the sample number,
/// sound only where the block size is fixed), and which absolute byte its offset
/// points at (relative to the first byte after the metadata — checked here, not
/// assumed: point 0 carries 0 while the first frame is hundreds of bytes in).
#[test]
fn every_seek_table_is_read_as_the_tools_and_the_frame_walk_render_it() {
    let mut with_table = 0usize;
    for v in load_vectors() {
        // Whether this stream has a table must be the same answer from two
        // places before anything is read from it: the block-list rows, and
        // whether the generator wrote the seek rows at all.
        let has_rows = v.rows.contains_key("seektable_points_total");
        let listed = v
            .blocks
            .iter()
            .any(|b| b.block_type == stream::TYPE_SEEKTABLE);
        assert_eq!(
            has_rows, listed,
            "{}: the block list and the seek rows disagree about whether this \
             stream has a seek table",
            v.label
        );
        if !has_rows {
            assert!(
                stream::seek_table(v.fixture())
                    .expect("a valid chain")
                    .is_none(),
                "{}: the block list has no SEEKTABLE and the chain walk found \
                 one anyway — a type this suite does not know is being mistaken \
                 for one",
                v.label
            );
            continue;
        }
        with_table += 1;
        let bytes = v.fixture();
        let facts = parsed(&v);

        // The block, from the chain walk's own view of the real header bytes,
        // against the block-list rows that metaflac's output produced.
        let want_block = v
            .blocks
            .iter()
            .find(|b| b.block_type == stream::TYPE_SEEKTABLE)
            .expect("checked above");
        let mut walk = Blocks::new(bytes).expect("a valid stream");
        let mut found: Vec<stream::MetadataBlock> = Vec::new();
        while let Some(b) = walk.next_block().expect("a valid chain") {
            if b.block_type == stream::TYPE_SEEKTABLE {
                found.push(b);
            }
        }
        assert_eq!(
            found.len(),
            1,
            "{}: the chain walk found {} SEEKTABLE blocks where the block list \
             has one — and a second one is a refusal this crate makes on \
             purpose, so it must not silently pick one",
            v.label,
            found.len()
        );
        let block = found[0];
        assert_eq!(
            block.offset as u64,
            v.num("seektable_block_offset"),
            "{}: the seek table is not where the rows say it is",
            v.label
        );
        assert_eq!(
            block.length as u64,
            v.num("seektable_length"),
            "{}: the seek table's payload length",
            v.label
        );
        assert_eq!(block.length, want_block.length, "block length row");

        // The convenience entry point is allowed no rule of its own: the
        // payload it hands back is the block's payload, byte for byte.
        let table = stream::seek_table(bytes)
            .expect("a valid stream's table")
            .expect("this fixture has one");
        let payload = block.payload(bytes).expect("the block is inside the file");
        assert_eq!(table.payload(), payload, "not the block's payload");
        assert_eq!(
            table.payload().len(),
            block.length,
            "the payload includes something beyond the block's bytes"
        );

        // The count, from the payload's length alone: there is no count field.
        let total = v.num("seektable_points_total") as usize;
        assert_eq!(table.len(), total, "entry count implied by the length");
        assert_eq!(
            table.payload().len(),
            stream::SEEKPOINT_LEN * total,
            "{}: {} points at {} bytes each is not the payload this stream has",
            v.label,
            total,
            stream::SEEKPOINT_LEN
        );
        assert_eq!(!table.is_empty(), total > 0, "is_empty()");
        assert_eq!(
            table.point(total),
            None,
            "indexing one past the last point is bounded"
        );

        // The points, in table order. These lists are the generator's parse of
        // the block payload and metaflac's rendering of it — generation refuses
        // to write the table unless those two agree, field for field, for every
        // real point.
        let sample_numbers = nums(&v, "seek_points_sample_numbers");
        let stream_offsets = nums(&v, "seek_points_stream_offsets");
        let point_frame_samples = nums(&v, "seek_points_frame_samples");
        let want_frame_number = nums(&v, "seek_points_frame_indices");
        let want_absolute = nums(&v, "seek_points_frame_offsets");
        let frame_offsets = per_frame(&v, "frame_offsets");
        let frame_sizes = per_frame(&v, "frame_blocksizes");
        let n_real = v.num("seektable_points_real") as usize;
        let n_placeholder = v.num("seektable_points_placeholder") as usize;
        for list in [
            &sample_numbers,
            &stream_offsets,
            &point_frame_samples,
            &want_frame_number,
            &want_absolute,
        ] {
            assert_eq!(
                list.len(),
                n_real,
                "{}: a per-point row does not have one entry per real point",
                v.label
            );
        }
        assert_eq!(
            n_real + n_placeholder,
            total,
            "{}: real points plus placeholders is not the point count",
            v.label
        );
        let placeholders: Vec<usize> = if v.raw("seek_placeholder_indices") == "none" {
            Vec::new()
        } else {
            nums(&v, "seek_placeholder_indices")
                .into_iter()
                .map(|i| i as usize)
                .collect()
        };
        assert_eq!(
            placeholders.len(),
            n_placeholder,
            "{}: the placeholder index list is not one entry per placeholder — \
             metaflac prints a placeholder as a bare word and hides every field \
             it has, so this list is the only rendering of them there is",
            v.label
        );

        let mut real = 0usize;
        let mut placeholders_seen = 0usize;
        for i in 0..table.len() {
            let p = table.point(i).expect("an index inside the table");
            if p.is_placeholder() {
                placeholders_seen += 1;
                assert!(
                    placeholders.contains(&i),
                    "{}: entry {i} is a placeholder by its sample number, but \
                     the placeholder list the tools produced does not name it",
                    v.label
                );
                assert_eq!(
                    p.first_sample,
                    stream::SEEKPOINT_PLACEHOLDER,
                    "a placeholder is a placeholder by this field alone"
                );
                if v.raw("seek_placeholder_other_fields_zero") == "true" {
                    assert_eq!(
                        (p.frame_offset, u64::from(p.frame_samples)),
                        (0, 0),
                        "{}: the row says the reference writer zeroed a \
                         placeholder's other fields; the bytes say otherwise. \
                         Table 7 calls those fields undefined, and the crate \
                         does not read them — this row is a record of what the \
                         tool wrote, not a rule",
                        v.label
                    );
                }
                assert_eq!(
                    p.frame_number(&facts.info),
                    None,
                    "{}: a placeholder names no sample, so it names no frame",
                    v.label
                );
                continue;
            }
            assert!(
                !placeholders.contains(&i),
                "{}: entry {i} is listed as a placeholder but is not one",
                v.label
            );
            // Placeholders have to come after every real point (see the parser's
            // docs), so the n-th real point is the n-th entry. Reading that back
            // is what pins the parser's ordering refusal against a real table.
            assert_eq!(
                i,
                real,
                "{}: entry {i} is real while {} earlier entries were \
                 placeholders — a table whose placeholders sit among its real \
                 points is one this crate refuses",
                v.label,
                i - real
            );
            let j = real;
            assert_eq!(
                p.first_sample, sample_numbers[j],
                "{}: point {i}'s sample number",
                v.label
            );
            assert_eq!(
                p.frame_offset, stream_offsets[j],
                "{}: point {i}'s stream offset",
                v.label
            );
            assert_eq!(
                u64::from(p.frame_samples),
                point_frame_samples[j],
                "{}: point {i}'s frame sample count",
                v.label
            );

            // Which frame the point names: the number line's frame number,
            // against the generator's count of frame starts. The row key keeps
            // the generator's historical `frame_indices` name; the value is the
            // coded frame number RFC 9639 §9.1.5 defines, and for these
            // conformant streams it equals the frame-list index.
            let frame_number = p.frame_number(&facts.info).expect(
                "both fixtures with a table state a total and a fixed \
                         block size, so a sample number has a frame number",
            );
            assert_eq!(
                frame_number, want_frame_number[j],
                "{}: point {i} (sample {}) names frame {frame_number}; counting \
                 frame starts found {}",
                v.label, p.first_sample, want_frame_number[j]
            );
            assert!(
                frame_number < frame_sizes.len() as u64,
                "{}: point {i} names frame {frame_number} of {} frames",
                v.label,
                frame_sizes.len()
            );
            assert_eq!(
                u64::from(p.frame_samples),
                frame_sizes[frame_number as usize],
                "{}: point {i} claims {} samples and the frame it names carries \
                 {} — §8.5.1's frame_samples is not decoration, and this is the \
                 fixture where a short final frame would be where a decoder that \
                 inferred the length from the declared maximum went wrong",
                v.label,
                p.frame_samples,
                frame_sizes[frame_number as usize]
            );

            // The base: the table's offsets count from the first byte after the
            // metadata. `frame_offsets` is absolute, from the frame filter.
            let absolute = facts.frame_data_offset as u64 + p.frame_offset;
            assert_eq!(
                absolute, want_absolute[j],
                "{}: point {i}'s offset resolved to {absolute}, the table says {}",
                v.label, want_absolute[j]
            );
            assert_eq!(
                absolute, frame_offsets[frame_number as usize],
                "{}: the byte this point aims at is not where the frame it \
                 names begins",
                v.label
            );
            assert_eq!(
                bytes[absolute as usize] & 0xFE,
                0xFE,
                "{}: the byte a real point aims at does not begin with a frame \
                 sync code (§8.1)",
                v.label
            );
            // And what the wrong reading would do. Taken as a file offset, this
            // entry would point at a byte that is not any frame's start in this
            // stream — wrong by the whole container, not by one byte. (If that
            // ever stopped being true of a fixture, this leg would stop being
            // able to tell the two readings apart, and it says so.)
            assert!(
                !frame_offsets.contains(&p.frame_offset),
                "{}: point {i}'s offset read from the start of the file ({}) \
                 would have named a frame start",
                v.label,
                p.frame_offset
            );
            real += 1;
        }
        assert_eq!(real, n_real, "{}: real point count", v.label);
        assert_eq!(placeholders_seen, n_placeholder, "placeholder count");

        // A lookup, for every sample the table's own points bracket: the last
        // real point at or before it — not the first one after, and not a
        // placeholder. The probe set comes from the rows.
        let declared = v.num("streaminfo_total_samples");
        for (j, &start) in sample_numbers.iter().enumerate() {
            let stop = sample_numbers
                .get(j + 1)
                .copied()
                .unwrap_or(declared.max(start + 1));
            for probe in [start, start + 1, stop - 1, declared - 1] {
                if probe < start || probe >= stop || probe >= declared {
                    continue;
                }
                let (at, p) = table
                    .find(probe)
                    .unwrap_or_else(|| panic!("{}: no anchor for sample {probe}", v.label));
                assert_eq!(
                    at, j,
                    "{}: a seek to sample {probe} answered entry {at}, which is \
                     not the last one at or before it",
                    v.label
                );
                assert_eq!(p.first_sample, start, "the anchor's own sample");
                assert!(
                    !p.is_placeholder(),
                    "{}: a lookup returned a placeholder, which names no sample",
                    v.label
                );
            }
        }
    }
    assert!(
        with_table > 0,
        "no fixture in the table has a seek table — the §8.5 rows this test \
         reads would be gone from the table, and a section with no witness is \
         not a section that was built"
    );
}
