# GPUI

基于 Rust 与 GPUI 的原生桌面 Agent 应用，为本机 coding agent 提供统一的项目、会话与运行状态入口。当前仅接入 Codex app-server 的部分能力；界面参考桌面 ChatGPT，由原生组件绘制。其他 agent、部分导航入口和设置项尚未接入后端。

## 运行

当前开发与验收环境为 macOS。需要 [Rust 1.97.1](rust-toolchain.toml)，以及已安装、已登录且位于 `PATH` 的 Codex CLI；应用通过 `codex app-server --stdio` 启动后端。

```bash
cargo run --release -- --theme=dark
cargo run --release -- --theme=light
```

默认使用深色主题和启动时的工作目录。普通 `cargo run` 使用优化过的开发 profile。Git 审查需要本机 Git，创建 PR 另需已登录的 `gh`；Node.js、Python 和 Electron 仅用于开发验证。

## 功能与入口

| 功能 | 入口 | 当前行为 |
|---|---|---|
| 会话 | 侧栏项目、最近、归档、搜索 | 创建、恢复、重命名、归档、删除、移动及置顶；切换会话保留后台轮次 |
| 终端 | 右侧“终端”或 `Ctrl+反引号` | 在会话工作目录启动本机 `$SHELL`，支持多标签、回看、选择与复制粘贴；`Cmd+K` 清屏 |
| 文件 | 右侧“文件”、底部菜单或 `Cmd+P` | 文件树、路径筛选、多标签编辑、Markdown 预览、图片查看，以及聊天文件链接定位 |
| 审查 | 右侧“审查”、文件卡“审核”或 `Ctrl+Shift+G` | 查看历史补丁和 Git diff，添加评论，暂存、还原、提交、建分支、推送及创建 PR |
| 侧边聊天 | 右侧入口、底部菜单或 `⌥⌘S` | 基于已有主会话创建临时对话；独立输入、模型、权限、轮次和中断；支持多标签与文件上下文 |

- **历史与消息**：已完成轮次将最终答复之前的过程消息折叠，支持点击及 Enter／Space 展开。历史文件变更按路径汇总，保留原始 patch；Markdown 支持本地图片、带行号的文件链接和表格。
- **审批**：命令、终端输入、文件修改与附加权限使用原生审批卡，支持一次允许、会话允许及服务端提供的执行／网络策略。并发请求依次显示，提交后等待服务端释放；失败可见且不可重复提交，可停止当前轮次退出错误状态。文件行查看该次请求的原始补丁，长命令可展开、滚动、选择和复制。`Tab`／方向键导航，`Enter` 激活，`Esc` 关闭菜单或拒绝；文件审批的 `Shift+Esc` 拒绝并停止轮次。
- **文件保存**：停止输入约 400 ms 后自动保存，`Cmd+S` 立即保存；撤销／重做也写回磁盘。保留 UTF-8 BOM、CRLF 和权限，保存前检查外部修改。文本上限为 2 MiB、单行 64 KiB；仅访问本机文件系统。验收编辑行为时使用专用测试文件。
- **Git 审查**：范围包括上一轮、未提交、未暂存、已暂存、已提交和分支；分支使用 merge-base。支持统一／拆分差异、文字差异、上下文展开和逐行评论；评论可单独或随提示词发送。写操作前校验工作区与 index，失败保留输入；还原新增文件时备份到 worktree Git 目录下的 `gpui-discarded/`。PR 创建使用本机 `gh`。
- **面板生命周期**：收起面板或切换主会话保留终端、文件、审查和侧边聊天状态；应用退出后不恢复 shell 或临时侧边聊天。侧边标签支持拖动排序、`Ctrl+Tab`／`Ctrl+Shift+Tab` 切换、`Cmd+W` 关闭；有消息时确认关闭，连接失效后保留消息供查看和复制。

## 架构

`ChatApp` 装配共享服务，通过 `AgentBackend` 注入视图。项目和会话数据来自后端；本地只持久化 UI 偏好，不维护会话数据库。

