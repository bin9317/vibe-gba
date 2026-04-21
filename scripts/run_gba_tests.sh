#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="${1:-$ROOT/artifacts/gba-tests/batch1}"

ROMS=(
  "third_party/gba-tests/arm/arm.gba"
  "third_party/gba-tests/thumb/thumb.gba"
  "third_party/gba-tests/memory/memory.gba"
  "third_party/gba-tests/bios/bios.gba"
)

mkdir -p "$OUT_DIR"

for rom in "${ROMS[@]}"; do
  name="$(basename "$rom" .gba)"
  dest="$OUT_DIR/$name"
  rm -rf "$dest"
  mkdir -p "$dest"

  echo "==> $rom"
  cargo run --release -q -p cli_debugger --bin snapshot_frames -- \
    "$ROOT/$rom" \
    --output-dir "$dest" \
    --image-format png \
    --snapshot-every-frames 120 \
    --max-frames 720
done
