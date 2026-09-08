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

macOS 字体渲染对照使用本机 ChatGPT 的真实 CDP 样式。应用按 CSS 的灰度抗锯齿、430 默认字重、中文回退和分数行高绘制；显式 400/500/600 字重仍分别保留。`vendor/gpui`、`vendor/gpui_macos` 与 `vendor/gpui_apple` 固定于 Cargo 中同一 Zed revision，在文本选择、字体缓存、栅格化、行高/基线、虚拟列表高度估算及 Metal 透明度合成处保留兼容修正，升级 GPUI 时须同时复核这些改动。品牌标题从本机 `/Applications/ChatGPT.app` 或 `~/Applications/ChatGPT.app` 读取原始 OpenAI Sans 字体到内存；仓库不分发该字体，未安装时使用系统字体回退。

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

Markdown 文件按内容版本在线程外解析，预览只布局可见块，大表格进一步按行渲染并复用列宽。每个文件保留独立的预览位置，表格各行共享横向滚动；方向键、Page Up／Down、`Cmd+↑`／`Cmd+↓` 可导航预览。长行内代码和文件链接在窄栏中换行，内容裁剪在文件区域内。

文件访问和保存在线程外执行，保持原有 UTF-8 BOM、CRLF 和权限。保存前检查磁盘内容，外部修改发生冲突时保留编辑并提示复制、重新加载或重试；没有本地编辑时自动刷新外部变更。文本上限为 2 MB、单行 64 KB，二进制和非 UTF-8 文件提供外部打开入口。当前仅访问本机文件系统。

参考样式与自动保存交互来自独立 ChatGPT 调试实例的实时 CDP。先打开该实例的文件面板，可再次采集包含 shadow DOM 的尺寸、字体、颜色及截图：

```bash
CHATGPT_CDP_HTTP=http://127.0.0.1:9222 \
  node scripts/cdp_capture_file_panel.mjs artifacts/file-panel
cargo test components::file_ -- --test-threads=1
```

滚动性能可用固定文件重复测量（GPUI 测试窗口中的滚轮事件与布局绘制耗时，不等同于屏幕帧率）：

```bash
GPUI_MARKDOWN_BENCH_FILE=docs/APP_SERVER_INTEGRATION.md \
  cargo test markdown_preview_scroll_timings -- --ignored --nocapture
```

独立原生验收沿用 `GPUI Capture.app`，截图构建可附加 `--file-panel-root=/absolute/test/workspace` 和 `--open-file=/absolute/test/workspace/example.rs`，以真实可编辑测试文件验证保存、撤销和冲突。请使用专用测试文件，因为编辑会自动写回磁盘。

## 审查

右侧功能区的“审查”或 `Ctrl+Shift+G` 打开当前会话工作目录的原生 Git 审查面板；会话文件卡的“审核”和底部菜单的“审查”也进入同一个组件。每个会话保留自己的范围、折叠、滚动和评论，从差异打开文件后可通过“审查”标签返回；收起面板后再次打开会保留状态。

- 范围包括上一轮、未提交、未暂存、已暂存、已提交及分支；分支比较使用 merge-base。“上一轮”使用 app-server 的补丁及历史文件变更，保留原始路径、行号和 patch，不用当前工作区内容替代历史。从历史文件卡打开的差异保持固定，重新选择“上一轮”才跟随最新更改。
- 支持文件树、筛选和跳转、单文件/全部折叠、统一/拆分差异、上下文展开、换行、文字差异、忽略空白、Markdown 预览、复制路径/patch、字符和跨行选择，以及分支范围的“已查看”状态。
- 点击或拖动行号创建本地评论。原生多行编辑器支持中文 IME、选择、复制和撤销；保存后在主输入框显示可点击的评论汇总，可返回对应评论、修改或删除，也可只发送评论。发送经已有的 `turn/start` 传递文件及左右侧行范围；模型不可用时保留待发送评论，成功开始请求后清除一次。提交信息与评论草稿使用独立编辑器。
- 提供文件/差异块/全部暂存与取消暂存、确认还原、提交、新分支、推送和 PR 创建。提交信息留空时从暂存文件生成。PR 支持标题、说明、目标分支、是否提交本地更改、草稿/正式创建及打开已有 PR；PR 操作依赖已登录的 `gh`，普通 Git 审查不需要 GitHub 登录。

Git 查询在后台执行，Git/gh 命令有超时、输出上限及进程组回收，面板隐藏后停止轮询。长差异按行虚拟化，切换布局和刷新按文件与源码行恢复位置，Shift+滚轮仅横向滚动；显示选项由现有工作区偏好存储原子保存。改变 Git 状态前检查工作区和 index 的版本，外部修改会要求刷新；操作失败保留错误和输入。还原新增文件时，内容备份到该 worktree Git 目录的 `gpui-discarded/`。

