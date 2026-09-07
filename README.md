# GPUI Codex chat clone

这是一个纯 GPUI、组件化的 Codex 桌面界面。应用不嵌入 HTML/WebView，也不把整屏截图用作产品 UI；侧栏、工作区、Composer、设置、审批和 Diff Review 均由 GPUI 原生组件、系统字体与图标资源绘制。

## 启动

项目使用 Rust 1.97.1。首次拉取后可直接运行：

```bash
cargo run --release -- --theme=dark
cargo run --release -- --theme=light
```

普通 `cargo run` 也会以适合 GPUI 实时渲染的 profile 编译。需要从 Metal 场景导出确定性截图时显式启用 feature：

```bash
cargo build --release --features screenshot
target/release/gpui-chat-clone \
  --theme=dark \
  --screenshot=artifacts/actual-dark.png
```

恢复指定线程并等待历史内容稳定后再截图：

```bash
target/release/gpui-chat-clone \
  --theme=light \
  --window-width=1470 \
  --window-height=923 \
  --resume-thread=<thread-id> \
  --resume-scroll-from-bottom=3200 \
  --screenshot=artifacts/resumed-thread-light.png
```

该路径会等待线程历史、侧栏和模型目录完成 hydration，并额外等待三个实际绘制的稳定帧；历史加载失败或超时会以非零状态退出，截图输出目录会自动创建，并在 PNG 旁写入 `.png.render.json`，记录同一渲染路径的全部轮次、工具组与 item id，供逐项核对。
`<thread-id>` 可直接使用 ChatGPT App 侧栏 DOM 中的 `local:<uuid>`，也可使用 Codex app-server 的原始 UUID。
窗口宽高使用逻辑像素；PNG 的物理像素尺寸会跟随当前显示器缩放倍率。
`--resume-scroll-from-bottom` 为可选的逻辑像素距离，用于让 light/dark 捕获稳定落在同一段历史内容；省略时截图停在会话底部。

已完成的恢复轮次将最后一条最终答复之前的过程消息与工具活动收进“用时 …”折叠区，支持点击、Tab 聚焦和 Enter／Space 展开；展开后的工具仍按独立虚拟列表项渲染。历史缺少消息 `phase` 时只将最后一条未标注的助手消息视为最终答复；运行中、失败和中断轮次保持活动可见。

恢复轮次的已完成文件补丁会按路径汇总为文件卡，默认显示前三项，支持展开全部文件及打开真实 diff 审核。Markdown 本地图片保留缩略图和打开原图入口；文件链接保留行号与类型图标，表格按内容分配列宽并填满最小正文宽度。

真实恢复线程的双主题对照可使用以下脚本。manifest 是包含 `id`、`title`、`slug` 的 JSON 数组；CDP 脚本仅在专用 ChatGPT 调试实例中导航已有线程、切换主题和展开控件，不发送提示词。参考视口为 1440×900、DPR 1，脚本将侧栏拖到 240px；GPUI 也须在 1× 显示器上捕获，不能缩放图片后宣称像素对齐。

```bash
python3 scripts/capture_resume_reference.py --manifest /path/to/manifest.json
cargo build --features screenshot
mkdir -p 'target/GPUI Capture.app/Contents/MacOS'
cp scripts/gpui_capture_info.plist 'target/GPUI Capture.app/Contents/Info.plist'
cp target/debug/gpui-chat-clone 'target/GPUI Capture.app/Contents/MacOS/gpui-chat-clone'
codesign --force --sign - 'target/GPUI Capture.app'
python3 scripts/capture_resume_gpui.py \
  --manifest /path/to/manifest.json --output artifacts/resume-alignment/actual
```

对单个真实线程做逐条诊断时，可先运行 `scripts/capture_resume_activity_audit.py --output <目录>` 捕获当前 ChatGPT 线程全部五轮的双主题活动，再用 `scripts/audit_resume_rendering.py --jsonl <只读 rollout 路径> --history <thread/read 响应 JSON> --dom <捕获的 dom-audit.json> --native <GPUI 截图的 .png.render.json> --output <审计结果 JSON>` 建立每条记录的对应关系。JSONL 仅用于离线诊断；应用恢复仍通过 app-server 读取。

