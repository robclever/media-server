#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

python3 "$SCRIPT_DIR/publish_movie.py" \
  "$SCRIPT_DIR/output/movie_2026-09-27_10-13-33_Wwkae3/movie_2026-09-27_10-13-33.mp4" \
  --name "Mr Peabody and Sherman.mp4"
