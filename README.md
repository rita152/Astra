<h1 align="center">Astra</h1>

<p align="center"><strong>Your coding agents. One familiar workspace.</strong></p>
<p align="center">A native desktop app built with Rust and GPUI.<br>Powered by Codex app-server today. Designed to welcome more coding agents tomorrow.</p>
<p align="center"><strong>English</strong> · <a href="README.zh-CN.md">简体中文</a></p>

<p align="center">
  <a href="https://github.com/rita152/Astra"><img alt="Status: early development" src="https://img.shields.io/badge/status-early_development-8b7cf8"></a>
  <a href="rust-toolchain.toml"><img alt="Rust 1.97.1" src="https://img.shields.io/badge/Rust-1.97.1-dea584"></a>
  <a href="Cargo.toml"><img alt="UI: GPUI" src="https://img.shields.io/badge/UI-GPUI-5ca9a2"></a>
  <img alt="Development platform: macOS" src="https://img.shields.io/badge/platform-macOS-999999">
</p>

![Astra's native GPUI workspace in dark mode](docs/images/astra-dark.png)

## Why Astra

Astra takes its name from the stars: different agents, brought into one shared space. Its starting idea is equally simple: **let Astra help build Astra**, using coding agents through app-server to develop the GPUI application around them.

You should be able to keep the way you work: choose a project, start a conversation, inspect changes, run commands, and continue where you left off. Astra brings that flow into an independent native application with the familiar interaction patterns of the ChatGPT desktop app. As support expands, the goal is to use coding agent products from different providers in the same workspace, without learning a new interface for each one.

- **Native UI.** Rust and GPUI render the application, including the conversation, editor, terminal, and review panels.
- **A familiar flow.** Projects, conversations, files, approvals, and changes stay close to the work.
- **Room for more agents.** A shared backend contract separates the workspace from provider-specific protocols.

**Current scope:** Astra is in active development. Only part of Codex app-server is connected today; other coding agents are planned. Some navigation and settings entries are still placeholders. Astra is an independent project, not an official OpenAI product or an extension running inside ChatGPT.

<details>
<summary>See the light theme</summary>

![Astra's native GPUI workspace in light mode](docs/images/astra-light.png)

</details>

Both images are captured from the current native application using the dedicated `GPUI Capture.app` build and a local example conversation replayed offline, without model requests. The interface currently retains some Codex labels; Astra is the project name. These are application screenshots, not design mockups.

## What you can do today

| Workflow | Available behavior |
|---|---|
| **Projects & conversations** | Create, restore, search, rename, archive, delete, move, and pin conversations. Switching conversations keeps background turns running. |
| **Talk while the agent works** | Stream responses and send additional input into an active turn. Plans, search, waits, tool activity, and file changes appear in the timeline. |
| **Approve actions** | Inspect command, file, and extra-permission requests in native cards. View automatic review outcomes and choose supported permission profiles. |
| **Work with files** | Browse the local file tree, filter paths, edit in tabs, preview Markdown and images, and follow file links to a line. |
| **Use the terminal** | Run the local shell in the conversation directory, with tabs, scrollback, text selection, and clipboard support. |
| **Review & ship changes** | Inspect Git diffs, comment on lines, stage, restore, commit, create branches, push, and open pull requests through the local `gh` CLI. |
| **Explore in side chats** | Fork temporary conversations from the main thread, with their own input, model, permissions, and stop controls. |
| **Configure Codex** | Read effective configuration and its sources, inspect managed restrictions, edit supported user settings, and verify saves against the backend. |

The exact protocol coverage and compatibility rules live in [the app-server integration table](docs/APP_SERVER_INTEGRATION.md). A visible control does not imply full support for the corresponding provider feature.

## Get started

The current development and verification platform is **macOS**. Install the Rust toolchain pinned in [rust-toolchain.toml](rust-toolchain.toml), the macOS build tools, and a logged-in Codex CLI available on `PATH`. The current integration baseline is `codex-cli 0.153.0`; see the integration table before changing CLI versions.

```bash
git clone https://github.com/rita152/Astra.git
cd Astra

rustc --version
codex --version
cargo run --release -- --theme=dark
```

Use `--theme=light` for the light theme. `cargo run -- --theme=dark` uses the optimized development profile for local iteration. Launch from the repository root for the examples below.

Astra starts `codex app-server --stdio` and uses the local Codex installation for backend configuration and sessions. Git review requires Git; creating pull requests also requires an authenticated `gh`. Python, Node.js, and Electron are development verification tools, not requirements for running the native interface.

The Cargo package and executable are still named `gpui-chat-clone`, so the existing build and capture commands continue to use that name. Prebuilt releases and cross-platform verification are not provided yet.

## Find your way around

| Action | Entry point / shortcut |
|---|---|
| Open a conversation | Sidebar projects, recent items, archive, or search |
| Toggle the terminal | Right panel → Terminal; `Ctrl+Backtick` |
| Open files | Right panel → Files; `Cmd+P` |
| Open Git review | Right panel → Review; `Ctrl+Shift+G` |
| Open a side chat | Right/bottom panel menu; `Option+Cmd+S` |
| Open settings | Account menu → Settings; `Cmd+,` |
| Send / add input to an active turn | `Enter`; `Shift+Enter` inserts a newline |
| Save a file immediately | `Cmd+S` |
| Clear the terminal | `Cmd+K` |
| Switch / close panel tabs | `Ctrl+Tab`, `Ctrl+Shift+Tab`; `Cmd+W` |

<details>
<summary>Behavior and current boundaries</summary>

- **Active-turn input:** additional input uses `turn/steer`. It is sent immediately to the active turn; there is no server-side message queue. Failed submissions can be restored with their attachments and review comments, without overwriting newer drafts or automatically starting another turn.
- **History:** completed turns collapse intermediate messages before the final answer. Added user messages retain their position and attachments. File changes are grouped by path while retaining the original patch. History comes from app-server; missing timing, plan snapshots, and automatic-review history are not invented.
- **Approvals:** concurrent requests appear in order, and a submitted card waits for server resolution. A failed response is visible and cannot be submitted twice. Inspect the original requested patch or expand and copy long commands. Keyboard navigation uses Tab/arrows, Enter, and Esc; `Shift+Esc` rejects a file request and stops the turn.
- **Permissions:** profiles are read across all backend pages and show unavailable choices with reasons. Existing-thread changes become effective only after RPC success and a matching settings notification, and affect subsequent turns. Full access requires an in-app confirmation; side chats maintain independent settings.
- **Configuration:** supported edits use versioned `config/batchWrite` followed by readback. Project and managed layers are read-only. Conflicts retain the draft; unknown outcomes are not retried automatically. Saved model, reasoning, service-tier, and personality defaults apply to future threads, without hot-updating open ones.
- **Activity:** plans support streaming, progress, copying, explicit download, and read-only file tabs. Search preserves queries and results; waits retain duration and status. Hook feedback is read-only. Automatic-review details support keyboard navigation and text selection, and follow reduced-motion preferences. Authentication recovery and deprecation-display boundaries are documented in the integration table.
- **File editing:** autosave runs about 400 ms after typing stops; undo and redo also write to disk. UTF-8 BOM, CRLF, and permissions are retained, and external edits are checked before saving. Text files are limited to 2 MiB and individual lines to 64 KiB; files are local only.
- **Git review:** scopes include the last turn, uncommitted, unstaged, staged, committed, and branch changes; branch review uses merge-base. Unified/split views, word diffs, context expansion, and line comments are available. Writes validate the worktree and index first. Restoring a newly added file preserves a backup under the worktree Git directory's `gpui-discarded/`.
- **Panel lifetime:** collapsing panels or switching conversations preserves their state. Shell sessions and temporary side chats do not survive app exit. Tabs can be reordered by dragging; closing a side chat with messages requires confirmation. Disconnected side chats remain readable and copyable.

</details>

## How it fits together

```text
GPUI application · projects · conversations · native panels
                         │
              Shared AgentBackend contract
                         │
                 Codex adapter today
                         │
               codex app-server --stdio
                         │
               Local Codex configuration & sessions
```

`ChatApp` assembles shared services and injects `AgentBackend` into the views. The backend owns project and conversation data. Astra persists UI preferences locally and does not maintain its own conversation database. Future adapters will implement the shared contract according to their actual protocols and product needs.

| Location | Responsibility |
|---|---|
| [src/agent/](src/agent/) | Backend contracts and domain types for models, messages, events, approvals, activity, configuration, and history; no GPUI dependency in domain modules. |
| [src/agent/codex/](src/agent/codex/) | Codex codecs, method validation, transport, requests, and dispatch. Connection lifecycle and routing live in `manager.rs` and `manager/`. |
| [src/workspace.rs](src/workspace.rs), [src/workspace/](src/workspace/) | Workspace state and notification merging; pagination in `loaders.rs`, atomic UI preference storage in `preferences.rs`. |
| [src/conversation/](src/conversation/) | Conversation state, event reduction, stream batching, and history restoration; no GPUI Entity or Context. |
| [src/configuration.rs](src/configuration.rs) | Configuration drafts, save receipts, and readback verification, using types from `src/agent/config.rs`. |
| [src/components/](src/components/) | Composer, timeline, approvals, files, terminal, review, and side-chat rendering and interaction. |
| [src/git_review.rs](src/git_review.rs), [src/git_review/](src/git_review/) | Git/gh operations, diffs, version validation, process cleanup, and comments, independent of GPUI and agent adapters. |
| [src/app.rs](src/app.rs), [src/app/](src/app/) | Service assembly, conversation hosts, panel mounting, project creation, and image preview. |
| [src/settings/](src/settings/), [src/media.rs](src/media.rs), [src/typography.rs](src/typography.rs) | Settings pages, shared media helpers, fonts, and typography capture. |

On macOS, UI preferences default to `~/Library/Application Support/GPUI/ui-preferences.json`. Set `GPUI_UI_PREFERENCES_PATH` to use an isolated file during verification. Backend configuration is not stored in UI preferences.

GPUI dependencies share the pinned Zed revision in [Cargo.toml](Cargo.toml). Local patches in `vendor/gpui`, `vendor/gpui_macos`, and `vendor/gpui_apple` cover text, selection, virtual lists, and Metal compositing; review these when upgrading. OpenAI Sans is loaded from an existing local ChatGPT installation when available, with a system-font fallback. The font is not distributed in this repository.

## Development & verification

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features --no-deps -p gpui-chat-clone -- -D warnings
cargo check --all-targets
cargo check --all-targets --features screenshot
cargo build --features screenshot
git diff --check
```

The warning-free Clippy requirement applies to the first-party package. Tests that make real model requests and manual scrolling benchmarks are ignored by default.

For configuration and permissions integration checks:

```bash
python3 scripts/verify_config_permissions.py --output artifacts/config-permissions-smoke
```

This uses the local CLI with an isolated configuration home and Git project under the output directory. It verifies real reads, writes, overrides, version conflicts, invalid values, null deletion, profile pagination, thread-setting receipts, and process restart. It sends no model requests and does not change the user's existing configuration.

### Capture the native app

Follow the dedicated-instance rules in [AGENTS.md](AGENTS.md). Build the latest executable, package it with the separate capture bundle ID, and launch that instance:

```bash
cargo build --features screenshot
mkdir -p 'target/GPUI Capture.app/Contents/MacOS' artifacts
cp scripts/gpui_capture_info.plist 'target/GPUI Capture.app/Contents/Info.plist'
cp target/debug/gpui-chat-clone 'target/GPUI Capture.app/Contents/MacOS/gpui-chat-clone'
codesign --force --sign - 'target/GPUI Capture.app'

GPUI_UI_PREFERENCES_PATH="$PWD/artifacts/capture-preferences.json" \
GPUI_CAPTURE_OUTPUT="$PWD/artifacts/frame.png" \
  'target/GPUI Capture.app/Contents/MacOS/gpui-chat-clone' \
  --theme=light --window-width=1440 --window-height=900
```

Enumerate apps in Computer Use, connect to **GPUI Capture**, inspect the accessibility tree and screenshot, and then navigate. Press `Cmd+Shift+F12` to save an unscaled PNG with a `.render.json` sidecar while the app keeps running. Close only the dedicated instance you started.

For automatic capture of a saved thread, append `--resume-thread=THREAD_ID --screenshot="$PWD/artifacts/resumed-thread.png"`. IDs accept a UUID or `local:<uuid>`. Optional `--resume-scroll-from-bottom=3200` sets the scroll distance; omission captures the bottom. The app waits for history, sidebar, model catalog, and three stable frames, then exits; failures and timeouts return a nonzero status. Use a thread that is not held by another active writer.

Window sizes are logical pixels; PNG resolution depends on the display's DPR. Compare the same theme, content, dimensions, DPR, and scroll position, without scaling or translating images. The older `scripts/compare_all.sh` rescales captures and is not a pixel-acceptance entry point. Keep raw captures, logs, and comparison output in `artifacts/`; curated README images live in `docs/images/`.

<details>
<summary>Additional capture and verification entry points</summary>

Full launch options are in [src/main.rs](src/main.rs).

| Option | Use |
|---|---|
| `--markdown-file=/absolute/path/to/sample.txt` | Standalone Markdown window without app-server; combine with `--window-width=480` for narrow layouts. |
| `--file-panel-root=/absolute/workspace --open-file=/absolute/file` | Real file editing, saving, and conflict checks; use disposable files. |
| `--review-root=/absolute/repository --review-filter=src/example.rs` | Real Git review; screenshots wait for the diff to load. |
| `--settings-page=appearance` | Open a settings page; slugs are in `src/settings/mod.rs`. |
| `--image-generation-ui-state=running/completed/failed/load-error` | Fixed image-generation states; completed also uses `--image-generation-path=/absolute/image.png`. |
| `--auto-approval-ui-state=inProgress/approved/denied/timedOut/aborted/strict/warning` | Automatic review; add `--auto-approval-expanded`, `--auto-approval-details-expanded`, or `--reduce-motion`. Long text and motion traces use `--auto-approval-rationale-file` and `--auto-approval-motion-output`. |
| `--runtime-ui-state=completed/running/turnless/auth-started/auth-completed/interrupted/disconnected/history/long/deprecation` | Deterministic Hook, hookPrompt, authentication, and app-notice states, without hooks or model requests. Set `GPUI_RUNTIME_AUDIT_OUTPUT` for raw state and local completion reasons. |
| `--progress-ui-state=running/streaming/completed/interrupted` | Plan, search, and wait reduction; streaming emits timed updates and completion, and running can be interrupted. |
| `--typography-specimen --typography-display=N` | Font samples and display selection; see `src/typography.rs`. |

`--approval-replay=/absolute/fixture.json` replays offline JSON-RPC through production parsing and response handling. The fixture contains an `events` array from `turn/started` through items and approval requests, with optional `cwd`, `userMessage`, `assistantMessage`, and `failWrites`. Responses are written to an adjacent `.responses.jsonl`; replay does not execute commands or modify approved files.

Visual scripts require Python 3 with Pillow, NumPy, and websocket-client. CDP scripts need Node.js with global WebSocket support. Settings checks additionally need Electron (`npm ci`) and `jq`. Capture ChatGPT references only from a dedicated debug instance using a newly allocated port:

```bash
export CHATGPT_CDP_HTTP="http://127.0.0.1:${CAPTURE_CDP_PORT:?Set a dedicated debug port}"
```

| Check | Entry point |
|---|---|
| Saved threads, both themes | `python3 scripts/capture_resume_reference.py --endpoint "$CHATGPT_CDP_HTTP" --manifest /path/to/manifest.json`; `python3 scripts/capture_resume_gpui.py --manifest /path/to/manifest.json --output artifacts/resume-alignment/actual` |
| Terminal / files | `node scripts/cdp_capture_terminal.mjs artifacts/terminal`; `node scripts/cdp_capture_file_panel.mjs artifacts/file-panel` |
| Review / side chat | `node scripts/cdp_capture_review.mjs artifacts/review-reference`; `node scripts/cdp_capture_side_chat.mjs artifacts/side-chat reference` |
| Settings matrix | `./node_modules/.bin/electron scripts/verify_chatgpt_settings.cjs`; `REFRESH_SETTINGS_REFERENCES=1 scripts/capture_settings_matrix.sh`; `python3 scripts/verify_settings_matrix.py` |
| Image generation | `python3 scripts/compare_image_generation_component.py --help`; supply measured equal-size crops and DPR. |
| History diagnostics | `python3 scripts/audit_resume_rendering.py --help`; rollout files are for offline diagnostics only. |

Thread manifests are JSON arrays with `id`, `title`, and `slug`. Thread/settings comparisons use 1440×900 at DPR 1 on a 1× display; the settings matrix covers 21 pages in both themes. Git-write verification uses temporary repositories and a local bare remote; PR command-chain checks use a `gh` test double.

```bash
GPUI_MARKDOWN_BENCH_FILE=docs/APP_SERVER_INTEGRATION.md \
  cargo test markdown_preview_scroll_timings -- --ignored --nocapture
GPUI_DIFF_BENCH_PATCH=/absolute/path/to/long.diff \
GPUI_DIFF_BENCH_OUTPUT=/tmp/gpui-diff-timings.json \
  cargo test long_diff_scroll_timings -- --ignored --nocapture

cargo test -p gpui_macos --lib --features font-kit typography_
cargo test -p gpui_apple --lib compositing_tests
node scripts/cdp_capture_chatgpt_typography.mjs --artifact-dir=artifacts/typography/live
node scripts/cdp_capture_typography_specimen.mjs artifacts/typography 1
'target/GPUI Capture.app/Contents/MacOS/gpui-chat-clone' \
  --typography-specimen --screenshot=artifacts/typography/gpui-1x.png
python3 scripts/compare_typography.py \
  artifacts/typography/electron-1x.png artifacts/typography/gpui-1x.png \
  --output=artifacts/typography/comparison-1x.json
```

Scrolling benchmarks measure event/layout time in a GPUI test window, not screen FPS. Typography comparison counts glyph pixels only. For 2×, set the CDP scale to `2`, use a real Retina display for GPUI, and compare with `--dpr=2`. Save translucent captures separately: add `--translucent` to CDP/comparison scripts and `--typography-translucent` to GPUI. The comparison composites both alpha encodings onto the same background.

</details>

## Where Astra is heading

- Support more coding agent providers through their real protocols and capabilities.
- Unify local agent discovery, configuration, launch, sessions, and runtime status.
- Keep familiar conversation and review workflows as the choice of agents grows.
- Expand Codex coverage where it serves the product, with explicit compatibility boundaries.

These are directions, not shipped integrations or release-date commitments. For contributions, begin with [the repository conventions](AGENTS.md) and [the app-server integration table](docs/APP_SERVER_INTEGRATION.md). Keep the English and Chinese READMEs aligned when behavior changes.