| 位置 | 职责 |
|---|---|
| [src/agent/](src/agent/) | 后端契约、能力、模型、消息、活动、事件、审批与历史类型；领域模块不依赖 GPUI 或具体适配器 |
| [src/agent/codex/](src/agent/codex/) | Codex 编解码、方法校验、进程通信、请求响应与事件派发 |
| [src/agent/codex/manager/](src/agent/codex/manager/) | 共享连接、轮次路由、目录与工作区请求、临时侧边线程；生命周期入口在 `manager.rs` |
| [src/workspace.rs](src/workspace.rs)、[src/workspace/](src/workspace/) | 工作区状态与通知合并；`loaders.rs` 分页读取，`preferences.rs` 原子保存 UI 偏好 |
| [src/conversation/](src/conversation/) | 会话状态、事件归约、流式批处理与历史恢复；不持有 GPUI Entity／Context，仍复用组件中的展示数据类型 |
| [src/components/](src/components/) | Composer 输入与交互、Home 时间线、审批、文件、终端、审查及侧边聊天；各功能的渲染与测试就近维护 |
| [src/git_review.rs](src/git_review.rs)、[src/git_review/](src/git_review/) | 不依赖 GPUI 或具体 agent 的 Git/gh 操作、diff、版本校验、命令回收和评论数据 |
| [src/app.rs](src/app.rs)、[src/app/](src/app/) | 服务装配、会话 host、面板挂载、项目创建与全局图片预览 |
| [src/settings/](src/settings/) | 设置导航、页面规格和原生页面；Chronicle 插画位于 `assets/illustrations/` |
| [src/media.rs](src/media.rs)、[src/typography.rs](src/typography.rs) | 图片尺寸读取、字体初始化与字体验收入口 |

macOS 的 UI 偏好默认保存到 `~/Library/Application Support/GPUI/ui-preferences.json`，可用 `GPUI_UI_PREFERENCES_PATH` 指定验收专用文件。GPUI 依赖固定在 [Cargo.toml](Cargo.toml) 中的同一 Zed revision；`vendor/gpui`、`vendor/gpui_macos`、`vendor/gpui_apple` 保留文本、选择、虚拟列表和 Metal 合成修正，升级时一并复核。OpenAI Sans 从本机 ChatGPT 安装读取；未安装时回退系统字体，仓库不分发该字体。

## 代码验证

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features --no-deps -p gpui-chat-clone -- -D warnings
cargo check --all-targets
cargo check --all-targets --features screenshot
cargo build --features screenshot
git diff --check
```

Clippy 零告警要求限于第一方包。`cargo test` 默认忽略真实模型请求和两项手动滚动基准；基准测量 GPUI 测试窗口的事件与布局耗时，不代表屏幕 FPS：

```bash
GPUI_MARKDOWN_BENCH_FILE=docs/APP_SERVER_INTEGRATION.md \
  cargo test markdown_preview_scroll_timings -- --ignored --nocapture
GPUI_DIFF_BENCH_PATCH=/absolute/path/to/long.diff \
  GPUI_DIFF_BENCH_OUTPUT=/tmp/gpui-diff-timings.json \
  cargo test long_diff_scroll_timings -- --ignored --nocapture
```

## 界面验证

先构建独立 bundle，再按 [AGENTS.md](AGENTS.md) 的 Computer Use 规则验收。截图需要 `screenshot` feature；下例恢复指定线程，运行前替换 `THREAD_ID`：

```bash
cargo build --features screenshot
mkdir -p 'target/GPUI Capture.app/Contents/MacOS'
cp scripts/gpui_capture_info.plist 'target/GPUI Capture.app/Contents/Info.plist'
cp target/debug/gpui-chat-clone 'target/GPUI Capture.app/Contents/MacOS/gpui-chat-clone'
codesign --force --sign - 'target/GPUI Capture.app'
GPUI_UI_PREFERENCES_PATH="$PWD/artifacts/capture-preferences.json" \
  'target/GPUI Capture.app/Contents/MacOS/gpui-chat-clone' \
  --theme=light --window-width=1440 --window-height=900 \
  --resume-thread=THREAD_ID --resume-scroll-from-bottom=3200 \
  --screenshot="$PWD/artifacts/resumed-thread.png"
