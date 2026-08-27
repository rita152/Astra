# GPUI Codex chat clone

这是一个纯 GPUI、组件化的 Codex 首页复刻。界面没有嵌入 HTML/WebView，也不使用任何 PNG 作为界面资源。除 SVG 图标外，侧栏、列表、滚动条、背景、文字、Composer 与主题控件都由 GPUI 原生组件绘制。

```bash
cargo run --release -- --theme=dark
cargo run --release -- --theme=light
```

普通 `cargo run` 也会保留调试符号和断言，但会以适合 GPUI 实时渲染的优化等级编译应用与依赖，避免全屏窗口在 Cargo 默认 `opt-level=0` 下出现滚动掉帧。首次构建会比完全未优化的 dev build 稍慢。

应用也能直接从 GPUI 的 Metal 场景导出 PNG，不依赖系统录屏权限：

```bash
cargo build --release --features screenshot
target/release/gpui-chat-clone --theme=dark --screenshot=artifacts/actual-dark.png
```

截图能力被单独放在 `screenshot` feature 中；常规交互构建不会开启 GPU drawable 读回路径。

设置页使用 Electron 参考截图进行 1440×900 的 light/dark 像素回归。完整验收固定覆盖 manifest 中的 21 个页面和 light/dark 两套主题，共 42 对截图：

```bash
npm ci
./node_modules/.bin/electron scripts/verify_chatgpt_settings.cjs
REFRESH_SETTINGS_REFERENCES=1 scripts/capture_settings_matrix.sh
python3 scripts/verify_settings_matrix.py
```

`REFRESH_SETTINGS_REFERENCES=1` 会强制让 Electron 逐页重新渲染 `chat-reference/settings/{theme}/{slug}.html`，避免沿用陈旧基准。捕获脚本在 `target/` 中生成仅用于测试的 1× app bundle，让 Electron 与 GPUI 在 Retina Mac 上也稳定输出逻辑像素，不对 2× 图片做缩放。每次截图先写入独立临时文件，通过尺寸检查后才替换矩阵文件，因此旧截图不能掩盖捕获失败。只有 reference 和 actual 各自都恰好包含固定白名单中的 42 张非空、1440×900 PNG 时捕获才成功；manifest、light/dark HTML 与四个截图目录都必须精确匹配这 21 个页面。

默认的 99.5% 像素门槛定义为：

```text
normalized_similarity = 100 × (1 - Σ|reference RGB - actual RGB| / (宽 × 高 × 3 × 255))
```

该指标直接使用未模糊的 Electron reference 与 GPUI actual 像素，每一个页面/主题组合都必须达到 `99.5%`，不能用 42 页平均值抵消单页失败。布局亮度在 2px Gaussian blur 后计算的 `layout_similarity` 也必须逐页达到 `99.0%`。此外，报告继续记录并校验 0.65px 模糊、12/255 逐通道容差下的 `soft_pixel_consistency`，以及允许双向 1px 偏差的 `layout_edge_f1`。

Hooks 页面还会对页头与空状态卡片所在的内容裁剪区单独计算同一套未模糊 RGB 指标，并要求达到 `99.0%`。这条局部门禁防止全屏大面积空白掩盖卡片塌缩、链接或刷新按钮缺失等结构错误。

完整结果写入 `artifacts/settings-matrix/report.json`；每页的增强差异图与 50% 叠图分别位于 `artifacts/settings-matrix/diff/{theme}/{slug}/diff.png` 和 `overlay.png`。矩阵验证器不允许把 normalized 正式硬门槛调低到 99.5%，也不允许把 layout 正式硬门槛调低到 99%；可以显式传入相同或更高门槛：

```bash
python3 scripts/verify_settings_matrix.py \
  --min-normalized-similarity 99.5 \
  --min-layout-similarity 99
```

单页 `scripts/settings_layout_compare.py` 使用相同的 `normalized_similarity >= 99.5%` 和
`layout_similarity >= 99.0%` 默认硬门槛，并把门槛、失败项与最终 `passing` 状态写入
`report.json`。仅做无门槛诊断时必须显式传入：

```bash
python3 scripts/settings_layout_compare.py reference.png actual.png --output artifacts/diagnostic \
  --min-soft-consistency 0 \
  --min-normalized-similarity 0 \
  --min-layout-similarity 0 \
  --min-layout-edge-f1 0
```

像素对比固定使用 1440×900 内容窗口。参考快照必须在 Electron 中渲染，不能使用 Chrome 截图：

```bash
npm install
scripts/capture_references.sh
```

Electron 壳固定使用 `hiddenInset` 原生标题栏、1440×900 内容区和 1x device scale factor，并等待网页字体及两个 animation frame 后通过 `webContents.capturePage()` 输出基准图。捕获时会硬校验 `innerWidth=1440`、`innerHeight=900`、`devicePixelRatio=1`、活动 slug、页面标题和输出 PNG 尺寸。基准 PNG 仅用于测试对比，不会被 GPUI 应用加载或显示。

双击顶部 46px 标题栏会调用 GPUI `zoom_window()`，在普通窗口最大化与还原之间切换；不会进入 macOS fullscreen Space。

然后运行：

```bash
scripts/capture_window.sh dark
python3 scripts/pixel_compare.py artifacts/reference-dark.png artifacts/actual-dark.png \
  --output artifacts/dark --tolerance 0 --min-consistency 100 --min-edge-consistency 100
```

输出包含 `report.json`、`diff.png` 和 50% 透明叠图 `overlay.png`。`pixel_consistency` 是逐像素、逐通道容差内的匹配率；`exact_pixel_consistency` 同时保留严格零差异指标，避免用均值掩盖局部偏差。

准备好两张基准后，可一次构建、截图并校验双主题：

```bash
scripts/compare_all.sh
```

需要强制刷新 Electron 基准时：

```bash
REFRESH_REFERENCES=1 scripts/compare_all.sh
```

门槛与逐通道容差可显式覆盖，例如：

```bash
MIN_CONSISTENCY=100 MIN_EDGE_CONSISTENCY=100 PIXEL_TOLERANCE=0 scripts/compare_all.sh
```

正式实现始终由 GPUI primitives、系统字体和 SVG 图标构成，不存在像素皮肤或图片回退路径。
