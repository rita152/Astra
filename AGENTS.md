# 仓库约定

适用于整个仓库；不设置子目录级 `AGENTS.md` 或 `AGENTS.override.md`。

## 项目背景

基于 Rust 与 GPUI 的原生桌面 Agent 应用，目标是统一管理本机 coding agent 的发现、配置、启动、会话与运行状态。当前只接入 Codex app-server 的部分能力，其他 agent 尚未接入；扩展以产品需求和实际协议为依据。

## 文档职责

仓库只维护以下三份 Markdown。记录当前行为、边界和验证入口；完成过程、历史测试数量及逐次验收记录留在本机产物中。

| 文档 | 内容 | 更新时机 |
|---|---|---|
| [README.md](README.md) | 运行、功能入口、架构与验证命令 | 依赖、入口、行为、架构或常用命令变化 |
| [AGENTS.md](AGENTS.md) | 项目背景、工作约定与文档职责 | 项目定位或仓库约定变化 |
| [docs/APP_SERVER_INTEGRATION.md](docs/APP_SERVER_INTEGRATION.md) | Codex app-server 全量方法及接入状态的唯一总表 | CLI schema、运行时接入或兼容处理变化 |

## 代码边界

- `src/agent/` 领域模块不得依赖 GPUI、界面组件或具体适配器；`mod.rs` 只维护模块与导出。
- Codex 编解码留在 `src/agent/codex/`，与连接生命周期分开维护；通用媒体、文件工具不经适配器导出给 UI。
- 工作区状态、分页加载、偏好持久化分别由 `src/workspace.rs`、`src/workspace/loaders.rs`、`src/workspace/preferences.rs` 负责。
- 会话状态与事件归约在 `src/conversation/`；GPUI Entity、Context 和交互驱动留在视图层。
- 实现与测试按职责维护。删除代码前核对调用方和协议覆盖；不得扩大 `allow(dead_code)` 或 Clippy 抑制范围来消除告警。

## 界面验收

- 修改渲染或交互后，先构建最新可执行文件，再以独立 bundle ID 的 `GPUI Capture.app` 启动验收实例；构建命令见 README。
- Computer Use 先枚举应用并连接 `GPUI Capture`，再通过可访问性树和截图定位，复现修复前行为并验证修复结果。交互问题须检查完整命中区域及相关点击、滚动、拖动、键盘、悬停或文本选择路径。
- 前后使用相同主题、窗口尺寸、DPR、线程、内容及滚动位置；不得缩放或平移截图提高对比分数。
- ChatGPT 参考采集使用专用调试实例及新建端口；不占用其他任务的调试端口，不操作用户正在运行的 ChatGPT 或 GPUI 实例。
- 结束后只关闭本次专用实例；运行与改动相称的格式、测试和编译检查，交付时说明实际验证范围与结果。截图、日志和对比数据保存在 `artifacts/`。
