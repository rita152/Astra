# GPUI Codex chat clone

这是一个纯 GPUI、组件化的 Codex 桌面界面。应用不嵌入 HTML/WebView，也不把截图或 PNG/JPEG 用作产品 UI；侧栏、工作区、Composer、设置、审批和 Diff Review 均由 GPUI 原生组件、系统字体与 SVG 绘制。

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

该路径会等待线程历史、侧栏和模型目录完成 hydration，并额外等待三个实际绘制的稳定帧；历史加载失败或超时会以非零状态退出，截图输出目录会自动创建。
`<thread-id>` 可直接使用 ChatGPT App 侧栏 DOM 中的 `local:<uuid>`，也可使用 Codex app-server 的原始 UUID。
窗口宽高使用逻辑像素；PNG 的物理像素尺寸会跟随当前显示器缩放倍率。
`--resume-scroll-from-bottom` 为可选的逻辑像素距离，用于让 light/dark 捕获稳定落在同一段历史内容；省略时截图停在会话底部。

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

## Codex app-server 协议

当前全部 JSON-RPC 方法及实际接入状态统一维护在 [`docs/APP_SERVER_INTEGRATION.md`](docs/APP_SERVER_INTEGRATION.md)。升级 Codex CLI 时直接核对并更新该总表。

## 项目文档

仓库只维护三份 Markdown：本 README 负责快速入口，[`AGENTS.md`](AGENTS.md) 记录项目背景与文档地图，[`docs/APP_SERVER_INTEGRATION.md`](docs/APP_SERVER_INTEGRATION.md) 以唯一总表记录 Codex app-server 当前全量方法及接入状态。
