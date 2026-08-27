#!/bin/zsh
set -euo pipefail

mkdir -p artifacts
for theme in dark light; do
  node_modules/.bin/electron scripts/electron_reference.cjs \
    "--theme=${theme}" \
    "--output=artifacts/reference-${theme}.png"
done
