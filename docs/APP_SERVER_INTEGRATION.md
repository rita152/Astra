# Codex app-server 协议接入总表

> 当前 Codex CLI 版本：`codex-cli 0.151.0`；运行时事实来源：`src/agent/codex.rs`、`src/agent/mod.rs`、`src/components/composer.rs`。

客户端初始化时启用 experimental API。下表是当前协议的唯一维护来源，完整列出 250 个 JSON-RPC 方法：157 个客户端请求、11 个服务端请求、1 个客户端通知、81 个服务端通知。

当前接入统计：已接入 27、后端已接入 2、部分接入 3、未接入 218。未接入的客户端方法不会发送；未接入的服务端请求按原 id 回复 `-32601` 后 fail-fast；未接入的服务端通知收到即 fail-fast。升级 Codex CLI 时直接核对并更新本表。

状态口径：“已接入”表示本表声明的产品语义已形成真实协议收发、领域映射和必要 UI／副作用的完整闭环，不等于消费 schema 的每个可选字段；已知有效变体或安全相关字段尚未承接时标记为“部分接入”。

| 方法 | 方向与类型 | 协议范围 | 协议要点 | 运行时入口 | 领域映射 | UI／副作用 | 接入状态 | 兼容与测试 |
|---|---|---|---|---|---|---|---|---|
| `account/bedrock/discover` | 客户端请求 | 实验 | `params: BedrockDiscoverParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `account/bedrock/setup` | 客户端请求 | 实验 | `params: BedrockSetupParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `account/login/cancel` | 客户端请求 | 默认 | `params: CancelLoginAccountParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `account/login/start` | 客户端请求 | 默认 | `params: LoginAccountParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `account/logout` | 客户端请求 | 默认 | `params: undefined`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `account/rateLimitResetCredit/consume` | 客户端请求 | 默认 | `params: ConsumeAccountRateLimitResetCreditParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `account/rateLimits/read` | 客户端请求 | 默认 | `params: undefined`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `account/read` | 客户端请求 | 默认 | `params: GetAccountParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `account/sendAddCreditsNudgeEmail` | 客户端请求 | 默认 | `params: SendAddCreditsNudgeEmailParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `account/usage/read` | 客户端请求 | 默认 | `params?: GetAccountTokenUsageParams \| undefined`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `account/workspaceMessages/read` | 客户端请求 | 默认 | `params: undefined`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `app/installed` | 客户端请求 | 默认 | `params: AppsInstalledParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `app/list` | 客户端请求 | 默认 | `params: AppsListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `app/read` | 客户端请求 | 默认 | `params: AppsReadParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `collaborationMode/list` | 客户端请求 | 实验 | `params: CollaborationModeListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `command/exec` | 客户端请求 | 默认 | `params: CommandExecParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `command/exec/resize` | 客户端请求 | 默认 | `params: CommandExecResizeParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `command/exec/terminate` | 客户端请求 | 默认 | `params: CommandExecTerminateParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `command/exec/write` | 客户端请求 | 默认 | `params: CommandExecWriteParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `config/batchWrite` | 客户端请求 | 默认 | `params: ConfigBatchWriteParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `config/mcpServer/reload` | 客户端请求 | 默认 | `params: undefined`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `config/read` | 客户端请求 | 默认 | `params: ConfigReadParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `config/value/write` | 客户端请求 | 默认 | `params: ConfigValueWriteParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `configRequirements/read` | 客户端请求 | 默认 | `params: undefined`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `environment/add` | 客户端请求 | 实验 | `params: EnvironmentAddParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `environment/info` | 客户端请求 | 实验 | `params: EnvironmentInfoParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `environment/status` | 客户端请求 | 实验 | `params: EnvironmentStatusParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `experimentalFeature/enablement/set` | 客户端请求 | 默认 | `params: ExperimentalFeatureEnablementSetParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `experimentalFeature/list` | 客户端请求 | 默认 | `params: ExperimentalFeatureListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `externalAgentConfig/detect` | 客户端请求 | 默认 | `params: ExternalAgentConfigDetectParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `externalAgentConfig/import` | 客户端请求 | 默认 | `params: ExternalAgentConfigImportParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `externalAgentConfig/import/readHistories` | 客户端请求 | 默认 | `params: undefined`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `externalAgentConfig/import/recordHistory` | 客户端请求 | 默认 | `params: ExternalAgentConfigImportHistoryRecordParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `feedback/upload` | 客户端请求 | 默认 | `params: FeedbackUploadParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `fs/copy` | 客户端请求 | 默认 | `params: FsCopyParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `fs/createDirectory` | 客户端请求 | 默认 | `params: FsCreateDirectoryParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `fs/getMetadata` | 客户端请求 | 默认 | `params: FsGetMetadataParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `fs/readDirectory` | 客户端请求 | 默认 | `params: FsReadDirectoryParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `fs/readFile` | 客户端请求 | 默认 | `params: FsReadFileParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `fs/remove` | 客户端请求 | 默认 | `params: FsRemoveParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `fs/unwatch` | 客户端请求 | 默认 | `params: FsUnwatchParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `fs/watch` | 客户端请求 | 默认 | `params: FsWatchParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `fs/writeFile` | 客户端请求 | 默认 | `params: FsWriteFileParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `fuzzyFileSearch` | 客户端请求 | 默认 | `params: FuzzyFileSearchParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `fuzzyFileSearch/sessionStart` | 客户端请求 | 实验 | `params: FuzzyFileSearchSessionStartParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `fuzzyFileSearch/sessionStop` | 客户端请求 | 实验 | `params: FuzzyFileSearchSessionStopParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `fuzzyFileSearch/sessionUpdate` | 客户端请求 | 实验 | `params: FuzzyFileSearchSessionUpdateParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `getAuthStatus` | 客户端请求 | 默认 | `params: GetAuthStatusParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `getConversationSummary` | 客户端请求 | 默认 | `params: GetConversationSummaryParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `gitDiffToRemote` | 客户端请求 | 默认 | `params: GitDiffToRemoteParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `hooks/list` | 客户端请求 | 默认 | `params: HooksListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `initialize` | 客户端请求 | 默认 | `clientInfo`；`capabilities.experimentalApi=true`；按 id 等待响应 | `initialize_connection`、`initialize_turn_connection` | 无 | 建立可用连接 | 已接入 | 握手期间未知方法不会跳过；`handshake_wait_rejects_unknown_methods_instead_of_skipping_them` |
| `marketplace/add` | 客户端请求 | 默认 | `params: MarketplaceAddParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `marketplace/remove` | 客户端请求 | 默认 | `params: MarketplaceRemoveParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `marketplace/upgrade` | 客户端请求 | 默认 | `params: MarketplaceUpgradeParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `mcpServer/event/stream/start` | 客户端请求 | 实验 | `params: McpServerEventStreamStartParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `mcpServer/event/stream/stop` | 客户端请求 | 实验 | `params: McpServerEventStreamStopParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `mcpServer/oauth/login` | 客户端请求 | 默认 | `params: McpServerOauthLoginParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `mcpServer/resource/read` | 客户端请求 | 默认 | `params: McpResourceReadParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `mcpServer/tool/call` | 客户端请求 | 默认 | `params: McpServerToolCallParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `mcpServerStatus/list` | 客户端请求 | 默认 | `params: ListMcpServerStatusParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `memory/reset` | 客户端请求 | 实验 | `params: undefined`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `mock/experimentalMethod` | 客户端请求 | 实验 | `params: MockExperimentalMethodParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `model/list` | 客户端请求 | 默认 | `cursor`、`limit=50`、`includeHidden=false`；读取 `data`、`nextCursor` | `drive_model_catalog` | `AgentModelCatalog` | 填充模型、推理强度和 service tier 选项 | 已接入 | 空目录、重复 cursor、错误响应均失败；`model_catalog_accumulates_pages_and_maps_defaults_and_options` |
| `modelProvider/capabilities/read` | 客户端请求 | 默认 | `params: ModelProviderCapabilitiesReadParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `permissionProfile/list` | 客户端请求 | 默认 | `cursor=null`、`limit=100`、`cwd`；读取 profile `id/allowed/extends` | `drive_permission_profiles` | `Vec<AgentPermissionProfile>` | 当前无可见调用方 | 后端已接入 | 超过 100 项不静默截断；`permission_profile_list_maps_available_profiles` |
| `plugin/install` | 客户端请求 | 默认 | `params: PluginInstallParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `plugin/installed` | 客户端请求 | 默认 | `params: PluginInstalledParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `plugin/list` | 客户端请求 | 默认 | `params: PluginListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `plugin/read` | 客户端请求 | 默认 | `params: PluginReadParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `plugin/search` | 客户端请求 | 实验 | `params: PluginSearchParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `plugin/share/checkout` | 客户端请求 | 默认 | `params: PluginShareCheckoutParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `plugin/share/delete` | 客户端请求 | 默认 | `params: PluginShareDeleteParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `plugin/share/list` | 客户端请求 | 默认 | `params: PluginShareListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `plugin/share/save` | 客户端请求 | 默认 | `params: PluginShareSaveParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `plugin/share/updateTargets` | 客户端请求 | 默认 | `params: PluginShareUpdateTargetsParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `plugin/skill/read` | 客户端请求 | 默认 | `params: PluginSkillReadParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `plugin/uninstall` | 客户端请求 | 默认 | `params: PluginUninstallParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `process/kill` | 客户端请求 | 实验 | `params: ProcessKillParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `process/resizePty` | 客户端请求 | 实验 | `params: ProcessResizePtyParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `process/spawn` | 客户端请求 | 实验 | `params: ProcessSpawnParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `process/writeStdin` | 客户端请求 | 实验 | `params: ProcessWriteStdinParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `project/create` | 客户端请求 | 实验 | `params: ProjectCreateParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `project/delete` | 客户端请求 | 实验 | `params: ProjectDeleteParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `project/import` | 客户端请求 | 实验 | `params: ProjectImportParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `project/list` | 客户端请求 | 实验 | `params: ProjectListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `project/move` | 客户端请求 | 实验 | `params: ProjectMoveParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `project/read` | 客户端请求 | 实验 | `params: ProjectReadParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `project/update` | 客户端请求 | 实验 | `params: ProjectUpdateParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `remoteControl/client/list` | 客户端请求 | 实验 | `params: RemoteControlClientsListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `remoteControl/client/revoke` | 客户端请求 | 实验 | `params: RemoteControlClientsRevokeParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `remoteControl/disable` | 客户端请求 | 实验 | `params: RemoteControlDisableParams \| null`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `remoteControl/enable` | 客户端请求 | 实验 | `params: RemoteControlEnableParams \| null`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `remoteControl/pairing/start` | 客户端请求 | 实验 | `params: RemoteControlPairingStartParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `remoteControl/pairing/status` | 客户端请求 | 实验 | `params: RemoteControlPairingStatusParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `remoteControl/status/read` | 客户端请求 | 实验 | `params: undefined`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `review/start` | 客户端请求 | 默认 | `params: ReviewStartParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `server/diagnostics` | 客户端请求 | 实验 | `params: ServerDiagnosticsParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `skills/config/write` | 客户端请求 | 默认 | `params: SkillsConfigWriteParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `skills/extraRoots/set` | 客户端请求 | 默认 | `params: SkillsExtraRootsSetParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `skills/list` | 客户端请求 | 默认 | `params: SkillsListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/approveGuardianDeniedAction` | 客户端请求 | 默认 | `params: ThreadApproveGuardianDeniedActionParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/archive` | 客户端请求 | 默认 | `params: ThreadArchiveParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/backgroundTerminals/clean` | 客户端请求 | 实验 | `params: ThreadBackgroundTerminalsCleanParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/backgroundTerminals/list` | 客户端请求 | 实验 | `params: ThreadBackgroundTerminalsListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/backgroundTerminals/terminate` | 客户端请求 | 实验 | `params: ThreadBackgroundTerminalsTerminateParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/compact/start` | 客户端请求 | 默认 | `params: ThreadCompactStartParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/decrement_elicitation` | 客户端请求 | 实验 | `params: ThreadDecrementElicitationParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/delete` | 客户端请求 | 默认 | `params: ThreadDeleteParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/fork` | 客户端请求 | 默认 | `params: ThreadForkParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/goal/clear` | 客户端请求 | 默认 | `params: ThreadGoalClearParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/goal/get` | 客户端请求 | 默认 | `params: ThreadGoalGetParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/goal/set` | 客户端请求 | 默认 | `params: ThreadGoalSetParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/increment_elicitation` | 客户端请求 | 实验 | `params: ThreadIncrementElicitationParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/inject_items` | 客户端请求 | 默认 | `params: ThreadInjectItemsParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/items/list` | 客户端请求 | 默认 | `params: ThreadItemsListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/list` | 客户端请求 | 默认 | `params: ThreadListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/loaded/list` | 客户端请求 | 默认 | `params: ThreadLoadedListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/memoryMode/set` | 客户端请求 | 实验 | `params: ThreadMemoryModeSetParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/metadata/update` | 客户端请求 | 默认 | `params: ThreadMetadataUpdateParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/name/set` | 客户端请求 | 默认 | `params: ThreadSetNameParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/queue/add` | 客户端请求 | 实验 | `params: ThreadQueueAddParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/queue/delete` | 客户端请求 | 实验 | `params: ThreadQueueDeleteParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/queue/list` | 客户端请求 | 实验 | `params: ThreadQueueListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/queue/reorder` | 客户端请求 | 实验 | `params: ThreadQueueReorderParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/queue/start` | 客户端请求 | 实验 | `params: ThreadQueueStartParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/queue/update` | 客户端请求 | 实验 | `params: ThreadQueueUpdateParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/read` | 客户端请求 | 默认 | `params: ThreadReadParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/realtime/appendAudio` | 客户端请求 | 实验 | `params: ThreadRealtimeAppendAudioParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/realtime/appendSpeech` | 客户端请求 | 实验 | `params: ThreadRealtimeAppendSpeechParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/realtime/appendText` | 客户端请求 | 实验 | `params: ThreadRealtimeAppendTextParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/realtime/listVoices` | 客户端请求 | 实验 | `params: ThreadRealtimeListVoicesParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/realtime/start` | 客户端请求 | 实验 | `params: ThreadRealtimeStartParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/realtime/stop` | 客户端请求 | 实验 | `params: ThreadRealtimeStopParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/resume` | 客户端请求 | 默认 | 发送 `threadId`；要求 `result.thread.id` 与请求完全一致 | `drive_session` | 复用现有 thread id | 在原会话继续 turn | 已接入 | 失败时不回退创建新 thread；`resume_rpc_error_fails_closed_without_starting_or_turning`、`resume_response_requires_the_requested_thread_id` |
| `thread/revert` | 客户端请求 | 默认 | `params: ThreadRevertParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/rollback` | 客户端请求 | 默认 | `params: ThreadRollbackParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/search` | 客户端请求 | 实验 | `params: ThreadSearchParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/searchOccurrences` | 客户端请求 | 实验 | `params: ThreadSearchOccurrencesParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/section/move` | 客户端请求 | 默认 | `params: ThreadSectionMoveParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/settings/update` | 客户端请求 | 实验 | `threadId`、`approvalPolicy`、`approvalsReviewer`，以及 profile 或 `sandboxPolicy` | `drive_thread_settings_update` | `AgentThreadSettings` | 更新权限模式、有效 sandbox/profile 与错误提示 | 已接入 | 必须同时等到成功 response 和匹配 thread 的 `thread/settings/updated`；设置错误不得伪造成功状态 |
| `thread/shellCommand` | 客户端请求 | 默认 | `params: ThreadShellCommandParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/start` | 客户端请求 | 默认 | 发送 `cwd`、`ephemeral=false`、`serviceName`、`model`、`serviceTier`；读取 `result.thread.id` | `drive_session` | `AgentEvent::ThreadCreated` | Composer 保存新 thread id | 已接入 | 缺少 thread id 失败；完整创建路径由 `drives_one_complete_prompt_and_normalizes_stream_events` 覆盖 |
| `thread/timeline/list` | 客户端请求 | 实验 | `params: ThreadTimelineListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/turns/list` | 客户端请求 | 默认 | `params: ThreadTurnsListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/unarchive` | 客户端请求 | 默认 | `params: ThreadUnarchiveParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `thread/unsubscribe` | 客户端请求 | 默认 | `params: ThreadUnsubscribeParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `threadSection/create` | 客户端请求 | 默认 | `params: ThreadSectionCreateParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `threadSection/delete` | 客户端请求 | 默认 | `params: ThreadSectionDeleteParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `threadSection/list` | 客户端请求 | 默认 | `params: ThreadSectionListParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `threadSection/update` | 客户端请求 | 默认 | `params: ThreadSectionUpdateParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `turn/interrupt` | 客户端请求 | 默认 | 当前 `threadId`、`turnId`；同一 turn 只发送一次 | `turn_interrupt_request`、`AgentInterruptHandle` | `AgentInterruptOutcome`；终态由 `turn/completed` 决定 | Composer 进入 stopping，最终显示 stopped/failed | 已接入 | 重复、已终止和 abandon 路径有独立回归；`pending_interrupt_uses_the_active_thread_and_turn_and_waits_for_terminal_status` |
| `turn/settings/update` | 客户端请求 | 实验 | `params: TurnSettingsUpdateParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `turn/start` | 客户端请求 | 默认 | `threadId`、文本 `input`、`model`、`effort`、`serviceTier`；新 thread 另带权限字段；读取 `result.turn.id` | `drive_session` | 后续 `turn/started` 等事件 | Composer 进入 starting/streaming | 已接入 | response 前到达的 turn 消息先缓存，确定 turn id 后原子校验；失败不得泄漏延迟事件 |
| `turn/steer` | 客户端请求 | 默认 | `params: TurnSteerParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `windowsSandbox/readiness` | 客户端请求 | 默认 | `params: undefined`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `windowsSandbox/setupStart` | 客户端请求 | 默认 | `params: WindowsSandboxSetupStartParams`；当前客户端不发送 | — | 无 | 无 | 未接入 | 客户端不发送 |
| `account/chatgptAuthTokens/refresh` | 服务端请求 | 默认 | payload 当前不解析 | `respond_to_server_request(_on_session)` | 无 | 无 | 未接入 | 按原 id 回复 `-32601`，随后本地 fail-fast；通用未知反向请求测试保护 |
| `applyPatchApproval` | 服务端请求 | 默认 | legacy patch 审批，payload 当前不解析 | 同上 | 无 | 现有文件审批组件不代表协议已接入 | 未接入 | `-32601` 后 fail-fast |
| `attestation/generate` | 服务端请求 | 默认 | attestation payload 当前不解析 | 同上 | 无 | 无 | 未接入 | `-32601` 后 fail-fast |
| `currentTime/read` | 服务端请求 | 实验 | 时间读取 payload 当前不解析 | 同上 | 无 | 无 | 未接入 | `-32601` 后 fail-fast |
| `execCommandApproval` | 服务端请求 | 默认 | legacy command 审批，不能与 v2 item 请求混用 | 同上 | 无 | 不进入当前命令审批卡 | 未接入 | `-32601` 后 fail-fast |
| `item/commandExecution/requestApproval` | 服务端请求 | 默认 | 保留原始 id 类型并校验基础关联字段；当前只映射命令、原因、`networkApprovalContext.host` 与 accept/decline/execpolicy 相关 decision；未承接 `kind=writeStdin`、`approvalId`、网络 `protocol`、`additionalPermissions`、`proposedNetworkPolicyAmendments` 以及 `acceptForSession`/`applyNetworkPolicyAmendment` 等有效变体 | `respond_to_server_request_on_session`、`parse_command_approval_request`、统一 pending registry | 窄化的 `AgentCommandApprovalRequest` + `AgentApprovalHandle` | 普通命令或仅展示 host 的网络审批卡；附加文件／网络权限与网络协议尚不可见 | 部分接入 | 已覆盖原始 id、单次回复、execpolicy 与 response→resolved；服务端仅声明 `cancel` 时当前回复语义不同的 `decline`，其余未承接变体也尚未 fail-fast |
| `item/fileChange/requestApproval` | 服务端请求 | 默认 | 文件变更审批 payload 当前不解析 | 同上 | 无 | 文件审批组件仅有展示/测试能力，未接真实 RPC | 未接入 | `unsupported_file_change_server_request_is_rejected` 验证 `-32601` |
| `item/permissions/requestApproval` | 服务端请求 | 默认 | 严格读取 `threadId/turnId/itemId/cwd/startedAtMs`、nullable `environmentId/reason` 与 `RequestPermissionProfile`；保留 read/write、entries、glob 深度、path/glob/special path 和 nullable network；允许时原样返回请求权限子集，scope 映射 `turn/session`，拒绝返回空权限，UI 未选择时不返回 `strictAutoReview` | `respond_to_server_request_on_session`、`parse_permissions_approval_request`、统一 pending registry | `AgentPermissionsApprovalRequest` + `AgentPermissionsApprovalHandle` | 现有权限卡展示 cwd、reason、文件和网络权限；Allow once/session/Decline 调用真实 responder，响应后禁用并等待 resolved | 已接入 | 文件、网络、混合权限及三种决定、越权防护、重复操作、错误参数、清理与 resolved 回归均覆盖 |
| `item/tool/call` | 服务端请求 | 默认 | 动态工具调用 payload 当前不解析 | 同上 | 无 | 无 | 未接入 | `-32601` 后 fail-fast |
| `item/tool/requestUserInput` | 服务端请求 | 默认 | 严格读取 `threadId/turnId/itemId/questions/isBlocking` 与 nullable `autoResolutionMs`；保留每题 `id/header/question/options/isOther/isSecret`；按 schema 返回 question id 到字符串数组的 `answers` map，0.151.0 无取消结果分支 | `respond_to_server_request_on_session`、`parse_user_input_request`、统一 pending registry | `AgentUserInputRequest` + `AgentUserInputHandle`；答案 Debug 全量脱敏 | 复用现有多问题/Other/secret UI；提交后显示 submitting、禁用重复提交并等待 resolved | 已接入 | 单选、多答案、Other、secret、多问题、原始 id、精确 JSON response、重复提交、错误参数和终止清理均覆盖 |
| `mcpServer/elicitation/request` | 服务端请求 | 默认 | MCP elicitation payload 当前不解析 | 同上 | 无 | 无 | 未接入 | `-32601` 后 fail-fast |
| `initialized` | 客户端通知 | 默认 | `params={}`，无 id | 同上 | 无 | 解锁业务请求 | 已接入 | `drives_one_complete_prompt_and_normalizes_stream_events` 覆盖完整顺序 |
| `account/login/completed` | 服务端通知 | 默认 | `params: AccountLoginCompletedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `account/rateLimits/updated` | 服务端通知 | 默认 | 严格读取必填 `rateLimits` 稀疏快照；覆盖 limit id/name、primary/secondary 窗口、credits、individual limit、spend-control、plan type 与 reached type；窗口和额度数字按 int32/int64 校验，枚举仅接受 0.151.0 schema 值 | `parse_account_rate_limits_updated`、`parse_agent_notification`、`ensure_server_method_is_defined`；无 turn 事件流的短连接仍校验 schema | `AgentEvent::AccountRateLimitsUpdated` | 合并 GPUI 账户配额状态的可用字段；缺省或 null 不覆盖既有值；不生成对话活动、不结束 turn | 已接入 | 截图完整 payload、合法稀疏 payload、全部 plan/reached 枚举、缺字段/错类型/越界/未知枚举 fail-fast；`account_rate_limits_updated_is_validated_and_normalized`、`account_rate_limits_update_is_validated_on_an_app_scoped_connection`、`drives_one_complete_prompt_and_normalizes_stream_events`、`account_rate_limits_sparse_updates_merge_without_ending_the_turn` |
| `account/updated` | 服务端通知 | 默认 | `params: AccountUpdatedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `app/list/updated` | 服务端通知 | 默认 | `params: AppListUpdatedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `autoApprovalReview/strictReviewRequired` | 服务端通知 | 默认 | `params: StrictReviewRequiredNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `command/exec/outputDelta` | 服务端通知 | 默认 | `params: CommandExecOutputDeltaNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `configWarning` | 服务端通知 | 默认 | `summary`；可选 `details/path/range` | `parse_agent_notification` | `AgentEvent::ConfigWarning` | 配置警告 Notice；有 path 时可打开文件 | 已接入 | range 类型与行列字段严格校验 |
| `deprecationNotice` | 服务端通知 | 默认 | `params: DeprecationNoticeNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `error` | 服务端通知 | 默认 | `threadId`、`turnId`、`error.message`、可选 details、`willRetry` | `parse_agent_notification` | `AgentEvent::Error` | 可重试错误显示低强调活动；不可重试显示协议错误 | 已接入 | 通知本身不伪造 turn 终态；用户可见通知标准化测试覆盖 |
| `externalAgentConfig/import/completed` | 服务端通知 | 默认 | `params: ExternalAgentConfigImportCompletedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `externalAgentConfig/import/progress` | 服务端通知 | 默认 | `params: ExternalAgentConfigImportProgressNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `fs/changed` | 服务端通知 | 默认 | `params: FsChangedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `fuzzyFileSearch/sessionCompleted` | 服务端通知 | 默认 | `params: FuzzyFileSearchSessionCompletedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `fuzzyFileSearch/sessionUpdated` | 服务端通知 | 默认 | `params: FuzzyFileSearchSessionUpdatedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `guardianWarning` | 服务端通知 | 默认 | `params: GuardianWarningNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `hook/completed` | 服务端通知 | 默认 | `params: HookCompletedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `hook/started` | 服务端通知 | 默认 | `params: HookStartedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `item/agentMessage/delta` | 服务端通知 | 默认 | 严格读取 `threadId`、`turnId`、`itemId`、`delta` | `process_turn_message` | `AgentEvent::TextDelta` | 合并流式助手文本 | 已接入 | 缺少字段或字段类型错误立即 fail-fast；相邻 delta 在 UI 批次中合并但不跨事件边界 |
| `item/autoApprovalReview/completed` | 服务端通知 | 默认 | `params: ItemGuardianApprovalReviewCompletedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `item/autoApprovalReview/started` | 服务端通知 | 默认 | `params: ItemGuardianApprovalReviewStartedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `item/commandExecution/outputDelta` | 服务端通知 | 默认 | 严格读取 `threadId`、`turnId`、`itemId`、`delta` | `process_turn_message` | `AgentEvent::CommandOutputDelta` | 追加命令输出，缺少 started 时创建最小活动行 | 已接入 | 缺少字段或字段类型错误立即 fail-fast；当前 turn 归属校验与完整 prompt 回归覆盖 |
| `item/commandExecution/terminalInteraction` | 服务端通知 | 默认 | `params: TerminalInteractionNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `item/completed` | 服务端通知 | 默认 | 要求对象 `item` 及字符串 `type/id`；`userMessage` 严格读取 nullable `clientId` 与文本 `content`（含可选 `text_elements` 数组），`agentMessage` 严格读取 `text`，`commandExecution` 严格读取命令、actions、cwd、已知状态及 nullable 输出/exit code | `process_turn_message`、`validate_user_message` | `userMessage` → 确认已发送消息、不产生重复事件；其余为 `CommandCompleted` 或最终 `TextDelta` | `userMessage` 沿用发送时已建立的用户气泡；其余完成命令活动，或在未流式时补全助手消息 | 部分接入 | 仅接入文本型 `userMessage`、`agentMessage`、`commandExecution`；其他 item.type、非文本 user input、缺字段或错误类型立即 fail-fast；完整 prompt、userMessage 生命周期及边界测试覆盖 |
| `item/fileChange/outputDelta` | 服务端通知 | 默认 | `params: FileChangeOutputDeltaNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `item/fileChange/patchUpdated` | 服务端通知 | 默认 | `params: FileChangePatchUpdatedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `item/mcpToolCall/progress` | 服务端通知 | 默认 | `params: McpToolCallProgressNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `item/plan/delta` | 服务端通知 | 默认 | `params: PlanDeltaNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `item/reasoning/summaryPartAdded` | 服务端通知 | 默认 | `params: ReasoningSummaryPartAddedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `item/reasoning/summaryTextDelta` | 服务端通知 | 默认 | `params: ReasoningSummaryTextDeltaNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `item/reasoning/textDelta` | 服务端通知 | 默认 | `params: ReasoningTextDeltaNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `item/started` | 服务端通知 | 默认 | 校验当前 thread/turn；要求对象 `item` 及字符串 `type/id`；`userMessage` 严格读取 nullable `clientId` 与文本 `content`（含可选 `text_elements` 数组），`agentMessage` 严格读取 `text`，`commandExecution` 严格读取命令、actions、cwd、已知状态及 nullable 输出/exit code | `process_turn_message`、`validate_user_message` | `userMessage` → 确认已发送消息、不产生重复事件；`agentMessage` → `AssistantMessageStarted`；`commandExecution` → `CommandStarted` | `userMessage` 沿用发送时已建立的用户气泡；其余建立助手消息或命令活动行 | 部分接入 | 仅接入文本型 `userMessage`、`agentMessage`、`commandExecution`；非文本 user input、其他 item.type、缺字段或错误类型仍立即 fail-fast；userMessage 生命周期及边界测试覆盖 |
| `mcpServer/event/stream/notification` | 服务端通知 | 默认 | `params: McpServerEventStreamNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `mcpServer/oauthLogin/completed` | 服务端通知 | 默认 | `params: McpServerOauthLoginCompletedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `mcpServer/startupStatus/updated` | 服务端通知 | 默认 | 严格读取 `name` 与 `starting/ready/failed/cancelled` 状态；`threadId/error/failureReason` 可缺省或为 null，仅接受 `reauthenticationRequired` 失败原因 | `parse_mcp_server_startup_status_updated`、`parse_agent_notification`、`ensure_server_method_is_defined` | `AgentEvent::McpServerStartupStatusUpdated` | 按 thread/server 保存最新 GPUI 状态；启动失败显示非终止警告，认证失效时提示重新连接；其他状态不打断 turn | 已接入 | app-scoped 状态允许 `threadId=null`；无 Composer 事件流的短连接（如模型目录）只校验不发 UI 事件；未知状态、失败原因及错误字段 fail-fast；`mcp_server_startup_status_is_validated_and_normalized`、`drives_one_complete_prompt_and_normalizes_stream_events`、`mcp_server_startup_status_updates_gpui_state_without_ending_the_turn` |
| `model/rerouted` | 服务端通知 | 默认 | `fromModel`、`toModel`、`reason` 与 thread/turn | `parse_agent_notification` | `AgentEvent::ModelRerouted` | 更新实际模型和状态提示 | 已接入 | `model_notifications_are_normalized_into_agent_events` |
| `model/safetyBuffering/updated` | 服务端通知 | 默认 | model、useCases、reasons、showBufferingUi、nullable fasterModel | `parse_agent_notification` | `AgentEvent::ModelSafetyBufferingUpdated` | 显示/清除安全检查状态并提示可选更快模型 | 已接入 | 字段类型严格校验；模型通知回归覆盖 |
| `model/verification` | 服务端通知 | 默认 | `verifications[]` 与 thread/turn | `parse_agent_notification` | `AgentEvent::ModelVerificationRequired` | 显示需要账户验证并把 Composer 置为失败终态 | 已接入 | 模型通知解析和 UI 状态均有回归 |
| `process/exited` | 服务端通知 | 默认 | `params: ProcessExitedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `process/outputDelta` | 服务端通知 | 默认 | `params: ProcessOutputDeltaNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `project/changed` | 服务端通知 | 默认 | `params: ProjectChangedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `rawResponse/completed` | 服务端通知 | 默认 | 无 params；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `rawResponseItem/completed` | 服务端通知 | 默认 | 无 params；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `remoteControl/status/changed` | 服务端通知 | 默认 | 严格校验 `status`、`serverName`、`installationId` 与 nullable `environmentId` | `validate_remote_control_status_changed` | 连接生命周期兼容信号 | 无；不会阻断初始化后的 `model/list` 或 turn 握手 | 后端已接入 | 仅兼容 app-server 初始化时主动发送的连接状态，不建立 Composer UI；未知状态或错误字段仍 fail-fast；`model_catalog_accumulates_pages_and_maps_defaults_and_options`、`remote_control_status_changed_is_a_validated_connection_notification` |
| `serverRequest/resolved` | 服务端通知 | 默认 | `threadId` 与保持原类型的 `requestId`；由 registry 还原并核对 request 的 thread/turn/item/kind | `handle_server_request_resolved`、统一 pending/completed registry | `AgentEvent::ServerRequestResolved` | 最终结束并释放 command approval、user input、permissions approval 三类 responder 与等待状态 | 已接入 | 合法 response→resolved、三种请求、重复 resolved 幂等、错误 thread/turn/item、未知 request 和 turn 终止清理均覆盖 |
| `skills/changed` | 服务端通知 | 默认 | `params: SkillsChangedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/archived` | 服务端通知 | 默认 | `params: ThreadArchivedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/closed` | 服务端通知 | 默认 | `params: ThreadClosedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/compacted` | 服务端通知 | 默认 | `params: ContextCompactedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/deleted` | 服务端通知 | 默认 | `params: ThreadDeletedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/environment/connected` | 服务端通知 | 默认 | `params: EnvironmentConnectionNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/environment/disconnected` | 服务端通知 | 默认 | `params: EnvironmentConnectionNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/goal/cleared` | 服务端通知 | 默认 | payload 当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到时明确报错；等待对应 AgentEvent 与 GPUI 状态后才可接入 |
| `thread/goal/updated` | 服务端通知 | 默认 | payload 当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到时明确报错；等待对应 AgentEvent 与 GPUI 状态后才可接入 |
| `thread/name/updated` | 服务端通知 | 默认 | `params: ThreadNameUpdatedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/project/updated` | 服务端通知 | 默认 | `params: ThreadProjectUpdatedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/queue/changed` | 服务端通知 | 默认 | `params: ThreadQueueChangedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/realtime/closed` | 服务端通知 | 默认 | `params: ThreadRealtimeClosedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/realtime/error` | 服务端通知 | 默认 | `params: ThreadRealtimeErrorNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/realtime/item/completed` | 服务端通知 | 默认 | `params: ThreadRealtimeItemCompletedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/realtime/item/started` | 服务端通知 | 默认 | `params: ThreadRealtimeItemStartedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/realtime/item/transcript/delta` | 服务端通知 | 默认 | `params: ThreadRealtimeItemTranscriptDeltaNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/realtime/itemAdded` | 服务端通知 | 默认 | `params: ThreadRealtimeItemAddedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/realtime/outputAudio/delta` | 服务端通知 | 默认 | `params: ThreadRealtimeOutputAudioDeltaNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/realtime/sdp` | 服务端通知 | 默认 | `params: ThreadRealtimeSdpNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/realtime/started` | 服务端通知 | 默认 | `params: ThreadRealtimeStartedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/realtime/transcript/delta` | 服务端通知 | 默认 | `params: ThreadRealtimeTranscriptDeltaNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/realtime/transcript/done` | 服务端通知 | 默认 | `params: ThreadRealtimeTranscriptDoneNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/reverted` | 服务端通知 | 默认 | `params: ThreadRevertedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `thread/settings/updated` | 服务端通知 | 默认 | thread id；model、effort、serviceTier、cwd；可选有效权限字段 | `parse_agent_notification` | `AgentEvent::ThreadSettingsUpdated` | 同步模型、目录和有效权限状态 | 已接入 | 权限切换必须等到含有效 permissions 的匹配通知 |
| `thread/started` | 服务端通知 | 默认 | 严格读取 `params.thread.id`；允许 Thread 其余字段前向扩展；thread id 仍以请求响应为 canonical 来源 | `thread_started_id`、`ThreadStartedCorrelation`、`ensure_server_method_is_defined`、`ensure_session_message_matches` | 与 `thread/start`／`thread/resume` 响应关联；新 thread 继续映射既有 `AgentEvent::ThreadCreated` | 无新增视觉状态；Composer 的 `thread_id` 仍由请求响应只更新一次 | 已接入 | 通知先于或晚于响应均可；字段错误、通知间冲突、通知与 canonical id 不一致均 fail-fast；`drives_one_complete_prompt_and_normalizes_stream_events`、`existing_thread_resumes_before_turn_start`、`thread_started_must_match_the_canonical_thread_id_in_either_order`、`thread_started_is_a_validated_lifecycle_notification` |
| `thread/status/changed` | 服务端通知 | 默认 | 严格读取 `threadId` 与 `notLoaded/idle/systemError/active`；`active` 必须携带数组 `activeFlags`，仅接受 `waitingOnApproval/waitingOnUserInput` | `parse_thread_status_changed`、`parse_agent_notification`、`ensure_server_method_is_defined` | `AgentEvent::ThreadStatusChanged` | 按 thread id 保存最新 GPUI 状态及 active flags | 已接入 | 线程状态不替代 `turn/started`／`turn/completed` 的 turn 生命周期终态；未知状态、flag、缺字段或错误类型 fail-fast；`thread_status_changed_is_validated_and_normalized`、`drives_one_complete_prompt_and_normalizes_stream_events`、`thread_status_changed_updates_gpui_state_without_ending_the_turn` |
| `thread/tokenUsage/updated` | 服务端通知 | 默认 | 严格读取 `threadId`、`turnId`、`tokenUsage.total/last` 的总量、输入、缓存输入、缓存写入、输出与推理输出 token；`cacheWriteInputTokens` 缺省为 0；`modelContextWindow` 可缺省或为 null | `parse_thread_token_usage_updated`、`parse_agent_notification`、`ensure_server_method_is_defined` | `AgentEvent::ThreadTokenUsageUpdated` | 按 thread id 保存最新 GPUI 用量与上下文窗口状态；不生成对话活动、不结束 turn | 已接入 | 当前 turn 归属校验；缺少必填字段、错误类型或超出 int64 立即 fail-fast；`thread_token_usage_updated_is_validated_and_normalized`、`thread_token_usage_update_must_match_the_active_turn`、`drives_one_complete_prompt_and_normalizes_stream_events`、`thread_token_usage_updates_gpui_state_without_ending_the_turn` |
| `thread/unarchived` | 服务端通知 | 默认 | `params: ThreadUnarchivedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `turn/completed` | 服务端通知 | 默认 | `params.turn.status` 为 `completed`、`interrupted` 或 `failed`；失败读取 error message/details | `process_turn_message` | `Completed`、`Interrupted` 或 `Failed` | 设置完成、停止或失败终态并结束消费 | 已接入 | 未知终态 fail-fast；`failed_turn_completion_is_the_terminal_event_and_keeps_error_details` |
| `turn/diff/updated` | 服务端通知 | 默认 | `params: TurnDiffUpdatedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `turn/moderationMetadata` | 服务端通知 | 默认 | `params: TurnModerationMetadataNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `turn/plan/updated` | 服务端通知 | 默认 | payload 当前不读取；没有计划 UI 映射 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到时明确报错；等待对应 AgentEvent 与 GPUI 状态后才可接入 |
| `turn/started` | 服务端通知 | 默认 | 要求 `params.turn.status=inProgress`，并校验 thread/turn | `parse_agent_notification` | `AgentEvent::Started` | Composer 进入流式状态 | 已接入 | 缺字段、未知状态或错配 id 立即失败 |
| `warning` | 服务端通知 | 默认 | `message`；可选 `threadId` | `parse_agent_notification` | `AgentEvent::Warning` | 非终止警告 Notice | 已接入 | 无可见事件通道的模型目录连接收到该通知时失败，不静默丢弃 |
| `windows/worldWritableWarning` | 服务端通知 | 默认 | `params: WindowsWorldWritableWarningNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
| `windowsSandbox/setupCompleted` | 服务端通知 | 默认 | `params: WindowsSandboxSetupCompletedNotification`；当前不读取 | `ensure_server_method_is_defined` | 无 | 无 | 未接入 | 收到即 fail-fast |
