# Codex CLI App Server JSON-RPC 协议清单

> 基准版本：`codex-cli 0.150.1`
> 生成日期：2026-08-30
> 接入状态更新：2026-08-30
> 范围：`codex app-server generate-json-schema --experimental` 输出的全部方法，并与默认 schema 对比标注能力门槛。

## 结论

Codex App Server 使用双向 JSON-RPC 2.0 语义，但线上消息省略标准的 `"jsonrpc": "2.0"` 字段。当前版本完整 schema 共包含 **244** 个方法：

| 消息族 | 数量 | 含义 |
|---|---:|---|
| ClientRequest | 153 | 客户端调用 App Server，服务端按同一 `id` 返回 `result` 或 `error` |
| ServerRequest | 11 | App Server 反向调用客户端，客户端必须按同一 `id` 响应 |
| ClientNotification | 1 | 客户端发送的无响应通知 |
| ServerNotification | 79 | App Server 推送的无响应事件 |
| **合计** | **244** | 默认 schema 185 个；仅实验 schema 额外 59 个 |

“默认”表示该方法存在于不带 `--experimental` 的生成结果；“实验性”表示只存在于带 `--experimental` 的结果，连接初始化时通常还需声明 `capabilities.experimentalApi: true`。这不是产品成熟度评级；部分默认方法仍可能处于开发中或已经弃用。

## 传输与消息格式

| 项目 | 协议约定 |
|---|---|
| stdio | 默认；每行一个 JSON 消息（JSONL） |
| WebSocket | 每个文本帧一个 JSON-RPC 消息；实验性 |
| Unix socket | 在 Unix socket 上使用 WebSocket HTTP Upgrade |
| 请求 | `{"method":"…","id":1,"params":{…}}` |
| 成功响应 | `{"id":1,"result":{…}}` |
| 错误响应 | `{"id":1,"error":{"code":-32600,"message":"…"}}` |
| 通知 | `{"method":"…","params":{…}}`，没有 `id` |
| 握手 | 每条连接先发 `initialize` 请求，再发 `initialized` 通知 |
| 过载错误 | WebSocket 入站队列满时返回 `-32001`，客户端应采用带抖动的指数退避重试 |

## 接入所需的最小生命周期

| 顺序 | 方法 | 说明 |
|---:|---|---|
| 1 | `initialize` | 发送客户端名称、标题、版本及能力；每条连接只能调用一次 |
| 2 | `initialized` | 确认初始化完成 |
| 3 | `thread/start` 或 `thread/resume` | 创建或恢复会话线程 |
| 4 | `turn/start` | 向指定线程提交用户输入并开始一次执行 |
| 5 | 监听 `turn/*`、`item/*` 等通知 | 接收增量文本、工具进度、文件修改和状态变化 |
| 6（可选） | `turn/interrupt` | 在同一连接上携带当前 `threadId`、`turnId` 请求取消活动 turn |
| 7 | `turn/completed` | 一次 turn 的最终状态通知；取消成功最终为 `status: "interrupted"` |

## 已接入范围

当前实现通过统一的 `AgentBackend` 接口隔离具体 coding agent：`load_model_catalog()` 返回 agent-neutral 的 `AgentModelCatalog`，`run_prompt(AgentRequest)` 返回包含 `AgentEvent` 流和 `AgentInterruptHandle` 的 `AgentRun`。Codex 适配器位于 `src/agent/codex.rs`；`model/list` 的分页、camelCase 字段、请求 id、通知 payload 和 JSON-RPC 错误都封装在该模块。UI 只依赖 agent-neutral 类型，不直接依赖 Codex JSON-RPC。流式事件仍由 `ComposerView::apply_agent_event_batch` 批量消费。

| 已接入 JSON-RPC 方法 | 方向 | 内部协议 | 接入职责 |
|---|---|---|---|
| `initialize`、`initialized` | 客户端 → 服务端 | `initialize_connection` 连接生命周期（无 `AgentEvent`） | 为模型目录连接和 prompt 连接建立初始化握手 |
| `model/list` | 客户端 → 服务端 | `CodexAppServerBackend::load_model_catalog` → `AgentModelCatalog` | 使用 `cursor`/`nextCursor` 拉取全部可见页；映射 model、`displayName`、默认模型、effort 与 service tier |
| `thread/start` | 客户端 → 服务端 | `AgentRequest` → `drive_session`（无 `AgentEvent`） | 为本次 prompt 创建临时、只读线程，并传入所选 `model`、`serviceTier` |
| `turn/start` | 客户端 → 服务端 | `AgentRequest` → `drive_session`（无 `AgentEvent`） | 提交文本 prompt，并传入所选 `model`、`effort`、`serviceTier` |
| `turn/interrupt` | 客户端 → 服务端 | `AgentInterruptHandle::interrupt` → `CodexTurnSession` | 在原 stdio 连接上使用已保存的 `threadId`、`turnId` 发送一次中断；开始阶段的停止请求会排队，重复请求及已结束 turn 不会重复写入 |
| `item/started` | 服务端 → 客户端 | `AgentEvent::AssistantMessageStarted { item_id }` / `AgentEvent::CommandStarted(CommandExecution)` | 建立 assistant message 或 command activity |
| `item/agentMessage/delta` | 服务端 → 客户端 | `AgentEvent::TextDelta(String)` | 追加流式 assistant 文本 |
| `item/commandExecution/outputDelta` | 服务端 → 客户端 | `AgentEvent::CommandOutputDelta { item_id, delta }` | 按 `item_id` 将流式输出追加到对应 command activity |
| `item/completed` | 服务端 → 客户端 | `AgentEvent::CommandCompleted(CommandExecution)` / `AgentEvent::TextDelta(String)` | 完成 command activity；未收到文本 delta 时用完整 agent message 兜底 |
| `model/rerouted` | 服务端 → 客户端 | `AgentEvent::ModelRerouted` | 更新本轮实际模型，并在选择器触发器中显示 reroute 状态 |
| `model/verification` | 服务端 → 客户端 | `AgentEvent::ModelVerificationRequired` | 将额外账户验证要求转换为可见的失败状态 |
| `model/safetyBuffering/updated` | 服务端 → 客户端 | `AgentEvent::ModelSafetyBufferingUpdated` | 更新实际模型和暂态安全检查提示，结束 buffering 时清除提示 |
| `turn/completed` | 服务端 → 客户端 | `AgentEvent::Completed` / `AgentEvent::Interrupted` / `AgentEvent::Failed(String)` | 按 `completed`、`interrupted`、`failed` 终态结束本轮，并在终态后回收 stdin 与 app-server 子进程 |

