# 项目背景与文档地图

## 项目背景

本项目是一个基于 Rust 与 GPUI 构建的原生桌面 Agent 应用，长期目标是为用户本机安装的各类 coding agent 提供统一入口，集中管理其发现、配置、启动、会话与运行状态。

项目目前处于协议接入阶段：只完成了 Codex app-server 部分生命周期、方法和事件的适配，尚未覆盖其全部协议能力；其他 coding agent 也还没有正式接入。后续扩展应以真实产品需求和各 agent 的协议边界为依据。

## 文档地图

仓库只维护以下 3 份 Markdown。

| 路径 | 类型 | 功能 | 何时更新 |
|---|---|---|---|
| `README.md` | 人工维护 | 项目定位、启动方式、核心验证命令和文档入口 | 启动方式、依赖或常用命令变化时 |
| `AGENTS.md` | 人工维护 | 项目背景与仓库 Markdown 文件地图 | 项目定位或文档职责变化时 |
| `docs/APP_SERVER_INTEGRATION.md` | 人工维护 | 当前 Codex app-server 全量 JSON-RPC 方法及接入状态的唯一总表 | Codex CLI 协议、运行时接入、passive 集合或兼容行为变化时 |
