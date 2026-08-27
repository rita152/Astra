#!/bin/zsh
set -euo pipefail

mkdir -p artifacts
cargo build --release --features screenshot

if [[ "${REFRESH_REFERENCES:-0}" == "1" ]]; then
  scripts/capture_references.sh
fi

for theme in dark light; do
  reference="artifacts/reference-${theme}.png"
  actual="artifacts/actual-${theme}.png"
  if [[ ! -f "$reference" ]]; then
    echo "missing reference: $reference" >&2
    exit 2
  fi

  scripts/capture_window.sh "$theme" "$actual"
  python3 scripts/pixel_compare.py "$reference" "$actual" \
    --output "artifacts/diff-${theme}" \
    --tolerance "${PIXEL_TOLERANCE:-0}" \
    --min-consistency "${MIN_CONSISTENCY:-100}" \
    --min-edge-consistency "${MIN_EDGE_CONSISTENCY:-100}"
done