仅验收 Markdown 排版时，截图构建支持 `--markdown-file=/absolute/path/to/sample.txt`，直接使用生产 Markdown 渲染器打开独立窗口，无需等待 app-server。可组合 `--theme=light` / `--theme=dark`、`--window-width=480` 和 `--screenshot=artifacts/markdown.png`；省略 `--screenshot` 可手动检查表格横向滚动、链接和窄窗布局。仍使用上面的 `GPUI Capture.app` 独立 bundle 验收。

图像生成组件可用同一条确定性截图路径复核。`running`、`completed`、`failed`、`load-error` 分别固定加载、成功、额度失败和文件加载失败状态；成功态传入真实输出文件：

```bash
target/release/gpui-chat-clone \
  --theme=light \
  --window-width=1470 \
  --window-height=923 \
  --image-generation-ui-state=completed \
  --image-generation-path=/absolute/path/to/generated.png \
  --screenshot=artifacts/image-generation-completed-light.png
```

使用相同 CSS 尺寸、DPR、主题和输出文件完成两侧截图后，以各自组件左上角和相同宽高执行数值比较；脚本会报告 crop、归一化 MAE 相似度、SSIM、PSNR、精确像素比例及 99.5% 阈值结果：

```bash
python3 scripts/compare_image_generation_component.py \
  artifacts/chatgpt-image-generation.png \
  artifacts/gpui-image-generation.png \
  --reference-crop=502,209,480,480 \
  --actual-crop=488,205,480,480 \
  --dpr=2 \
  --threshold=0.995 \
  --output-json=artifacts/image-generation-comparison.json \
  --diff=artifacts/image-generation-diff.png
```

## 验证

Rust 基础校验：

```bash
cargo fmt --check
cargo test
cargo check --all-targets
```

macOS 字体渲染对照使用本机 ChatGPT 的真实 CDP 样式。应用按 CSS 的灰度抗锯齿、430 默认字重、中文回退和分数行高绘制；显式 400/500/600 字重仍分别保留。`vendor/gpui`、`vendor/gpui_macos` 与 `vendor/gpui_apple` 固定于 Cargo 中同一 Zed revision，在文本选择、字体缓存、栅格化、行高/基线及 Metal 透明度合成处保留兼容修正，升级 GPUI 时须同时复核这些改动。品牌标题从本机 `/Applications/ChatGPT.app` 或 `~/Applications/ChatGPT.app` 读取原始 OpenAI Sans 字体到内存；仓库不分发该字体，未安装时使用系统字体回退。

字体专用验证（`CHATGPT_CDP_HTTP` 指向已开启的本机调试端口）：

```bash
cargo test -p gpui_macos --lib --features font-kit typography_
cargo build --features screenshot
CHATGPT_CDP_HTTP=http://127.0.0.1:9222 \
  node scripts/cdp_capture_chatgpt_typography.mjs --artifact-dir=artifacts/typography/live
CHATGPT_CDP_HTTP=http://127.0.0.1:9222 \
  node scripts/cdp_capture_typography_specimen.mjs artifacts/typography 1
target/debug/gpui-chat-clone --typography-specimen \
  --screenshot=artifacts/typography/gpui-1x.png
python3 scripts/compare_typography.py \
  artifacts/typography/electron-1x.png artifacts/typography/gpui-1x.png \
  --output=artifacts/typography/comparison-1x.json
```

样本是两端实时绘制的相同文字，覆盖 light/dark、中英文、代码、emoji 和品牌字形；比较只计算字形像素，不以大块空白背景稀释误差，不缩放图片或搜索平移来提高分数。2× 对照需将 CDP 样本最后参数改为 `2`，并通过 `--typography-display=<显示器索引>` 在真实 Retina 屏捕获 GPUI，比较时传 `--dpr=2`。`--typography-native-smoothing` 仅供 screenshot feature 下隔离默认笔画增厚行为的 A/B 验证。

半透明侧栏必须另外验证：CDP 字样脚本增加 `--translucent`，GPUI 增加 `--typography-translucent`，比较脚本增加 `--translucent`，输出到独立目录以保留不透明对照。该模式使用 70% 背景与 85% 文字透明度，并将 Electron 的 straight-alpha PNG 和 GPUI 的 premultiplied-alpha 读回像素正确合成到相同底色；直接丢弃 alpha 会产生错误的字体粗细结论。`cargo test -p gpui_apple --lib compositing_tests` 在真实 Metal 管线上检查不同字形覆盖率、深浅底色及透明/半透明/不透明背景的 source-over 合成。