Prompt 会话的 stdin 由可并发写入的 `CodexTurnSession` 持续持有，当前 `threadId` 与 `turnId` 会保留到终态。点击停止后 Composer 只进入 `Stopping`，不会截断事件流或伪造本地完成；只有收到匹配 turn 的 `turn/completed` 且状态为 `interrupted` 后才转为 `Stopped`。若任务先自然完成，则保留 `Complete`；若中断写入或连接失败，则进入 `Failed`。控制句柄丢弃、协议异常和正常终态都走幂等的关闭、kill、wait 路径，避免重复停止与退出竞态留下子进程。

选择器不再维护模型硬编码目录。目录加载完成后优先选择 `isDefault: true` 的模型（缺失时退回首项），使用该模型的 `defaultReasoningEffort` 和 `defaultServiceTier`；切换模型时重新应用目标模型的默认项。高级菜单、键盘导航及简化 effort 滑杆都按当前目录长度动态生成。`ThreadStartParams` 的本机 schema 没有 `effort` 字段，因此 effort 按 schema 仅发送给 `turn/start`，没有通过未定义字段塞入 `thread/start`。

Composer 的视觉层以本机 ChatGPT App（CDP `127.0.0.1:9222`）的实际计算样式为基准：展开触发器和主菜单宽 224 px，模型子菜单宽 280 px，推理强度子菜单宽 180 px，速度子菜单宽 233 px；行高、内边距、圆角、悬停/选中态、勾选图标、子菜单底部对齐和“重置为默认设置”行为均按实测值实现。模型子菜单只显示 `model/list` 返回的 `displayName`；推理强度和速度选项仍由目录动态决定，其中 ChatGPT UI 专属的 Ultra 副文案固定本地化为“更快消耗使用额度”，不使用协议中的英文 effort 描述替代该界面文案。

未接入的服务端反向请求会收到 `-32601`，因此当前实现固定使用 `approvalPolicy: "never"`、`sandbox: "read-only"`。该策略下 app-server 自行执行的 `commandExecution` 可展示开始状态、增量输出和完成状态；需要用户审批或交互的操作仍未接入。

下方总表中，“是”表示消息已转换为内部协议并由应用消费；“已知（no-op）”表示适配器会显式接受该通知，但不生成 `AgentEvent`；“否”表示尚未定义或接入，实际收到时会进入未定义方法错误处理。

## 全部方法

### 客户端 → 服务端请求（ClientRequest，153 个）

