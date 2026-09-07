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

## Codex app-server 协议

当前全部 JSON-RPC 方法及实际接入状态统一维护在 [`docs/APP_SERVER_INTEGRATION.md`](docs/APP_SERVER_INTEGRATION.md)。升级 Codex CLI 时直接核对并更新该总表。

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

当前自动化结果为 409 项测试通过、0 项失败、1 项按原配置忽略。忽略项会调用已登录的本机 Codex CLI 发起真实模型请求；本轮未执行。Clippy 对第一方包的全部 target 与 feature 使用 `-D warnings`；依赖及 vendor 不在此次零告警结论范围内。

界面回归使用保留的旧版可执行文件和最新构建，均通过独立 bundle ID 的 `GPUI Capture.app` 验收。有效截图为 66 组：21 个设置页面 × 两种主题、10 种会话夹具 × 两种主题，以及 Markdown 两种宽度 × 两种主题。对应图片保持相同物理尺寸，未缩放或平移；47 组整图逐像素一致。其余 19 组差异全部落在窗口激活态影响的半透明侧栏、异步工作区列表或模型名称区域，排除这些明确标记的动态区域后，66 组的功能内容区域均逐像素一致。设置正文包含从源码迁出的 Chronicle 插画；三组资源的 3,846 条坐标和颜色记录与迁移前逐项相同。

Computer Use 另对旧版与新版复核了工具组／命令详情点击、工具区滚动与 Tab 焦点、右侧面板拖动、终端／文件快捷键、输入文字及全选、中文与 emoji 粘贴、模型菜单与 Escape、设置正文滚动及页面切换。会话流、中断、审批、线程切换和键盘折叠的状态逻辑由原有自动化测试覆盖；本轮未发送真实模型提示。专用验收实例已关闭。截图和逐项比较记录保存在本机 `artifacts/refactor-validation/{before-final,after-final}/` 与 `comparison.json`，不作为新的 Markdown 文档维护。

## 项目文档

仓库只维护三份 Markdown：本 README 负责快速入口与架构重构进度，[`AGENTS.md`](AGENTS.md) 记录项目背景与文档地图，[`docs/APP_SERVER_INTEGRATION.md`](docs/APP_SERVER_INTEGRATION.md) 以唯一总表记录 Codex app-server 当前全量方法及接入状态。