样式和交互来自独立 CDP 端口的 ChatGPT 实例，覆盖 shadow DOM、深浅主题、范围/显示菜单、评论、提交和 PR 对话框。已有 CDP 端口视为正在使用，不能复用；采集脚本要求显式指定新实例的端口：

```bash
CHATGPT_CDP_HTTP="http://127.0.0.1:$REVIEW_CDP_PORT" node scripts/cdp_capture_review.mjs artifacts/review-reference
cargo test git_review::tests
cargo test components::review_panel
```

screenshot 构建支持 `--review-root=/absolute/test/repository` 和可选的 `--review-filter=src/example.rs`，通过生产数据层打开真实 Git 仓库；与 `--screenshot=...` 组合时先等待 Git 加载完成。使用独立 `GPUI Capture.app` 和临时 Git 仓库验收暂存、还原、提交与推送。PR 回归使用本地 bare remote 和 `gh` 测试替身验证命令链，不发布远程 PR。

两份审查实现已整合：保留完整 Git/hunk 操作、原生提交与 PR 对话框、显示偏好和文件标签往返；接入结构化评论与 Composer 联动、仅评论发送、固定历史差异、源码行滚动定位、命令超时与进程组回收、窄栏自适应。面板宽度小于 560 px 时自动收起文件树，使用“跳转到文件”或 `Cmd+F` 查找；恢复宽度后沿用文件树显示偏好。

整合验收（2026-09-08）：441 项测试通过（426 项单元测试、15 项集成测试），2 项原有忽略项未运行；格式、第一方严格 Clippy、普通目标检查及 screenshot 构建通过。Computer Use 使用独立 bundle ID 的 `GPUI Capture.app`，复核同尺寸深浅主题、菜单键盘、统一/拆分布局、评论的多行中文及 emoji 编辑、评论汇总往返、快捷键、筛选、文件打开、全屏、宽度拖动和窄栏跳转。已在临时仓库实际完成暂存、取消暂存、仅提交已暂存文件并推送至本机 bare remote；本地与远端提交一致，未暂存内容保留。PR 成功命令链使用隔离的 `gh` 替身测试，评论发送使用记录请求的测试后端，不发布真实远程 PR 或发送真实模型提示。双主题截图及结构化记录位于 `artifacts/review-integration/`；本次专用验收实例已关闭。


长 diff 性能修复（2026-09-08）：可见行语法高亮使用最多 1,024 行、约 4 MiB 载荷的有界缓存；快照或主题改变时失效。文字差异范围、最大行宽和行号栏宽度按快照建立索引，滚动时直接查找；五位及六位数行号不换行，避免虚拟列表行高意外翻倍。4 万行差异的 100 次滚轮＋布局绘制采样中，统一视图中位耗时从 9.08 ms 降为 1.88 ms，深处拆分＋文字差异从 12.98 ms 降为 3.55 ms。这是 GPUI 测试窗口 CPU 处理耗时，不是屏幕 FPS。独立 Capture 已按相同主题、1470×923 窗口、相同测试仓库和源码位置验证滚动、行号、文字差异、跨行选择复制、评论及水平滚动条。445 项常规测试通过，专项基准已单独运行，两个原有忽略项未运行；格式、严格 Clippy、普通目标检查和 screenshot 构建通过。采样与对比记录位于 `artifacts/long-diff-performance/`。

可用保存的 unified patch 重复测量；该基准默认忽略，需要显式调用：

```bash
GPUI_DIFF_BENCH_PATCH=/absolute/path/to/long.diff \
  GPUI_DIFF_BENCH_OUTPUT=/tmp/gpui-diff-timings.json \
  cargo test long_diff_scroll_timings -- --ignored --nocapture
```

## Codex app-server 协议

当前全部 JSON-RPC 方法及实际接入状态统一维护在 [`docs/APP_SERVER_INTEGRATION.md`](docs/APP_SERVER_INTEGRATION.md)。升级 Codex CLI 时直接核对并更新该总表。

## 侧边聊天

右侧功能区的“侧边聊天”、底部添加菜单或 `⌥⌘S` 在当前主聊天的上下文上新建临时侧边对话。主聊天需要先有真实线程。侧边聊天支持多个标签、拖动排序、`Ctrl+Tab`／`Ctrl+Shift+Tab` 切换、`Cmd+W` 关闭当前标签、全屏和面板缩放；标题跟随首条问题，后台回复显示未读状态。收起面板和切换主聊天会保留对应的消息、草稿与滚动位置，文件、审查和终端提供返回侧边聊天的标签。