| # | Method | 消息形式 | Params schema | 能力门槛 / 状态 | 内部方法 | 是否接入 |
|---:|---|---|---|---|---|:---:|
| 1 | `initialize` | 请求（有 `id`） | `InitializeParams` | 默认 | `initialize_connection`（`load_model_catalog` 与 `run_prompt` 共用） | 是 |
| 2 | `server/diagnostics` | 请求（有 `id`） | `ServerDiagnosticsParams` | 实验性 | — | 否 |
| 3 | `thread/start` | 请求（有 `id`） | `ThreadStartParams` | 默认 | `AgentRequest` → `drive_session`（`model`、`serviceTier`） | 是 |
| 4 | `thread/resume` | 请求（有 `id`） | `ThreadResumeParams` | 默认 | — | 否 |
| 5 | `thread/fork` | 请求（有 `id`） | `ThreadForkParams` | 默认 | — | 否 |
| 6 | `thread/archive` | 请求（有 `id`） | `ThreadArchiveParams` | 默认 | — | 否 |
| 7 | `thread/delete` | 请求（有 `id`） | `ThreadDeleteParams` | 默认 | — | 否 |
| 8 | `thread/unsubscribe` | 请求（有 `id`） | `ThreadUnsubscribeParams` | 默认 | — | 否 |
| 9 | `thread/increment_elicitation` | 请求（有 `id`） | `ThreadIncrementElicitationParams` | 实验性 | — | 否 |
| 10 | `thread/decrement_elicitation` | 请求（有 `id`） | `ThreadDecrementElicitationParams` | 实验性 | — | 否 |
| 11 | `thread/name/set` | 请求（有 `id`） | `ThreadSetNameParams` | 默认 | — | 否 |
| 12 | `thread/goal/set` | 请求（有 `id`） | `ThreadGoalSetParams` | 默认 | — | 否 |
| 13 | `thread/goal/get` | 请求（有 `id`） | `ThreadGoalGetParams` | 默认 | — | 否 |
| 14 | `thread/goal/clear` | 请求（有 `id`） | `ThreadGoalClearParams` | 默认 | — | 否 |
| 15 | `thread/queue/add` | 请求（有 `id`） | `ThreadQueueAddParams` | 实验性 | — | 否 |
| 16 | `thread/queue/list` | 请求（有 `id`） | `ThreadQueueListParams` | 实验性 | — | 否 |
| 17 | `thread/queue/update` | 请求（有 `id`） | `ThreadQueueUpdateParams` | 实验性 | — | 否 |
| 18 | `thread/queue/delete` | 请求（有 `id`） | `ThreadQueueDeleteParams` | 实验性 | — | 否 |
| 19 | `thread/queue/reorder` | 请求（有 `id`） | `ThreadQueueReorderParams` | 实验性 | — | 否 |
| 20 | `thread/queue/start` | 请求（有 `id`） | `ThreadQueueStartParams` | 实验性 | — | 否 |
| 21 | `thread/metadata/update` | 请求（有 `id`） | `ThreadMetadataUpdateParams` | 默认 | — | 否 |
| 22 | `thread/section/move` | 请求（有 `id`） | `ThreadSectionMoveParams` | 默认 | — | 否 |
| 23 | `thread/settings/update` | 请求（有 `id`） | `ThreadSettingsUpdateParams` | 实验性 | — | 否 |
| 24 | `thread/memoryMode/set` | 请求（有 `id`） | `ThreadMemoryModeSetParams` | 实验性 | — | 否 |
| 25 | `memory/reset` | 请求（有 `id`） | `无` | 实验性 | — | 否 |
| 26 | `thread/unarchive` | 请求（有 `id`） | `ThreadUnarchiveParams` | 默认 | — | 否 |
| 27 | `thread/compact/start` | 请求（有 `id`） | `ThreadCompactStartParams` | 默认 | — | 否 |
| 28 | `thread/shellCommand` | 请求（有 `id`） | `ThreadShellCommandParams` | 默认 | — | 否 |
| 29 | `thread/approveGuardianDeniedAction` | 请求（有 `id`） | `ThreadApproveGuardianDeniedActionParams` | 默认 | — | 否 |
| 30 | `thread/backgroundTerminals/clean` | 请求（有 `id`） | `ThreadBackgroundTerminalsCleanParams` | 实验性 | — | 否 |
| 31 | `thread/backgroundTerminals/list` | 请求（有 `id`） | `ThreadBackgroundTerminalsListParams` | 实验性 | — | 否 |
| 32 | `thread/backgroundTerminals/terminate` | 请求（有 `id`） | `ThreadBackgroundTerminalsTerminateParams` | 实验性 | — | 否 |
| 33 | `thread/rollback` | 请求（有 `id`） | `ThreadRollbackParams` | 默认；已弃用 | — | 否 |
| 34 | `thread/revert` | 请求（有 `id`） | `ThreadRevertParams` | 实验性 | — | 否 |
| 35 | `thread/list` | 请求（有 `id`） | `ThreadListParams` | 默认 | — | 否 |
| 36 | `project/list` | 请求（有 `id`） | `ProjectListParams` | 实验性 | — | 否 |
| 37 | `project/read` | 请求（有 `id`） | `ProjectReadParams` | 实验性 | — | 否 |
| 38 | `project/create` | 请求（有 `id`） | `ProjectCreateParams` | 实验性 | — | 否 |
| 39 | `project/import` | 请求（有 `id`） | `ProjectImportParams` | 实验性 | — | 否 |
| 40 | `project/update` | 请求（有 `id`） | `ProjectUpdateParams` | 实验性 | — | 否 |
| 41 | `project/move` | 请求（有 `id`） | `ProjectMoveParams` | 实验性 | — | 否 |
| 42 | `project/delete` | 请求（有 `id`） | `ProjectDeleteParams` | 实验性 | — | 否 |
| 43 | `threadSection/list` | 请求（有 `id`） | `ThreadSectionListParams` | 默认 | — | 否 |
| 44 | `threadSection/create` | 请求（有 `id`） | `ThreadSectionCreateParams` | 默认 | — | 否 |
| 45 | `threadSection/update` | 请求（有 `id`） | `ThreadSectionUpdateParams` | 默认 | — | 否 |
| 46 | `threadSection/delete` | 请求（有 `id`） | `ThreadSectionDeleteParams` | 默认 | — | 否 |
| 47 | `thread/search` | 请求（有 `id`） | `ThreadSearchParams` | 实验性 | — | 否 |
| 48 | `thread/searchOccurrences` | 请求（有 `id`） | `ThreadSearchOccurrencesParams` | 实验性 | — | 否 |
| 49 | `thread/loaded/list` | 请求（有 `id`） | `ThreadLoadedListParams` | 默认 | — | 否 |
| 50 | `thread/read` | 请求（有 `id`） | `ThreadReadParams` | 默认 | — | 否 |
| 51 | `thread/turns/list` | 请求（有 `id`） | `ThreadTurnsListParams` | 实验性 | — | 否 |
| 52 | `thread/items/list` | 请求（有 `id`） | `ThreadItemsListParams` | 实验性 | — | 否 |
| 53 | `thread/inject_items` | 请求（有 `id`） | `ThreadInjectItemsParams` | 默认 | — | 否 |
| 54 | `skills/list` | 请求（有 `id`） | `SkillsListParams` | 默认 | — | 否 |
| 55 | `skills/extraRoots/set` | 请求（有 `id`） | `SkillsExtraRootsSetParams` | 默认 | — | 否 |
| 56 | `hooks/list` | 请求（有 `id`） | `HooksListParams` | 默认 | — | 否 |
| 57 | `marketplace/add` | 请求（有 `id`） | `MarketplaceAddParams` | 默认 | — | 否 |
| 58 | `marketplace/remove` | 请求（有 `id`） | `MarketplaceRemoveParams` | 默认 | — | 否 |
| 59 | `marketplace/upgrade` | 请求（有 `id`） | `MarketplaceUpgradeParams` | 默认 | — | 否 |
| 60 | `plugin/list` | 请求（有 `id`） | `PluginListParams` | 默认 | — | 否 |
| 61 | `plugin/search` | 请求（有 `id`） | `PluginSearchParams` | 实验性 | — | 否 |
| 62 | `plugin/installed` | 请求（有 `id`） | `PluginInstalledParams` | 默认 | — | 否 |
| 63 | `plugin/read` | 请求（有 `id`） | `PluginReadParams` | 默认 | — | 否 |
| 64 | `plugin/skill/read` | 请求（有 `id`） | `PluginSkillReadParams` | 默认 | — | 否 |
| 65 | `plugin/share/save` | 请求（有 `id`） | `PluginShareSaveParams` | 默认 | — | 否 |
| 66 | `plugin/share/updateTargets` | 请求（有 `id`） | `PluginShareUpdateTargetsParams` | 默认 | — | 否 |
| 67 | `plugin/share/list` | 请求（有 `id`） | `PluginShareListParams` | 默认 | — | 否 |
| 68 | `plugin/share/checkout` | 请求（有 `id`） | `PluginShareCheckoutParams` | 默认 | — | 否 |
| 69 | `plugin/share/delete` | 请求（有 `id`） | `PluginShareDeleteParams` | 默认 | — | 否 |
| 70 | `app/read` | 请求（有 `id`） | `AppsReadParams` | 默认 | — | 否 |
| 71 | `app/list` | 请求（有 `id`） | `AppsListParams` | 默认 | — | 否 |
| 72 | `app/installed` | 请求（有 `id`） | `AppsInstalledParams` | 默认 | — | 否 |
| 73 | `fs/readFile` | 请求（有 `id`） | `FsReadFileParams` | 默认 | — | 否 |
| 74 | `fs/writeFile` | 请求（有 `id`） | `FsWriteFileParams` | 默认 | — | 否 |
| 75 | `fs/createDirectory` | 请求（有 `id`） | `FsCreateDirectoryParams` | 默认 | — | 否 |
| 76 | `fs/getMetadata` | 请求（有 `id`） | `FsGetMetadataParams` | 默认 | — | 否 |
| 77 | `fs/readDirectory` | 请求（有 `id`） | `FsReadDirectoryParams` | 默认 | — | 否 |
| 78 | `fs/remove` | 请求（有 `id`） | `FsRemoveParams` | 默认 | — | 否 |
| 79 | `fs/copy` | 请求（有 `id`） | `FsCopyParams` | 默认 | — | 否 |
| 80 | `fs/watch` | 请求（有 `id`） | `FsWatchParams` | 默认 | — | 否 |
| 81 | `fs/unwatch` | 请求（有 `id`） | `FsUnwatchParams` | 默认 | — | 否 |
| 82 | `skills/config/write` | 请求（有 `id`） | `SkillsConfigWriteParams` | 默认 | — | 否 |
| 83 | `plugin/install` | 请求（有 `id`） | `PluginInstallParams` | 默认 | — | 否 |
| 84 | `plugin/uninstall` | 请求（有 `id`） | `PluginUninstallParams` | 默认 | — | 否 |
| 85 | `turn/start` | 请求（有 `id`） | `TurnStartParams` | 默认 | `AgentRequest` → `drive_session`（`model`、`effort`、`serviceTier`） | 是 |
| 86 | `turn/steer` | 请求（有 `id`） | `TurnSteerParams` | 默认 | — | 否 |
| 87 | `turn/interrupt` | 请求（有 `id`） | `TurnInterruptParams` | 默认 | `AgentInterruptHandle` → `CodexTurnSession::request_interrupt_inner`（`threadId`、`turnId`） | 是 |
| 88 | `thread/realtime/start` | 请求（有 `id`） | `ThreadRealtimeStartParams` | 实验性 | — | 否 |
| 89 | `thread/realtime/appendAudio` | 请求（有 `id`） | `ThreadRealtimeAppendAudioParams` | 实验性 | — | 否 |
| 90 | `thread/realtime/appendText` | 请求（有 `id`） | `ThreadRealtimeAppendTextParams` | 实验性 | — | 否 |
| 91 | `thread/realtime/appendSpeech` | 请求（有 `id`） | `ThreadRealtimeAppendSpeechParams` | 实验性 | — | 否 |
| 92 | `thread/realtime/stop` | 请求（有 `id`） | `ThreadRealtimeStopParams` | 实验性 | — | 否 |
| 93 | `thread/timeline/list` | 请求（有 `id`） | `ThreadTimelineListParams` | 实验性 | — | 否 |
| 94 | `thread/realtime/listVoices` | 请求（有 `id`） | `ThreadRealtimeListVoicesParams` | 实验性 | — | 否 |
| 95 | `review/start` | 请求（有 `id`） | `ReviewStartParams` | 默认 | — | 否 |
| 96 | `model/list` | 请求（有 `id`） | `ModelListParams` | 默认 | `CodexAppServerBackend::load_model_catalog` → `drive_model_catalog` → `AgentModelCatalog` | 是 |
| 97 | `modelProvider/capabilities/read` | 请求（有 `id`） | `ModelProviderCapabilitiesReadParams` | 默认 | — | 否 |
| 98 | `experimentalFeature/list` | 请求（有 `id`） | `ExperimentalFeatureListParams` | 默认 | — | 否 |
| 99 | `permissionProfile/list` | 请求（有 `id`） | `PermissionProfileListParams` | 默认 | — | 否 |
| 100 | `experimentalFeature/enablement/set` | 请求（有 `id`） | `ExperimentalFeatureEnablementSetParams` | 默认 | — | 否 |
| 101 | `remoteControl/enable` | 请求（有 `id`） | `无` | 实验性 | — | 否 |
| 102 | `remoteControl/disable` | 请求（有 `id`） | `无` | 实验性 | — | 否 |
| 103 | `remoteControl/status/read` | 请求（有 `id`） | `无` | 实验性 | — | 否 |
| 104 | `remoteControl/pairing/start` | 请求（有 `id`） | `RemoteControlPairingStartParams` | 实验性 | — | 否 |
| 105 | `remoteControl/pairing/status` | 请求（有 `id`） | `RemoteControlPairingStatusParams` | 实验性 | — | 否 |
| 106 | `remoteControl/client/list` | 请求（有 `id`） | `RemoteControlClientsListParams` | 实验性 | — | 否 |
| 107 | `remoteControl/client/revoke` | 请求（有 `id`） | `RemoteControlClientsRevokeParams` | 实验性 | — | 否 |
| 108 | `collaborationMode/list` | 请求（有 `id`） | `CollaborationModeListParams` | 实验性 | — | 否 |
| 109 | `mock/experimentalMethod` | 请求（有 `id`） | `MockExperimentalMethodParams` | 实验性 | — | 否 |
| 110 | `environment/add` | 请求（有 `id`） | `EnvironmentAddParams` | 实验性 | — | 否 |
| 111 | `environment/info` | 请求（有 `id`） | `EnvironmentInfoParams` | 实验性 | — | 否 |
| 112 | `environment/status` | 请求（有 `id`） | `EnvironmentStatusParams` | 实验性 | — | 否 |
| 113 | `mcpServer/oauth/login` | 请求（有 `id`） | `McpServerOauthLoginParams` | 默认 | — | 否 |
| 114 | `config/mcpServer/reload` | 请求（有 `id`） | `无` | 默认 | — | 否 |
| 115 | `mcpServerStatus/list` | 请求（有 `id`） | `ListMcpServerStatusParams` | 默认 | — | 否 |
| 116 | `mcpServer/resource/read` | 请求（有 `id`） | `McpResourceReadParams` | 默认 | — | 否 |
| 117 | `mcpServer/event/stream/start` | 请求（有 `id`） | `McpServerEventStreamStartParams` | 实验性 | — | 否 |
| 118 | `mcpServer/event/stream/stop` | 请求（有 `id`） | `McpServerEventStreamStopParams` | 实验性 | — | 否 |
| 119 | `mcpServer/tool/call` | 请求（有 `id`） | `McpServerToolCallParams` | 默认 | — | 否 |
| 120 | `windowsSandbox/setupStart` | 请求（有 `id`） | `WindowsSandboxSetupStartParams` | 默认 | — | 否 |
| 121 | `windowsSandbox/readiness` | 请求（有 `id`） | `无` | 默认 | — | 否 |
| 122 | `account/login/start` | 请求（有 `id`） | `LoginAccountParams` | 默认 | — | 否 |
| 123 | `account/bedrock/discover` | 请求（有 `id`） | `BedrockDiscoverParams` | 实验性 | — | 否 |
| 124 | `account/bedrock/setup` | 请求（有 `id`） | `BedrockSetupParams` | 实验性 | — | 否 |
| 125 | `account/login/cancel` | 请求（有 `id`） | `CancelLoginAccountParams` | 默认 | — | 否 |
| 126 | `account/logout` | 请求（有 `id`） | `无` | 默认 | — | 否 |
| 127 | `account/rateLimits/read` | 请求（有 `id`） | `无` | 默认 | — | 否 |
| 128 | `account/rateLimitResetCredit/consume` | 请求（有 `id`） | `ConsumeAccountRateLimitResetCreditParams` | 默认 | — | 否 |
| 129 | `account/usage/read` | 请求（有 `id`） | `无` | 默认 | — | 否 |
| 130 | `account/workspaceMessages/read` | 请求（有 `id`） | `无` | 默认 | — | 否 |
| 131 | `account/sendAddCreditsNudgeEmail` | 请求（有 `id`） | `SendAddCreditsNudgeEmailParams` | 默认 | — | 否 |
| 132 | `feedback/upload` | 请求（有 `id`） | `FeedbackUploadParams` | 默认 | — | 否 |
| 133 | `command/exec` | 请求（有 `id`） | `CommandExecParams` | 默认 | — | 否 |
| 134 | `command/exec/write` | 请求（有 `id`） | `CommandExecWriteParams` | 默认 | — | 否 |
| 135 | `command/exec/terminate` | 请求（有 `id`） | `CommandExecTerminateParams` | 默认 | — | 否 |
| 136 | `command/exec/resize` | 请求（有 `id`） | `CommandExecResizeParams` | 默认 | — | 否 |
| 137 | `process/spawn` | 请求（有 `id`） | `ProcessSpawnParams` | 实验性 | — | 否 |
| 138 | `process/writeStdin` | 请求（有 `id`） | `ProcessWriteStdinParams` | 实验性 | — | 否 |
| 139 | `process/kill` | 请求（有 `id`） | `ProcessKillParams` | 实验性 | — | 否 |
| 140 | `process/resizePty` | 请求（有 `id`） | `ProcessResizePtyParams` | 实验性 | — | 否 |
| 141 | `config/read` | 请求（有 `id`） | `ConfigReadParams` | 默认 | — | 否 |
| 142 | `externalAgentConfig/detect` | 请求（有 `id`） | `ExternalAgentConfigDetectParams` | 默认 | — | 否 |
| 143 | `externalAgentConfig/import` | 请求（有 `id`） | `ExternalAgentConfigImportParams` | 默认 | — | 否 |
| 144 | `externalAgentConfig/import/recordHistory` | 请求（有 `id`） | `ExternalAgentConfigImportHistoryRecordParams` | 默认 | — | 否 |
| 145 | `externalAgentConfig/import/readHistories` | 请求（有 `id`） | `无` | 默认 | — | 否 |
| 146 | `config/value/write` | 请求（有 `id`） | `ConfigValueWriteParams` | 默认 | — | 否 |
| 147 | `config/batchWrite` | 请求（有 `id`） | `ConfigBatchWriteParams` | 默认 | — | 否 |
| 148 | `configRequirements/read` | 请求（有 `id`） | `无` | 默认 | — | 否 |
| 149 | `account/read` | 请求（有 `id`） | `GetAccountParams` | 默认 | — | 否 |
| 150 | `fuzzyFileSearch` | 请求（有 `id`） | `FuzzyFileSearchParams` | 默认 | — | 否 |
| 151 | `fuzzyFileSearch/sessionStart` | 请求（有 `id`） | `FuzzyFileSearchSessionStartParams` | 实验性 | — | 否 |
| 152 | `fuzzyFileSearch/sessionUpdate` | 请求（有 `id`） | `FuzzyFileSearchSessionUpdateParams` | 实验性 | — | 否 |
| 153 | `fuzzyFileSearch/sessionStop` | 请求（有 `id`） | `FuzzyFileSearchSessionStopParams` | 实验性 | — | 否 |