```

线程 ID 接受原始 UUID 或 `local:<uuid>`。恢复截图等待历史、侧栏、模型目录与三个稳定绘制帧，失败或超时返回非零状态；PNG 旁生成 `.png.render.json` 供逐项核对。省略 `--resume-scroll-from-bottom` 停在底部；省略 `--screenshot` 可手动交互。

窗口尺寸为逻辑像素，PNG 尺寸取决于显示器缩放。对照必须使用相同主题、内容、窗口尺寸、DPR 和滚动位置；禁止缩放或平移图片提高分数。旧 `scripts/compare_all.sh` 经 `capture_window.sh` 缩放输出，不作为像素验收入口。

截图构建还支持以下入口，完整参数见 [src/main.rs](src/main.rs)：

| 参数 | 用途 |
|---|---|
| `--markdown-file=/absolute/path/to/sample.txt` | 独立 Markdown 窗口，无需 app-server；可搭配 `--window-width=480` 检查窄窗 |
| `--file-panel-root=/absolute/workspace`、`--open-file=/absolute/file` | 使用真实文件验收编辑、保存和冲突 |
| `--review-root=/absolute/repository`、`--review-filter=src/example.rs` | 加载真实 Git 仓库；截图等待 diff 就绪 |
| `--settings-page=appearance` | 直接打开设置页，页面列表见 `src/settings/mod.rs` |
| `--image-generation-ui-state=running/completed/failed/load-error` | 固定图像生成状态；完成态另传 `--image-generation-path=/absolute/image.png` |
| `--typography-specimen`、`--typography-display=N` | 字体样本与目标显示器；实现及专用参数见 `src/typography.rs` |

截图构建还提供离线审批协议回放：`--approval-replay=/absolute/fixture.json`。fixture 含 `events`（从 `turn/started` 到 item 与审批请求的 JSON-RPC 消息数组），可选 `cwd`、`userMessage`、`assistantMessage`、`failWrites`。回放复用生产解析、注册表、响应和 resolved 路径，响应写入相邻的 `.responses.jsonl`；不会执行命令或修改被审批文件。

交互后需要原始像素图时，在启动 Capture 前设置 `GPUI_CAPTURE_OUTPUT=/absolute/artifacts/frame.png`，再通过 Computer Use 按 `Cmd+Shift+F12`。应用保存未缩放 PNG 及窗口大小／DPR 的 `.render.json`，并继续运行，便于核查菜单、选择和滚动状态。仅 `screenshot` 构建启用此快捷键。

### 专项入口

视觉脚本需要 Python 3，以及 Pillow、NumPy、websocket-client；CDP 脚本需要支持全局 WebSocket 的 Node.js。设置矩阵另需 `npm ci` 安装 Electron，以及本机 `jq`。参考 ChatGPT 必须使用专用调试实例；将其新建端口填入 `CAPTURE_CDP_PORT`：

```bash
export CHATGPT_CDP_HTTP="http://127.0.0.1:${CAPTURE_CDP_PORT:?设置专用调试端口}"
```

| 专项 | 入口 |
|---|---|
| 真实线程双主题对照 | `python3 scripts/capture_resume_reference.py --endpoint "$CHATGPT_CDP_HTTP" --manifest /path/to/manifest.json`；`python3 scripts/capture_resume_gpui.py --manifest /path/to/manifest.json --output artifacts/resume-alignment/actual` |
| 终端／文件样式 | `node scripts/cdp_capture_terminal.mjs artifacts/terminal`；`node scripts/cdp_capture_file_panel.mjs artifacts/file-panel` |
| 审查／侧边聊天样式 | `node scripts/cdp_capture_review.mjs artifacts/review-reference`；`node scripts/cdp_capture_side_chat.mjs artifacts/side-chat reference` |
| 设置矩阵 | `./node_modules/.bin/electron scripts/verify_chatgpt_settings.cjs`；`REFRESH_SETTINGS_REFERENCES=1 scripts/capture_settings_matrix.sh`；`python3 scripts/verify_settings_matrix.py` |
| 图像生成组件 | `python3 scripts/compare_image_generation_component.py --help`，按实测位置传入等尺寸 crop 和 DPR |
| 历史逐项诊断 | `python3 scripts/audit_resume_rendering.py --help`，对照 rollout、历史响应、DOM 与原生 render JSON；rollout 仅用于离线诊断 |

线程 manifest 为含 `id`、`title`、`slug` 的 JSON 数组。线程对照与设置矩阵使用 1440×900、DPR 1，需在 1× 显示器捕获 GPUI；设置矩阵覆盖 21 页 × 两种主题。Git 写操作使用临时仓库和本机 bare remote，PR 命令链使用 `gh` 测试替身。

字体与 Metal 合成验证：

```bash
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

字体比较只统计字形像素。2× 样本将 CDP 参数改为 `2`，GPUI 选择真实 Retina 显示器，比较时传 `--dpr=2`。半透明对照另存目录：CDP 和比较脚本加 `--translucent`，GPUI 加 `--typography-translucent`；比较脚本将两端不同的 alpha 编码合成到同一底色后计算。

## 文档

- [AGENTS.md](AGENTS.md)：项目背景、代码边界与工作约定。
- [docs/APP_SERVER_INTEGRATION.md](docs/APP_SERVER_INTEGRATION.md)：Codex app-server 全量方法、接入状态与兼容边界。
