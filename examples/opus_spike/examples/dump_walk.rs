//! dump_walk — run the PRODUCTION driver over a packet region and dump the
//! raw f32-LE walk. Host-only tool for `scripts/gen_opus_assets.py`: the
//! per-arm `align_shift` / `fold_mismatch` pins in assets.rs are measured
//! against this output.
//!
//! Why an example in the crate instead of a second decode loop inside the
//! generator: shared-consumer rule. The pins must describe what
//! [`opus_spike::probe::walk_region`] actually produces — the same
//! transcription of the walk the witness drives and the ROM proof will run.
//! A Python re-implementation of the decode loop would let the pins and the
//! driver agree with each other's mistake instead of catching it.
//!
//! Usage (run OUTSIDE the repo tree — the root .cargo/config.toml leaks the
//! thumbv4t target otherwise; the generator passes --target explicitly, and
//! the `host-tools` feature keeps this out of the ROM build and the host
//! gate):
//!   cargo run --release --manifest-path <crate>/Cargo.toml \
//!       --target <host-triple> --features host-tools --example dump_walk -- \
//!       <region.bin> <lengths.bin> <out_walk.f32le>
//!
//! lengths.bin: one u32 LE per packet, in walk order (the packet lengths;
//! offsets are the cumulative sums — the tiling the manifest pins).

use std::fs;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [region_path, lengths_path, out_path] = args.as_slice() else {
        return Err("usage: dump_walk <region.bin> <lengths.bin> <out.f32le>".into());
    };
    let region = fs::read(region_path)?;
    let lengths_bytes = fs::read(lengths_path)?;
    if lengths_bytes.len() % 4 != 0 {
        return Err("lengths.bin not a u32-LE table".into());
    }
    let mut offset = 0u32;
    let mut entries = Vec::with_capacity(lengths_bytes.len() / 4);
    for chunk in lengths_bytes.chunks_exact(4) {
        let len = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let len: u16 = len
            .try_into()
            .map_err(|_| "packet length exceeds u16 manifest field")?;
        entries.push((offset, len));
        offset += len as u32;
    }

    let mut out = fs::File::create(out_path)?;
    let mut samples = 0usize;
    opus_spike::probe::walk_region(&region, entries.iter().copied(), 48_000, 1, |window| {
        for &sample in window {
            out.write_all(&sample.to_le_bytes())
                .expect("walk dump write failed");
        }
        samples += window.len();
    })
    .map_err(|(msg, packet)| {
        format!("walk failed at packet {packet}: {msg} (dump is incomplete)")
    })?;
    println!(
        "dump_walk: {} samples from {} packets",
        samples,
        entries.len()
    );
    Ok(())
}