### 服务端 → 客户端请求（ServerRequest，11 个）

| # | Method | 消息形式 | Params schema | 能力门槛 / 状态 | 内部方法 | 是否接入 |
|---:|---|---|---|---|---|:---:|
| 1 | `item/commandExecution/requestApproval` | 反向请求（有 `id`） | `CommandExecutionRequestApprovalParams` | 默认 | — | 否 |
| 2 | `item/fileChange/requestApproval` | 反向请求（有 `id`） | `FileChangeRequestApprovalParams` | 默认 | — | 否 |
| 3 | `item/tool/requestUserInput` | 反向请求（有 `id`） | `ToolRequestUserInputParams` | 默认 | — | 否 |
| 4 | `mcpServer/elicitation/request` | 反向请求（有 `id`） | `McpServerElicitationRequestParams` | 默认 | — | 否 |
| 5 | `item/permissions/requestApproval` | 反向请求（有 `id`） | `PermissionsRequestApprovalParams` | 默认 | — | 否 |
| 6 | `item/tool/call` | 反向请求（有 `id`） | `DynamicToolCallParams` | 默认 | — | 否 |
| 7 | `account/chatgptAuthTokens/refresh` | 反向请求（有 `id`） | `ChatgptAuthTokensRefreshParams` | 默认 | — | 否 |
| 8 | `attestation/generate` | 反向请求（有 `id`） | `AttestationGenerateParams` | 默认 | — | 否 |
| 9 | `currentTime/read` | 反向请求（有 `id`） | `CurrentTimeReadParams` | 实验性 | — | 否 |
| 10 | `applyPatchApproval` | 反向请求（有 `id`） | `ApplyPatchApprovalParams` | 默认；已弃用 | — | 否 |
| 11 | `execCommandApproval` | 反向请求（有 `id`） | `ExecCommandApprovalParams` | 默认；已弃用 | — | 否 |

