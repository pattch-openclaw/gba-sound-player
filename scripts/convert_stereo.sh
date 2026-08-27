#!/bin/bash

# convert_stereo.sh
# Converts a source audio file into a stereo WAV file formatted
# for the GBA agb software mixer.

if [ -z "$1" ]; then
  echo "Usage: $0 <input_audio_file> [sample_rate] [output_file]"
  echo "  sample_rate:   Target sample rate in Hz (default: 32768)"
  echo "  output_file:   Target output .wav file (default: test.wav)"
  echo ""
  echo "Example:"
  echo "  $0 my_song.mp3 32768 my_song.wav"
  echo "  -> Generates my_song.wav at 32768 Hz (stereo, 8-bit PCM)"
  exit 1
fi

INPUT_FILE="$1"
# Default to 32768 Hz if arg 2 is empty
SAMPLE_RATE="${2:-32768}"
OUTPUT_FILE="${3:-assets/sound/test.wav}"

echo "Converting '$INPUT_FILE' for GBA..."
echo "Sample Rate: $SAMPLE_RATE Hz"
echo "Output:      $OUTPUT_FILE"
echo "----------------------------------------"

# Run FFmpeg:
# -y overrides output files without prompting
# -hide_banner makes output slightly cleaner
# -ac 2 forces stereo
# -ar sets the sample rate
# -c:a pcm_u8 sets the audio codec to 8-bit unsigned PCM (standard for 8-bit WAV)
ffmpeg -i "$INPUT_FILE" -y -hide_banner \
  -ac 2 -ar "$SAMPLE_RATE" -c:a pcm_u8 "$OUTPUT_FILE"

if [ $? -eq 0 ]; then
  echo "Conversion complete! Output saved to $OUTPUT_FILE"
else
  echo "FFmpeg encountered an error."
  exit 1
fi
