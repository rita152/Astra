#!/bin/zsh
set -euo pipefail

root="${0:A:h:h}"
cd "$root"

refresh="${REFRESH_SETTINGS_REFERENCES:-0}"
manifest="chat-reference/settings/manifest.json"
canonical_slugs='[
  "general-settings", "import", "profile", "appearance", "voice", "agent",
  "personalization", "pets", "keyboard-shortcuts", "usage", "computer-use",
  "chronicle", "appshots", "plugins-settings", "browser-use", "hooks-settings",
  "connections", "git-settings", "local-environments", "worktrees", "data-controls"
]'
mkdir -p artifacts/settings-matrix/{reference,actual,diff}/{light,dark}
cargo build --release --features screenshot

# A minimal low-resolution app bundle makes AppKit expose a stable 1x backing
# scale on Retina Macs. Launching the bare executable can race between a 1x and
# 2x CAMetalLayer even though its logical window remains 1440x900.
gpui_capture_app="target/GPUI Capture.app"
gpui_capture_executable="$gpui_capture_app/Contents/MacOS/gpui-chat-clone"
mkdir -p "$gpui_capture_app/Contents/MacOS"
cp scripts/gpui_capture_info.plist "$gpui_capture_app/Contents/Info.plist"
cp target/release/gpui-chat-clone "$gpui_capture_executable"

png_is_1440x900() {
  local dimensions
  dimensions="$(sips -g pixelWidth -g pixelHeight "$1" 2>/dev/null \
    | awk '/pixelWidth/{width=$2} /pixelHeight/{height=$2} END{print width "x" height}')"
  [[ "$dimensions" == "1440x900" ]]
}

capture_electron_reference() {
  local theme="$1"
  local slug="$2"
  local output="$3"
  local attempt
  for attempt in {1..5}; do
    # Set DPR before Chromium starts; the capture program verifies it again
    # after the page has settled and rejects any non-1x result.
    if ./node_modules/.bin/electron --force-device-scale-factor=1 \
      scripts/electron_settings_reference.cjs \
      "--theme=$theme" "--slug=$slug" "--output=$output" >/dev/null; then
      return 0
    fi
    echo "retrying Electron reference ($attempt/5): $theme/$slug" >&2
  done
  return 1
}

if ! jq -e --argjson expected "$canonical_slugs" '
  ([.[] | select(type == "object" and has("slug")) | .slug]) as $slugs
  | $slugs == $expected
    and (.[-1].panelCount == 21)
    and (.[-1].themes == ["light", "dark"])
' "$manifest" >/dev/null; then
  echo "invalid settings manifest: expected the canonical ordered 21 slugs and light/dark metadata" >&2
  exit 2
fi

slugs=(${(f)"$(jq -r '.[]' <<< "$canonical_slugs")"})
for theme in light dark; do
  html_count=$(find "chat-reference/settings/$theme" -mindepth 1 -maxdepth 1 \
    | wc -l | tr -d ' ')
  if (( html_count != ${#slugs} )); then
    echo "invalid Electron HTML set for $theme: found $html_count, expected ${#slugs}" >&2
    exit 2
  fi
  for slug in $slugs; do
    if [[ ! -f "chat-reference/settings/$theme/$slug.html" ]]; then
      echo "missing Electron HTML reference: $theme/$slug.html" >&2
      exit 2
    fi
  done
done

captured_pairs=0
for theme in light dark; do
  for slug in $slugs; do
    reference="artifacts/settings-matrix/reference/$theme/$slug.png"
    actual="artifacts/settings-matrix/actual/$theme/$slug.png"
    reference_tmp="target/settings-reference-capture.png"
    actual_tmp="target/settings-actual-capture.png"
    if [[ "$refresh" == "1" || ! -f "$reference" ]]; then
      rm -f -- "$reference_tmp"
      capture_electron_reference "$theme" "$slug" "$reference_tmp"
      if [[ ! -s "$reference_tmp" ]] || ! png_is_1440x900 "$reference_tmp"; then
        echo "Electron did not produce a fresh 1440x900 reference for $theme/$slug" >&2
        exit 2
      fi
      mv -f -- "$reference_tmp" "$reference"
    fi
    rm -f -- "$actual_tmp"
    "$gpui_capture_executable" "--theme=$theme" "--settings-page=$slug" \
      "--screenshot=$actual_tmp" >/dev/null
    if [[ ! -s "$actual_tmp" ]] || ! png_is_1440x900 "$actual_tmp"; then
      echo "GPUI did not produce a fresh 1440x900 actual for $theme/$slug" >&2
      exit 2
    fi
    mv -f -- "$actual_tmp" "$actual"
    if [[ ! -s "$reference" || ! -s "$actual" ]] \
      || ! png_is_1440x900 "$reference" \
      || ! png_is_1440x900 "$actual"; then
      echo "missing, empty, or non-1440x900 settings capture for $theme/$slug" >&2
      exit 2
    fi
    (( captured_pairs += 1 ))
  done
done

expected_pairs=$(( ${#slugs} * 2 ))
reference_count=$(find artifacts/settings-matrix/reference/light artifacts/settings-matrix/reference/dark \
  -maxdepth 1 -type f -name '*.png' | wc -l | tr -d ' ')
actual_count=$(find artifacts/settings-matrix/actual/light artifacts/settings-matrix/actual/dark \
  -maxdepth 1 -type f -name '*.png' | wc -l | tr -d ' ')
if (( captured_pairs != expected_pairs || reference_count != expected_pairs || actual_count != expected_pairs )); then
  echo "incomplete settings matrix: pairs=$captured_pairs references=$reference_count actuals=$actual_count expected=$expected_pairs" >&2
  exit 2
fi

echo "Confirmed $expected_pairs Electron references and captured $expected_pairs GPUI actuals (${#slugs} pages x 2 themes)."
