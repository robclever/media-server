#!/usr/bin/env bash

set -euo pipefail

export PATH="$PATH:/opt/homebrew/bin:/usr/local/bin"
export PYTHONUNBUFFERED=1

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OUTPUT_ROOT="${1:-$SCRIPT_DIR/output}"
TITLE="${2:-}"
RUN_NAME="movie_$(date '+%Y-%m-%d_%H-%M-%S')"
RUN_DIR="$OUTPUT_ROOT/$RUN_NAME"
MP4_FILE="$RUN_DIR/${RUN_NAME}.mp4"

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    echo "Usage: $0 [output-directory] [title-number]"
    echo
    echo "Defaults:"
    echo "  output-directory: $SCRIPT_DIR/output"
    echo "  title-number:      largest title on the disc"
    exit 0
fi

command -v python3 >/dev/null 2>&1 || {
    echo "Error: python3 is required." >&2
    exit 1
}

command -v ffmpeg >/dev/null 2>&1 || {
    echo "Error: FFmpeg is required. Install it with: brew install ffmpeg" >&2
    exit 1
}

mkdir -p "$OUTPUT_ROOT"
RUN_DIR="$(mktemp -d "$OUTPUT_ROOT/${RUN_NAME}_XXXXXX")"
MP4_FILE="$RUN_DIR/${RUN_NAME}.mp4"
LOG_FILE="$RUN_DIR/run.log"
run_logged() {
    "$@" 2>&1 | tee -a "$LOG_FILE"
}
trap 'status=$?; if (( status != 0 )); then echo "Run failed (exit $status). Diagnostics: $LOG_FILE" >&2; fi' EXIT
echo "Output folder: $RUN_DIR"
echo "Log: $LOG_FILE"

if [[ -n "$TITLE" ]]; then
    echo "Ripping selected disc title $TITLE..."
    echo "The initial disc scan can take several minutes; progress will appear below."
    run_logged python3 "$SCRIPT_DIR/bluray_rip.py" rip "$RUN_DIR" --title "$TITLE"
else
    echo "Scanning the disc and ripping its largest title..."
    echo "The initial disc scan can take several minutes; progress will appear below."
    run_logged python3 "$SCRIPT_DIR/bluray_rip.py" rip "$RUN_DIR"
fi

MKV_FILE="$(find "$RUN_DIR" -maxdepth 1 -type f -iname '*.mkv' -print -quit)"
if [[ -z "$MKV_FILE" ]]; then
    echo "Error: the rip completed but no MKV was found in $RUN_DIR" >&2
    exit 1
fi

echo
echo "Converting to MP4 at the disc's native resolution..."
run_logged python3 "$SCRIPT_DIR/bluray_rip.py" convert \
    "$MKV_FILE" \
    "$MP4_FILE"

echo
echo "Finished."
echo "Original MKV: $MKV_FILE"
echo "Native-resolution MP4: $MP4_FILE"