### 客户端 → 服务端通知（ClientNotification，1 个）

| # | Method | 消息形式 | Params schema | 能力门槛 / 状态 | 内部方法 | 是否接入 |
|---:|---|---|---|---|---|:---:|
| 1 | `initialized` | 通知（无 `id`） | `无` | 默认 | `initialize_connection`（目录与 prompt 连接共用） | 是 |

### 服务端 → 客户端通知（ServerNotification，79 个）

| # | Method | 消息形式 | Params schema | 能力门槛 / 状态 | 内部方法 | 是否接入 |
|---:|---|---|---|---|---|:---:|
| 1 | `error` | 通知（无 `id`） | `ErrorNotification` | 默认 | — | 否 |
| 2 | `thread/started` | 通知（无 `id`） | `ThreadStartedNotification` | 默认 | `PASSIVE_SERVER_METHODS` → no-op（不生成 `AgentEvent`） | 已知（no-op） |
| 3 | `thread/status/changed` | 通知（无 `id`） | `ThreadStatusChangedNotification` | 默认 | `PASSIVE_SERVER_METHODS` → no-op（不生成 `AgentEvent`） | 已知（no-op） |
| 4 | `thread/archived` | 通知（无 `id`） | `ThreadArchivedNotification` | 默认 | — | 否 |
| 5 | `thread/deleted` | 通知（无 `id`） | `ThreadDeletedNotification` | 默认 | — | 否 |
| 6 | `thread/unarchived` | 通知（无 `id`） | `ThreadUnarchivedNotification` | 默认 | — | 否 |
| 7 | `thread/closed` | 通知（无 `id`） | `ThreadClosedNotification` | 默认 | — | 否 |
| 8 | `thread/reverted` | 通知（无 `id`） | `ThreadRevertedNotification` | 默认 | — | 否 |
| 9 | `skills/changed` | 通知（无 `id`） | `SkillsChangedNotification` | 默认 | — | 否 |
| 10 | `thread/name/updated` | 通知（无 `id`） | `ThreadNameUpdatedNotification` | 默认 | — | 否 |
| 11 | `thread/goal/updated` | 通知（无 `id`） | `ThreadGoalUpdatedNotification` | 默认 | — | 否 |
| 12 | `thread/goal/cleared` | 通知（无 `id`） | `ThreadGoalClearedNotification` | 默认 | — | 否 |
| 13 | `thread/queue/changed` | 通知（无 `id`） | `ThreadQueueChangedNotification` | 默认 | — | 否 |
| 14 | `project/changed` | 通知（无 `id`） | `ProjectChangedNotification` | 默认 | — | 否 |
| 15 | `thread/project/updated` | 通知（无 `id`） | `ThreadProjectUpdatedNotification` | 默认 | — | 否 |
| 16 | `thread/environment/connected` | 通知（无 `id`） | `EnvironmentConnectionNotification` | 默认 | — | 否 |
| 17 | `thread/environment/disconnected` | 通知（无 `id`） | `EnvironmentConnectionNotification` | 默认 | — | 否 |
| 18 | `thread/settings/updated` | 通知（无 `id`） | `ThreadSettingsUpdatedNotification` | 默认 | — | 否 |
| 19 | `thread/tokenUsage/updated` | 通知（无 `id`） | `ThreadTokenUsageUpdatedNotification` | 默认 | `PASSIVE_SERVER_METHODS` → no-op（不生成 `AgentEvent`） | 已知（no-op） |
| 20 | `turn/started` | 通知（无 `id`） | `TurnStartedNotification` | 默认 | `PASSIVE_SERVER_METHODS` → no-op（不生成 `AgentEvent`） | 已知（no-op） |
| 21 | `hook/started` | 通知（无 `id`） | `HookStartedNotification` | 默认 | — | 否 |
| 22 | `turn/completed` | 通知（无 `id`） | `TurnCompletedNotification` | 默认 | `drive_session` → `AgentEvent::Completed` / `AgentEvent::Interrupted` / `AgentEvent::Failed(String)` → `ComposerView::apply_agent_event_batch` | 是 |
| 23 | `hook/completed` | 通知（无 `id`） | `HookCompletedNotification` | 默认 | — | 否 |
| 24 | `turn/diff/updated` | 通知（无 `id`） | `TurnDiffUpdatedNotification` | 默认 | — | 否 |
| 25 | `turn/plan/updated` | 通知（无 `id`） | `TurnPlanUpdatedNotification` | 默认 | `PASSIVE_SERVER_METHODS` → no-op（不生成 `AgentEvent`） | 已知（no-op） |
| 26 | `item/started` | 通知（无 `id`） | `ItemStartedNotification` | 默认 | `drive_session` → `AgentEvent::AssistantMessageStarted { item_id }` / `AgentEvent::CommandStarted(CommandExecution)` → `ComposerView::apply_agent_event_batch` | 是（`agentMessage`、`commandExecution`） |
| 27 | `item/autoApprovalReview/started` | 通知（无 `id`） | `ItemGuardianApprovalReviewStartedNotification` | 默认 | — | 否 |
| 28 | `item/autoApprovalReview/completed` | 通知（无 `id`） | `ItemGuardianApprovalReviewCompletedNotification` | 默认 | — | 否 |
| 29 | `autoApprovalReview/strictReviewRequired` | 通知（无 `id`） | `StrictReviewRequiredNotification` | 默认 | — | 否 |
| 30 | `item/completed` | 通知（无 `id`） | `ItemCompletedNotification` | 默认 | `drive_session` → `AgentEvent::CommandCompleted(CommandExecution)` / `AgentEvent::TextDelta(String)` → `ComposerView::apply_agent_event_batch` | 是（`agentMessage`、`commandExecution`） |
| 31 | `item/agentMessage/delta` | 通知（无 `id`） | `AgentMessageDeltaNotification` | 默认 | `drive_session` → `AgentEvent::TextDelta(String)` → `ComposerView::apply_agent_event_batch` | 是 |
| 32 | `item/plan/delta` | 通知（无 `id`） | `PlanDeltaNotification` | 默认 | — | 否 |
| 33 | `command/exec/outputDelta` | 通知（无 `id`） | `CommandExecOutputDeltaNotification` | 默认 | — | 否 |
| 34 | `process/outputDelta` | 通知（无 `id`） | `ProcessOutputDeltaNotification` | 默认 | — | 否 |
| 35 | `process/exited` | 通知（无 `id`） | `ProcessExitedNotification` | 默认 | — | 否 |
| 36 | `item/commandExecution/outputDelta` | 通知（无 `id`） | `CommandExecutionOutputDeltaNotification` | 默认 | `drive_session` → `AgentEvent::CommandOutputDelta { item_id, delta }` → `ComposerView::apply_agent_event_batch` | 是 |
| 37 | `item/commandExecution/terminalInteraction` | 通知（无 `id`） | `TerminalInteractionNotification` | 默认 | — | 否 |
| 38 | `item/fileChange/outputDelta` | 通知（无 `id`） | `FileChangeOutputDeltaNotification` | 默认；已弃用 | — | 否 |
| 39 | `item/fileChange/patchUpdated` | 通知（无 `id`） | `FileChangePatchUpdatedNotification` | 默认 | — | 否 |
| 40 | `serverRequest/resolved` | 通知（无 `id`） | `ServerRequestResolvedNotification` | 默认 | — | 否 |
| 41 | `item/mcpToolCall/progress` | 通知（无 `id`） | `McpToolCallProgressNotification` | 默认 | — | 否 |
| 42 | `mcpServer/oauthLogin/completed` | 通知（无 `id`） | `McpServerOauthLoginCompletedNotification` | 默认 | — | 否 |
| 43 | `mcpServer/startupStatus/updated` | 通知（无 `id`） | `McpServerStatusUpdatedNotification` | 默认 | `PASSIVE_SERVER_METHODS` → no-op（不生成 `AgentEvent`） | 已知（no-op） |
| 44 | `mcpServer/event/stream/notification` | 通知（无 `id`） | `McpServerEventStreamNotification` | 默认 | — | 否 |
| 45 | `account/updated` | 通知（无 `id`） | `AccountUpdatedNotification` | 默认 | — | 否 |
| 46 | `account/rateLimits/updated` | 通知（无 `id`） | `AccountRateLimitsUpdatedNotification` | 默认 | `PASSIVE_SERVER_METHODS` → no-op（不生成 `AgentEvent`） | 已知（no-op） |
| 47 | `app/list/updated` | 通知（无 `id`） | `AppListUpdatedNotification` | 默认 | — | 否 |
| 48 | `remoteControl/status/changed` | 通知（无 `id`） | `RemoteControlStatusChangedNotification` | 默认 | `PASSIVE_SERVER_METHODS` → no-op（不生成 `AgentEvent`） | 已知（no-op） |
| 49 | `externalAgentConfig/import/progress` | 通知（无 `id`） | `ExternalAgentConfigImportProgressNotification` | 默认 | — | 否 |
| 50 | `externalAgentConfig/import/completed` | 通知（无 `id`） | `ExternalAgentConfigImportCompletedNotification` | 默认 | — | 否 |
| 51 | `fs/changed` | 通知（无 `id`） | `FsChangedNotification` | 默认 | — | 否 |
| 52 | `item/reasoning/summaryTextDelta` | 通知（无 `id`） | `ReasoningSummaryTextDeltaNotification` | 默认 | — | 否 |
| 53 | `item/reasoning/summaryPartAdded` | 通知（无 `id`） | `ReasoningSummaryPartAddedNotification` | 默认 | — | 否 |
| 54 | `item/reasoning/textDelta` | 通知（无 `id`） | `ReasoningTextDeltaNotification` | 默认 | — | 否 |
| 55 | `thread/compacted` | 通知（无 `id`） | `ContextCompactedNotification` | 默认；已弃用 | — | 否 |
| 56 | `model/rerouted` | 通知（无 `id`） | `ModelReroutedNotification` | 默认 | `drive_session` → `AgentEvent::ModelRerouted` → `ComposerView::apply_agent_event_batch` | 是 |
| 57 | `model/verification` | 通知（无 `id`） | `ModelVerificationNotification` | 默认 | `drive_session` → `AgentEvent::ModelVerificationRequired` → `ComposerView::apply_agent_event_batch` | 是 |
| 58 | `turn/moderationMetadata` | 通知（无 `id`） | `TurnModerationMetadataNotification` | 默认 | — | 否 |
| 59 | `model/safetyBuffering/updated` | 通知（无 `id`） | `ModelSafetyBufferingUpdatedNotification` | 默认 | `drive_session` → `AgentEvent::ModelSafetyBufferingUpdated` → `ComposerView::apply_agent_event_batch` | 是 |
| 60 | `warning` | 通知（无 `id`） | `WarningNotification` | 默认 | — | 否 |
| 61 | `guardianWarning` | 通知（无 `id`） | `GuardianWarningNotification` | 默认 | — | 否 |
| 62 | `deprecationNotice` | 通知（无 `id`） | `DeprecationNoticeNotification` | 默认 | — | 否 |
| 63 | `configWarning` | 通知（无 `id`） | `ConfigWarningNotification` | 默认 | — | 否 |
| 64 | `fuzzyFileSearch/sessionUpdated` | 通知（无 `id`） | `FuzzyFileSearchSessionUpdatedNotification` | 默认 | — | 否 |
| 65 | `fuzzyFileSearch/sessionCompleted` | 通知（无 `id`） | `FuzzyFileSearchSessionCompletedNotification` | 默认 | — | 否 |
| 66 | `thread/realtime/started` | 通知（无 `id`） | `ThreadRealtimeStartedNotification` | 默认 | — | 否 |
| 67 | `thread/realtime/itemAdded` | 通知（无 `id`） | `ThreadRealtimeItemAddedNotification` | 默认 | — | 否 |
| 68 | `thread/realtime/item/started` | 通知（无 `id`） | `ThreadRealtimeItemStartedNotification` | 默认 | — | 否 |
| 69 | `thread/realtime/item/transcript/delta` | 通知（无 `id`） | `ThreadRealtimeItemTranscriptDeltaNotification` | 默认 | — | 否 |
| 70 | `thread/realtime/item/completed` | 通知（无 `id`） | `ThreadRealtimeItemCompletedNotification` | 默认 | — | 否 |
| 71 | `thread/realtime/transcript/delta` | 通知（无 `id`） | `ThreadRealtimeTranscriptDeltaNotification` | 默认 | — | 否 |
| 72 | `thread/realtime/transcript/done` | 通知（无 `id`） | `ThreadRealtimeTranscriptDoneNotification` | 默认 | — | 否 |
| 73 | `thread/realtime/outputAudio/delta` | 通知（无 `id`） | `ThreadRealtimeOutputAudioDeltaNotification` | 默认 | — | 否 |
| 74 | `thread/realtime/sdp` | 通知（无 `id`） | `ThreadRealtimeSdpNotification` | 默认 | — | 否 |
| 75 | `thread/realtime/error` | 通知（无 `id`） | `ThreadRealtimeErrorNotification` | 默认 | — | 否 |
| 76 | `thread/realtime/closed` | 通知（无 `id`） | `ThreadRealtimeClosedNotification` | 默认 | — | 否 |
| 77 | `windows/worldWritableWarning` | 通知（无 `id`） | `WindowsWorldWritableWarningNotification` | 默认 | — | 否 |
| 78 | `windowsSandbox/setupCompleted` | 通知（无 `id`） | `WindowsSandboxSetupCompletedNotification` | 默认 | — | 否 |
| 79 | `account/login/completed` | 通知（无 `id`） | `AccountLoginCompletedNotification` | 默认 | — | 否 |

