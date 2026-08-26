#!/bin/bash

# convert_stereo.sh
# Converts a stereo audio file into two raw signed 8-bit PCM binary files
# formatted for GBA Direct Sound (DMA playback).

if [ -z "$1" ]; then
  echo "Usage: $0 <input_audio_file> [sample_rate] [output_prefix]"
  echo "  sample_rate:   Target sample rate in Hz (default: 65536)"
  echo "  output_prefix: Prefix for output files (default: left.bin / right.bin)"
  echo ""
  echo "Example:"
  echo "  $0 my_song.wav 44100 bgm"
  echo "  -> Generates bgm_left.bin and bgm_right.bin at 44100 Hz"
  exit 1
fi

INPUT_FILE="$1"
# Default to 65536 Hz if arg 2 is empty
SAMPLE_RATE="${2:-65536}"
OUTPUT_PREFIX="$3"

# Determine output filenames based on whether a prefix was provided
if [ -z "$OUTPUT_PREFIX" ]; then
  OUT_LEFT="left.bin"
  OUT_RIGHT="right.bin"
else
  OUT_LEFT="${OUTPUT_PREFIX}_left.bin"
  OUT_RIGHT="${OUTPUT_PREFIX}_right.bin"
fi

echo "Converting '$INPUT_FILE' for GBA..."
echo "Sample Rate: $SAMPLE_RATE Hz"
echo "Outputs:     $OUT_LEFT, $OUT_RIGHT"
echo "----------------------------------------"

# Run FFmpeg:
# -y overrides output files without prompting
# -hide_banner makes output slightly cleaner
ffmpeg -i "$INPUT_FILE" -y -hide_banner \
  -filter_complex "[0:a]loudnorm,channelsplit[left][right]" \
  -map "[left]"  -ar "$SAMPLE_RATE" -c:a pcm_s8 -f s8 "$OUT_LEFT" \
  -map "[right]" -ar "$SAMPLE_RATE" -c:a pcm_s8 -f s8 "$OUT_RIGHT"

if [ $? -eq 0 ]; then
  echo "Conversion complete!"
else
  echo "FFmpeg encountered an error."
  exit 1
fi
