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

## 验证

Rust 基础校验：

```bash
cargo fmt --check
cargo test
cargo check --all-targets
```

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