## 本次验证结果

- 本机版本：`codex-cli 0.150.1`。
- 重新执行 `codex app-server generate-json-schema --experimental` 和默认 schema 生成；实验 schema 仍为 153 个 ClientRequest、11 个 ServerRequest、1 个 ClientNotification、79 个 ServerNotification（合计 244），默认 schema 合计 185。
- 对真实 app-server 以 `limit: 2` 调用 `model/list`：通过 4 页及连续 `nextCursor` 拉取到 7 个可见模型；响应包含 `displayName`、`isDefault`、`supportedReasoningEfforts`、`defaultReasoningEffort`、`serviceTiers`、`defaultServiceTier`，与本机生成 schema 一致。
- `cargo fmt -- --check`：通过。
- `cargo test`：111 个测试全部通过；新增覆盖 `turn/interrupt` 的 `threadId`/`turnId` 参数、开始阶段排队、重复/结束后停止、`Stopping` → `Stopped`/`Failed` 状态转换，以及子进程 kill + wait 回收；原有模型目录、流式文本、命令输出和 UI 行为测试继续通过。
- `cargo check --all-targets`：通过。

## Schema 生成与版本同步

完整字段级定义不适合手工复制维护。接入项目时应由实际运行的 CLI 生成，这样请求参数、响应结果、枚举与通知 payload 都和二进制版本完全一致：