设置页的完整视觉矩阵包含 21 个页面、light/dark 两种主题，共 42 对 1440×900 截图：

```bash
npm ci
./node_modules/.bin/electron scripts/verify_chatgpt_settings.cjs
REFRESH_SETTINGS_REFERENCES=1 scripts/capture_settings_matrix.sh
python3 scripts/verify_settings_matrix.py
```

首页双主题截图和像素比较可一次完成：

```bash
REFRESH_REFERENCES=1 scripts/compare_all.sh
```

## 内置终端

打开右侧功能区后选择“终端”，或使用 `Ctrl+反引号` 显示/收起终端。终端在当前聊天的工作目录启动本机 `$SHELL` 登录会话，支持 ANSI 颜色、中文、shell 历史与 Tab 补全、Ctrl-C 中断、滚动回看、拖动选择及 `Cmd+C` / `Cmd+V` 复制粘贴。`Cmd+K` 清屏；顶部 `+` 新建独立终端，标签关闭按钮结束对应 shell。收起面板保留进程，切换聊天保留各自的终端；应用退出后不恢复 shell 进程。

终端通过本机 PTY 运行，界面由 GPUI 原生网格绘制，不经过 Codex app-server 的命令执行方法。参考样式来自独立 ChatGPT 调试实例的 CDP 实测。打开该实例的右侧终端后，可重新捕获样式及无副作用的输出样本：

```bash
CHATGPT_CDP_HTTP=http://127.0.0.1:9222 node scripts/cdp_capture_terminal.mjs artifacts/terminal
cargo test components::terminal::tests
```

## 文件查看与编辑

右侧功能区的“文件”、底部添加菜单中的“文件”或 `Cmd+P` 可打开当前聊天工作目录的文件树。支持展开目录、筛选路径、方向键导航、Enter 打开、多文件标签，以及点击聊天中的本地文件链接并定位到对应行。收起面板或切换聊天会保留各自的标签、选区和撤销记录。

文本直接在原生 GPUI 编辑器中修改，提供语法高亮、行号、自动换行、长文件滚动、跨行选择、中英文输入及复制粘贴。停止输入约 400 ms 后自动保存；`Cmd+S` 立即保存，`Cmd+Z` / `Cmd+Shift+Z` 或右下角按钮撤销／重做，并同步到磁盘。Markdown 默认显示预览，可切换源代码编辑；常见栅格图片支持面板内查看。

文件访问和保存在线程外执行，保持原有 UTF-8 BOM、CRLF 和权限。保存前检查磁盘内容，外部修改发生冲突时保留编辑并提示复制、重新加载或重试；没有本地编辑时自动刷新外部变更。文本上限为 2 MB、单行 64 KB，二进制和非 UTF-8 文件提供外部打开入口。当前仅访问本机文件系统。

参考样式与自动保存交互来自独立 ChatGPT 调试实例的实时 CDP。先打开该实例的文件面板，可再次采集包含 shadow DOM 的尺寸、字体、颜色及截图：

```bash
CHATGPT_CDP_HTTP=http://127.0.0.1:9222 \
  node scripts/cdp_capture_file_panel.mjs artifacts/file-panel
cargo test components::file_ -- --test-threads=1
```

独立原生验收沿用 `GPUI Capture.app`，截图构建可附加 `--file-panel-root=/absolute/test/workspace` 和 `--open-file=/absolute/test/workspace/example.rs`，以真实可编辑测试文件验证保存、撤销和冲突。请使用专用测试文件，因为编辑会自动写回磁盘。

## Codex app-server 协议

当前全部 JSON-RPC 方法及实际接入状态统一维护在 [`docs/APP_SERVER_INTEGRATION.md`](docs/APP_SERVER_INTEGRATION.md)。升级 Codex CLI 时直接核对并更新该总表。

## 项目文档

仓库只维护三份 Markdown：本 README 负责快速入口，[`AGENTS.md`](AGENTS.md) 记录项目背景与文档地图，[`docs/APP_SERVER_INTEGRATION.md`](docs/APP_SERVER_INTEGRATION.md) 以唯一总表记录 Codex app-server 当前全量方法及接入状态。