正文与主对话共用两侧 `24px` 留白，输入框在此基础上再向内 `6px`。长气泡按当前会话列的 70% 限宽，改变面板宽度时重新测量列表项并保留滚动锚点，避免窄栏中文字贴边、越界或覆盖后续回复。原生输入框支持多行、中文 IME、选择、复制粘贴、撤销／重做，`Enter` 发送，`Shift+Enter` 换行；窄栏仍保留模型、权限及发送入口。

侧边对话复用现有流式消息、工具、审批、Markdown、图片与回复复制／评价组件。添加菜单可通过系统选择器附加文件和文件夹，也支持拖入文件；图片作为 `localImage` 输入，其他文件作为路径上下文。计划模式使用真实 `turn/start.collaborationMode`。所有轮次都有独立的中断对象，停止或关闭侧边聊天不会中断主聊天。无消息的标签直接关闭，有消息时显示带“不再询问”的确认；关闭应用后临时聊天消失，连接失效时当前消息仍可查看和复制。

协议优先使用 `thread/fork(ephemeral=true, excludeTurns=true)`、`thread/inject_items` 和 `thread/unsubscribe`，不建立本地聊天数据库。参考样式通过独立 ChatGPT 调试实例的 CDP 采集；必须新建未被其他任务使用的端口，再将端点设置为 `CHATGPT_CDP_HTTP`：

```bash
node scripts/cdp_capture_side_chat.mjs artifacts/side-chat reference
cargo test side_ -- --test-threads=1
```

## 架构与渐进重构

应用以 `AgentBackend` 作为 coding agent 的应用边界，按领域契约、协议适配、工作区数据、会话状态和 GPUI 展示分层。`ChatApp` 在入口装配共享服务并注入各视图；会话事件归约与历史恢复集中在 `conversation`，输入框和活动视图按功能组织。各批迁移同步移动原有测试，保留既有协议与界面行为。

当前模块职责：

| 位置 | 职责与依赖边界 |
|---|---|
| `src/agent/mod.rs` | 只维护模块声明与领域、后端入口的导出 |
| `src/agent/{backend,catalog,thread,activity,status,events,requests,message}.rs` | 后端契约、模型配置、历史、活动、状态、事件、交互请求与消息规范化；不依赖 GPUI、组件或具体适配器 |
| `src/agent/codex.rs` | Codex 适配器模块入口 |
| `src/agent/codex/{catalog,items,methods,notifications,permissions,requests,workspace_protocol}.rs` | Codex wire 类型、方法覆盖与校验、请求编码、通知及历史解码 |
| `src/agent/codex/{transport,session,registry,dispatch}.rs` | JSONL 进程通信、轮次会话、服务端请求注册与响应校验、轮次事件派发 |
| `src/agent/codex/manager.rs`、`manager/` | 应用级连接 generation、启动与退出回收；子模块分别管理共享连接、传输、事件订阅、turn 路由、目录及工作区请求 |
| `src/workspace.rs` | 工作区状态、通知合并、异步操作及订阅；通过 `AgentBackend` 访问后端 |
| `src/workspace/{loaders,preferences}.rs` | 分页读取与历史补全、UI 偏好版本处理与原子写入；统一处理游标循环和加载错误 |
| `src/conversation/` | 会话状态、活动模型、事件归约、流式批处理、历史恢复、模型选择和轮次生命周期；不持有 GPUI Entity 或 Context |
| `src/components/composer.rs`、`composer/` | 输入框入口；运行时 UI 驱动、选项菜单、权限与审批交互、听写、布局、渲染及截图夹具分别维护 |
| `src/components/home.rs`、`home/` | 会话视图协调；时间线、消息、工具活动、推理、协作、媒体与请求按功能绘制；具名上下文承载共享渲染数据 |
| `src/git_review.rs`、`git_review/` | 本机 Git 状态、diff/hunk、版本校验、Git/gh 操作、命令生命周期及结构化评论；不依赖 GPUI 或具体 agent 适配器 |
| `src/components/review_panel.rs`、`review_panel/` | 审查状态、异步加载、虚拟列表、菜单、差异文本、评论及 Git 对话框；显示选项通过现有工作区偏好保存 |
| `src/app.rs`、`app/` | 服务装配与应用壳；会话 host、侧栏、底部／右侧面板、图片预览、项目创建、文件和终端面板分别维护；面板状态有独立类型 |
| `src/settings/view.rs`、`view/` | 设置导航与路由、共享控件及各功能页面；Chronicle 插画坐标作为嵌入资源位于 `assets/illustrations/` |
| `src/components/callback.rs`、`src/media.rs` | 通用 UI 回调封装和共享图片尺寸读取；通用媒体工具不经 Codex 适配器导出 |