```bash
# 默认协议面
codex app-server generate-json-schema --out ./schemas/app-server
codex app-server generate-ts --out ./schemas/app-server-ts

# 包含实验性方法和字段
codex app-server generate-json-schema --experimental --out ./schemas/app-server-experimental
codex app-server generate-ts --experimental --out ./schemas/app-server-ts-experimental
```

升级 Codex CLI 后应重新生成并对 schema 做 diff。不要只依赖本文件中的方法名，因为 App Server 仍在演进，实验性方法尤其可能变化。

## 重要接入注意事项

- 连接完成后，任何业务请求之前都必须完成 `initialize` → `initialized` 握手。
- stdio 模式下 stdout 是协议流；日志应从 stderr 读取，避免把非 JSON 文本混入解析器。
- 服务端反向请求必须由客户端响应，尤其是命令执行审批、文件修改审批、用户输入、动态工具调用和 MCP elicitation。
- `thread/shellCommand` 按官方文档是在沙箱外执行，不继承线程的 sandbox policy；UI 必须明确呈现其权限风险。
- `thread/delete` 是永久删除，`fs/remove` 会修改文件系统；接入层应提供显式确认与审计。
- WebSocket 目前是实验性传输；非本机监听必须配置鉴权并放在 TLS 后。
- 若初始化时没有开启 `experimentalApi`，调用实验方法或传递实验字段会被服务器拒绝。

## 来源

- [Codex App Server 官方文档](https://learn.chatgpt.com/docs/app-server)
- [Codex 文档索引](https://learn.chatgpt.com/docs/llms.txt)
- 本机 `codex-cli 0.150.1` 生成的 `codex_app_server_protocol.schemas.json`（默认及 `--experimental` 两套）
