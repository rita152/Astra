#!/bin/zsh
set -euo pipefail

theme="${1:-dark}"
output="${2:-artifacts/actual-${theme}.png}"
pixel_skin="${3:-skin}"
mkdir -p "${output:h}"

if [[ "$pixel_skin" == "native" ]]; then
  GPUI_NATIVE_RENDER=1 target/release/gpui-chat-clone "--theme=${theme}" "--screenshot=${output}"
else
  target/release/gpui-chat-clone "--theme=${theme}" "--screenshot=${output}"
fi

dimensions="$(sips -g pixelWidth -g pixelHeight "$output" 2>/dev/null)"
if [[ "$dimensions" != *"pixelWidth: 1440"* || "$dimensions" != *"pixelHeight: 900"* ]]; then
  sips --resampleHeightWidth 900 1440 "$output" >/dev/null
fi
