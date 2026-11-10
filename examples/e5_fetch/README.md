# e5-fetch — hardware arbitration of the ROM→IWRAM fetch ratio

E5 is the only measurement that can turn the decoder's mGBA numbers into a
real-time claim. This crate is the arbitration artifact.

One synthetic ARM instruction stream (4096 dependent `add r0, r0, #1` per call)
is assembled **twice** in one image: once into `.text` (cartridge ROM) and once
into `.iwram`. The same bytes, the same inputs, one calibration; the only
variable is the address the instruction fetch reads from.

```
make native-e5-fetch-rom        # build e5-fetch.gba
```

## Running it

**mGBA (harness check):**

```sh
timeout -k 5 60 mgba-test-runner e5-fetch.gba
# *** e5 state "PREFETCH ON": rom=... iwram=... ratio_x1000=...
```

On mGBA this should read roughly **4.0 cycles/add from ROM** and **1.0 from
IWRAM** (ratio ~3.9). That validates the harness; it is not the arbitration.

**Hardware:** copy `e5-fetch.gba` to an EverDrive, boot it, and read the table on
the GBA screen. There is no serial console on hardware; the same numbers are
rendered with the committed OFL pixel font (`assets/fonts/…`). The background is
blue when the harness completed and red when it did not.

Run it on **both units** (GBA 001 and GBA SP) and note the numbers for each.

## What it prints

Nine lines, so both states fit the 240x160 screen. `ON` is prefetch enabled,
`OFF` disabled; `add` is cycles per `add`.

```
E5 FETCH ARBITRATION
WAITCNT 0x4317
ON ROM 16733 IWRAM 4248
ON ratio 3.939
ON add 4.085/1.037
OFF ROM 16749 IWRAM 4259
OFF ratio 3.932
OFF add 4.089/1.039
PASS
```

The mGBA serial lines (`*** e5 state ...`) carry the same numbers.

## The reading of record

The ratio, not the absolute cycles: mGBA says ~3.9. If hardware agrees within
~10%, the model's fetch pricing stands and the 6a-5d IWRAM placement is real.
If prefetch makes ROM fetch near-IWRAM, the 6a-5d "2.9× from one directive" is
largely an mGBA artifact and the placement decision has to be re-read.

The real-kernel confirmation is a second run: `flac-spike-6a7.gba` (the
`ctrl`/`wide`/`asm` stack image) on the same unit. Its numbers are decode-only
and mGBA-only until that run exists.
