#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

python3 "$SCRIPT_DIR/publish_movie.py" \
  "$SCRIPT_DIR/output/movie_2026-09-26_12-26-42_ivZZlz/movie_2026-09-26_12-26-42.mp4" \
  --name "Bee Movie.mp4"
