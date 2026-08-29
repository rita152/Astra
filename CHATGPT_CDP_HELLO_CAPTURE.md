# ChatGPT 桌面 App `hello` 流程 CDP 采集记录

采集日期：2026-08-28；Composer 发送后生命周期复测：2026-08-29（Asia/Shanghai）
采集方式：直接请求 `http://127.0.0.1:9222/json/list`，再以原生 CDP WebSocket 连接页面 target；未使用 Codex Browser/Chrome 插件。

## 唯一基准 target

| 字段 | 采集值 |
|---|---|
| title | `ChatGPT` |
| URL | `app://-/index.html` |
| target id | `BC9C3CE5514A207255D9149CEB6C51B9` |
| viewport | `2560 × 1410` logical px |
| device pixel ratio | `1` |
| sidebar width | `260.359375px` |

## 关键几何与 computed styles

| 元素 | 位置与尺寸 | 背景 / 前景 | 字体 | 盒模型 |
|---|---|---|---|---|
| 主内容背景 | `x=260.359375, y=0`，填满余下窗口 | `#181818` | — | — |
| 会话内容轨道 | `x=1042.671875, width=736` | transparent | system UI | 外层最大宽 `768px`、左右 padding `16px` |
| Composer root | `x=1042.671875, y=1296, 736×98` | computed `oklab(0.297161 … / 0.864706)`；截图解析值 `#2a2a2a` | — | radius `25px`；四层 shadow：白色 0.5px、`0 3px 7.5px #0000000a`、`0 0 20px #0000000d`、`#181818` 0.5px |
| 输入框 | `x=1054.671875, y=1310, 712×44` | transparent / `#dfdfdf` | `-apple-system, system-ui, Segoe UI, sans-serif`, `14px`, 400, `20px` | min-height `44px`；无 padding、border；聚焦前后均无可见 outline |
| Footer controls | `y=1358, height=28` | transparent | `13px / 18px` | 左起 `+` 在 x=1050.671875；权限 x=1083.671875；模型 x=1591.046875；听写 x=1706.671875；主操作 x=1742.671875 |
| 用户气泡 | `x=1715.828125, y=78, 62.84375×42` | text 5% overlay；截图解析值约 `#222222` / `#dfdfdf` | 内层 `14px`, 400, `21–22px` | padding `10px 16px`；radius `22px`；右对齐 |
| 用户文字 | `x=1731.828125, y=88, 30.84375×22` | transparent / `#dfdfdf` | `14px`, 400, `21–22px` | — |
| 思考提示 | `x=1042.671875, y=166, 56×21` | 底字 `color(srgb 0.87451 0.87451 0.87451 / 0.385)`；扫光 `rgba(255,255,255,0.75)` | Chromium platform font：`PingFang SC` / `PingFangSC-Regular`，`14px`, 400, `21px` | 两层文字；宽度 `100%` 的线性 mask 从左至右扫过；`1s steps(48)` |
| 模型回复 | `x=1042.671875, y=166, 736×22` | transparent / `#dfdfdf` | `14px`, 400, `22px` | 无 padding、border、radius、shadow |
| 模型操作区 | 起点约 `x=1038.671875, y=191`，单项 `26×26` | transparent / tertiary text | SVG `14px` | gap `2px` |

## 状态与真实行为

| 顺序 | 状态 | 真实观察 |
|---:|---|---|
| 1 | 新对话 | Composer 自动获得焦点；空白首页主按钮是 `16×16` 语音波形图标，按钮 `28×28`。 |
| 2 | `Esc` | active element 从 textbox 变为 `BODY`；Composer 几何、背景、边框均不变，`1×17px` 插入光标消失。 |
| 3 | 再次点击输入框 | textbox 重新获得焦点，无可见 focus ring；仅在 `x=1055, y=1311–1327` 显示 `#dfdfdf` 闪烁插入光标。 |
| 4 | 输入 `hello` | 主按钮切换为 `20×20` 向上箭头；按钮仍为 `28×28`、`#dfdfdf` 圆形。 |
| 5 | hover / mouseDown | computed background、opacity、transform 和 shadow 不变；mouseDown 后焦点转到发送按钮。 |
| 6 | `Enter` 发送 | 输入内容清空；用户气泡立即出现在 `(1715.828125, 78)`；首页标题和建议项卸载。 |
| 7 | 启动 / 思考 | Enter 后 Composer 主体保持原位可见，仍为 `(1042.671875, 1296) 736×98`；仅项目/本地/分支上下文工具条移除，输入内容清空，主按钮切换为运行状态。 |
| 8 | 流式回复 | Composer 的几何、背景、圆角和阴影保持不变；上下文工具条不再出现，主按钮为 `20×20` 停止方块，DOM 回复按 token 追加。 |
| 9 | 完成 | 回复为 `Hello! What would you like to work on?`；停止按钮恢复为 `20×20` 发送箭头，空输入时最终 opacity `0.5`；模型回复下方出现复制、赞、踩、分支控件。 |
| 10 | 点击外部 | textbox 失焦，active element 变为 `BODY`；布局不移动。 |
| 11 | `Tab` | 焦点进入 Composer 上方第一个项目/上下文按钮；无菜单在本流程中打开。 |

首个可见回复 DOM 于 mutation 时间 `112881071.1ms` 出现，最终文本于 `112881175.7ms` 拼接完成，完成状态于 `112881403.0ms` 落地。首内容到完整文本约 `104.6ms`，首内容到最终状态约 `331.9ms`。

2026-08-29 复测的 `08/09/10` 快照在路由切换瞬间采到了 `textbox=null` 的中间 DOM；结合连续可见状态截图确认，这个采样空窗不应被实现为 Composer 的视觉卸载。正确视觉基准是 Composer 主体从发送到完成始终保留，只移除上方上下文工具条。完成状态仍为 `(1042.671875, 1296) 736×98`、radius `25px`，背景和四层阴影与发送前相同；当前占位文案为“随心输入”，空输入发送按钮 opacity 为 `0.5`。

## 图标

| 状态 / 控件 | SVG 尺寸 | 对齐与颜色 |
|---|---:|---|
| 首页语音主按钮 | `16×16` | 在 `28×28` 圆形按钮内水平、垂直居中；图标使用主操作实色 |
| 激活发送箭头 | `20×20` | `viewBox 0 0 20 20`；currentColor；居中 |
| 运行中停止方块 | `20×20` | 方块 path 从 `(4.5,4.5)` 到 `(15.5,15.5)`；currentColor；居中 |
| Footer 普通图标 | `16×16`（模型 fast 为 `14×14`） | 与 28px 控件中心对齐 |
| 回复操作图标 | 约 `14×14` | 置于 `26×26` hit target 中，tertiary text 色 |

每个状态的完整 DOM 片段、computed style、父级链、按钮 outerHTML/SVG、rect 和 mutation log均保存在 [`artifacts/chatgpt-hello-flow`](artifacts/chatgpt-hello-flow/)；2026-08-29 的发送后复测保存在 [`artifacts/chatgpt-composer-after-send-2026-08-29`](artifacts/chatgpt-composer-after-send-2026-08-29/)。对应 `.png` 是同一时刻的 CDP `Page.captureScreenshot` 结果。