执行顺序及验收结果（2026-09-07）：

| 阶段 | 完成范围 | 验收条件 | 状态 |
|---|---|---|---|
| 1. 建立领域边界 | 按职责拆分领域模块；通用图片工具移出 Codex；清理无调用方的旧进程入口 | 领域模块无 GPUI 或具体适配器依赖；现有调用方与测试通过 | 已完成 |
| 2. 分离工作区数据访问 | 提取偏好持久化、分页加载和测试；历史使用 `Page<ThreadTurn>`，合并重复分页循环 | 覆盖空页、重复及循环游标、失败传播、通道关闭、历史按需补全与偏好原子写入 | 已完成 |
| 3. 拆分 Codex 适配器 | 分离协议编解码、transport、turn session、请求注册表、共享连接生命周期及测试驱动 | 握手、乱序响应、generation 隔离、中断、请求清理和退出回收测试通过；协议接入范围保持一致 | 已完成 |
| 4. 提取会话状态 | 从 Composer 提取 transcript、活动更新、事件批处理、历史恢复和轮次状态；视图保留输入及 UI 驱动 | 流式输出、审批、断连、恢复、线程切换与历史折叠测试通过 | 已完成 |
| 5. 按功能拆分界面 | 拆分 Home 活动展示、Composer 交互、App 面板及设置页；使用渲染上下文、面板状态及控件参数结构体 | 同主题、尺寸、内容和滚动位置核对视觉；独立 Capture 实例验证点击、键盘、滚动、拖动和文本选择 | 已完成 |
| 6. 清理代码风格与冗余 | 统一相关导入与命名；合并重复回调；精简分支和重复克隆；为大型 MCP 枚举载荷使用 Box；UI 线程内共享列表使用 Rc | 格式、测试、普通与 screenshot 编译通过；第一方代码严格 Clippy 零告警；未扩大 lint 抑制，未改动 vendor 或依赖 | 已完成 |

六个阶段均已完成。后续新 agent 通过 `AgentBackend` 接入，新增协议编解码留在对应适配器；共享抽象和 Cargo workspace 拆分以第二个实际适配器的需求为依据。

重构验证命令：

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features --no-deps -p gpui-chat-clone -- -D warnings
cargo check --all-targets
cargo check --all-targets --features screenshot
cargo build --features screenshot
git diff --check
```

重构收尾时的自动化结果为 409 项测试通过、0 项失败、1 项按原配置忽略。忽略项会调用已登录的本机 Codex CLI 发起真实模型请求；本轮未执行。Clippy 对第一方包的全部 target 与 feature 使用 `-D warnings`；依赖及 vendor 不在此次零告警结论范围内。

界面回归使用保留的旧版可执行文件和最新构建，均通过独立 bundle ID 的 `GPUI Capture.app` 验收。有效截图为 66 组：21 个设置页面 × 两种主题、10 种会话夹具 × 两种主题，以及 Markdown 两种宽度 × 两种主题。对应图片保持相同物理尺寸，未缩放或平移；47 组整图逐像素一致。其余 19 组差异全部落在窗口激活态影响的半透明侧栏、异步工作区列表或模型名称区域，排除这些明确标记的动态区域后，66 组的功能内容区域均逐像素一致。设置正文包含从源码迁出的 Chronicle 插画；三组资源的 3,846 条坐标和颜色记录与迁移前逐项相同。

Computer Use 另对旧版与新版复核了工具组／命令详情点击、工具区滚动与 Tab 焦点、右侧面板拖动、终端／文件快捷键、输入文字及全选、中文与 emoji 粘贴、模型菜单与 Escape、设置正文滚动及页面切换。会话流、中断、审批、线程切换和键盘折叠的状态逻辑由原有自动化测试覆盖；本轮未发送真实模型提示。专用验收实例已关闭。截图和逐项比较记录保存在本机 `artifacts/refactor-validation/{before-final,after-final}/` 与 `comparison.json`，不作为新的 Markdown 文档维护。

## 项目文档

仓库只维护三份 Markdown：本 README 负责快速入口与架构重构进度，[`AGENTS.md`](AGENTS.md) 记录项目背景与文档地图，[`docs/APP_SERVER_INTEGRATION.md`](docs/APP_SERVER_INTEGRATION.md) 以唯一总表记录 Codex app-server 当前全量方法及接入状态。
