# Codex CLI App Server JSON-RPC 协议清单

> 基准版本：`codex-cli 0.150.1`
> 生成日期：2026-08-30
> 接入状态更新：2026-08-30
> 范围：`codex app-server generate-ts --experimental` 输出的全部方法联合类型，并与默认 TypeScript schema 对比标注能力门槛；同时用 JSON Schema 交叉校验字段定义。

## 结论

Codex App Server 使用双向 JSON-RPC 2.0 语义，但线上消息省略标准的 `"jsonrpc": "2.0"` 字段。以当前版本 `generate-ts` 输出的完整方法联合类型计，共包含 **249** 个方法：

| 消息族 | 数量 | 含义 |
|---|---:|---|
| ClientRequest | 156 | 客户端调用 App Server，服务端按同一 `id` 返回 `result` 或 `error` |
| ServerRequest | 11 | App Server 反向调用客户端，客户端必须按同一 `id` 响应 |
| ClientNotification | 1 | 客户端发送的无响应通知 |
| ServerNotification | 81 | App Server 推送的无响应事件 |
| **合计** | **249** | 默认 TypeScript schema 190 个；仅实验 schema 额外 59 个 |

“默认”表示该方法存在于不带 `--experimental` 的 TypeScript 生成结果；“实验性”表示只存在于带 `--experimental` 的结果，连接初始化时通常还需声明 `capabilities.experimentalApi: true`。这不是产品成熟度评级；部分默认方法仍可能处于开发中或已经弃用。

`codex-cli 0.150.1` 的两种生成器存在一处已验证的差异：`generate-json-schema` 的顶层方法联合仍为 244 个（默认 185 个），没有列出本次新增的 5 个方法；`generate-ts` 则已完整列出 249 个（默认 190 个）。这 5 个方法都存在于非实验 TypeScript 输出，因此下表按“默认”标注；其中仅由 TypeScript 方法联合暴露的情况也在状态栏中单独注明。

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
| `initialize`、`initialized` | 客户端 → 服务端 | `initialize_connection` 连接生命周期（无 `AgentEvent`） | 为模型目录连接和 prompt 连接建立初始化握手；按本机 0.150.1 schema 显式声明 `capabilities.experimentalApi: false`、`requestAttestation: false` |
| `model/list` | 客户端 → 服务端 | `CodexAppServerBackend::load_model_catalog` → `AgentModelCatalog` | 使用 `cursor`/`nextCursor` 拉取全部可见页；映射 model、`displayName`、默认模型、effort 与 service tier |
| `thread/start` | 客户端 → 服务端 | `AgentRequest` → `drive_session` → `AgentEvent::ThreadCreated` | 首回合创建可复用 thread，传入所选 `model`、`serviceTier`；不再发送固定权限字段 |
| `turn/start` | 客户端 → 服务端 | `AgentRequest` → `drive_session`（无 `AgentEvent`） | 提交文本 prompt；新 thread 首回合同时携带 Composer 模式对应的完整权限字段，后续回合复用已生效的 thread 设置 |
| `turn/interrupt` | 客户端 → 服务端 | `AgentInterruptHandle::interrupt` → `CodexTurnSession` | 在原 stdio 连接上使用已保存的 `threadId`、`turnId` 发送一次中断；开始阶段的停止请求会排队，重复请求及已结束 turn 不会重复写入 |
| `turn/started` | 服务端 → 客户端 | `AgentEvent::Started` | 将 `Starting` 推进到可见的 `Thinking` 运行态；保留停止按钮与活动状态，不结束事件流 |
| `error` | 服务端 → 客户端 | `AgentEvent::Error { message, details, will_retry }` | `willRetry: true` 显示低强调重试活动行，`false` 显示错误 Notice；两者都保持非终止，等待 `turn/completed` 决定最终状态 |
| `thread/settings/updated` | 服务端 → 客户端 | `AgentEvent::ThreadSettingsUpdated` | 静默同步模型、effort、service tier 与 effective 权限；当前状态由 Composer 控件直接呈现，不额外显示状态行 |
| `warning` | 服务端 → 客户端 | `AgentEvent::Warning` | 显示带警告图标和可访问 alert 语义的 Notice；保持非终止 |
| `configWarning` | 服务端 → 客户端 | `AgentEvent::ConfigWarning` | 显示 summary、details、文件与行列位置；存在 path 时提供“打开文件”按钮；保持非终止 |
| `item/started` | 服务端 → 客户端 | `AgentEvent::AssistantMessageStarted { item_id }` / `AgentEvent::CommandStarted(CommandExecution)` | 建立 assistant message 或 command activity |
| `item/agentMessage/delta` | 服务端 → 客户端 | `AgentEvent::TextDelta(String)` | 追加流式 assistant 文本 |
| `item/commandExecution/outputDelta` | 服务端 → 客户端 | `AgentEvent::CommandOutputDelta { item_id, delta }` | 按 `item_id` 将流式输出追加到对应 command activity |
| `item/completed` | 服务端 → 客户端 | `AgentEvent::CommandCompleted(CommandExecution)` / `AgentEvent::TextDelta(String)` | 完成 command activity；未收到文本 delta 时用完整 agent message 兜底 |
| `model/rerouted` | 服务端 → 客户端 | `AgentEvent::ModelRerouted` | 更新本轮实际模型，并在选择器触发器中显示 reroute 状态 |
| `model/verification` | 服务端 → 客户端 | `AgentEvent::ModelVerificationRequired` | 将额外账户验证要求转换为可见的失败状态 |
| `model/safetyBuffering/updated` | 服务端 → 客户端 | `AgentEvent::ModelSafetyBufferingUpdated` | 更新实际模型和暂态安全检查提示，结束 buffering 时清除提示 |
| `turn/completed` | 服务端 → 客户端 | `AgentEvent::Completed` / `AgentEvent::Interrupted` / `AgentEvent::Failed(String)` | 校验匹配的 `threadId`、`turnId`，按 `completed`、`interrupted`、`failed` 终态结束本轮；失败会合并 `error.message` 与 `additionalDetails`，终态后回收 stdin 与 app-server 子进程 |

Prompt 会话的 stdin 由可并发写入的 `CodexTurnSession` 持续持有，当前 `threadId` 与 `turnId` 会保留到终态。点击停止后 Composer 只进入 `Stopping`，不会截断事件流或伪造本地完成；只有收到匹配 turn 的 `turn/completed` 且状态为 `interrupted` 后才转为 `Stopped`。若任务先自然完成，则保留 `Complete`；若中断写入或连接失败，则进入 `Failed`。控制句柄丢弃、协议异常和正常终态都走幂等的关闭、kill、wait 路径，避免重复停止与退出竞态留下子进程。

选择器不再维护模型硬编码目录。目录加载完成后优先选择 `isDefault: true` 的模型（缺失时退回首项），使用该模型的 `defaultReasoningEffort` 和 `defaultServiceTier`；切换模型时重新应用目标模型的默认项。高级菜单、键盘导航及简化 effort 滑杆都按当前目录长度动态生成。`ThreadStartParams` 的本机 schema 没有 `effort` 字段，因此 effort 按 schema 仅发送给 `turn/start`，没有通过未定义字段塞入 `thread/start`。

Composer 的视觉层以本机 ChatGPT App（CDP `127.0.0.1:9222`）的实际计算样式为基准：展开触发器和主菜单宽 224 px，模型子菜单宽 280 px，推理强度子菜单宽 180 px，速度子菜单宽 233 px；行高、内边距、圆角、悬停/选中态、勾选图标、子菜单底部对齐和“重置为默认设置”行为均按实测值实现。模型子菜单只显示 `model/list` 返回的 `displayName`；推理强度和速度选项仍由目录动态决定，其中 ChatGPT UI 专属的 Ultra 副文案固定本地化为“更快消耗使用额度”，不使用协议中的英文 effort 描述替代该界面文案。

本批事件也通过同一 CDP 入口在真实 ChatGPT App 中临时注入并还原状态后取样：运行态使用 14 px / 21 px、60% 前景的计时状态；`error(willRetry: true)` 使用透明背景、6 px 间距的低强调活动行；不可重试错误和配置警告使用 20 px 圆角、`8px 8px 8px 12px` 内边距、0.5 px 强边框 ring 的 Notice。`configWarning` 的图标为 18 px，正文为 13 px / 20 px，可选“打开文件”按钮为 24 px 高、8 px 水平内边距，并保留 hover 反馈。实现同时为错误和警告添加 `Alert` 角色、为设置变更添加 `Status` 角色，避免只靠颜色传达状态。

### Composer 权限模式：真实 wire 取证、UI 恢复与协议边界

2026-08-31 Composer 四种权限模式已接入 JSON-RPC。首回合创建可复用 thread 并在 `turn/start` 发送权限字段；已有 thread 切换通过 `thread/settings/update`，且只以 `thread/settings/updated` 保存服务端 effective 权限。RPC 失败保留原 effective 设置并显示错误。审批交互类 `item/*/requestApproval` 仍不在本次范围内。

2026-08-30 已在真实 ChatGPT App 上对已有 thread `01a05195-1b99-7012-ac56-d651e2fada02` 分别选择四种模式。下表 request 列是 `thread/settings/update.params` 除 `threadId` 外的全部字段；四次 response 均为精确的 `result: {}`，不包含有效设置；最后一列只摘录随后 `thread/settings/updated.params.threadSettings` 中的权限相关服务端有效值。`V` 是该 thread 的 `/Users/zp/.codex/visualizations/2026/08/30/01a05195-1b99-7012-ac56-d651e2fada02`。

| Composer 模式 | `thread/settings/update.params` | RPC response | `thread/settings/updated` 有效权限 |
|---|---|---|---|
| Request / 请求批准 | `approvalPolicy:"on-request"`, `approvalsReviewer:"user"`, `permissions:":workspace"` | `result:{}` | `on-request` / `user` / `sandboxPolicy:{type:"workspaceWrite",writableRoots:[V],networkAccess:false,excludeTmpdirEnvVar:false,excludeSlashTmp:false}` / `activePermissionProfile:{id:":workspace",extends:null}` |
| Assist / 帮我批准 | `approvalPolicy:"on-request"`, `approvalsReviewer:"guardian_subagent"`, `permissions:":workspace"` | `result:{}` | `on-request` / **`auto_review`** / 同上 `workspaceWrite` / `activePermissionProfile:{id:":workspace",extends:null}` |
| Full / 完全访问权限 | `approvalPolicy:"never"`, `approvalsReviewer:"user"`, `permissions:":danger-full-access"` | `result:{}` | `never` / `user` / `sandboxPolicy:{type:"dangerFullAccess"}` / `activePermissionProfile:{id:":danger-full-access",extends:null}` |
| Custom / 自定义 | `approvalPolicy:"on-request"`, `approvalsReviewer:"user"`, `sandboxPolicy:{type:"dangerFullAccess"}`（无 `permissions`） | `result:{}` | `on-request` / `user` / `sandboxPolicy:{type:"dangerFullAccess"}` / `activePermissionProfile:null` |

Assist 的三个值必须分层保存：菜单选择 request 发送 `guardian_subagent`，服务端 effective 回写为 `auto_review`，另行创建的 Assist 新对话首回合 `turn/start` 也携带 `auto_review`。不能把 bundle catalog 的 request 值写死成 server-effective 值，也不能把回写的 `auto_review` 反向改写成 `guardian_subagent`。Custom 与 Full 也不等价：本机 Custom 虽然同样解析为 danger-full-access sandbox，但仍是 `on-request + user + 显式 sandboxPolicy`，且 effective profile 为 null；Full 是 `never + user + ":danger-full-access" profile`。

另外对四种模式分别创建新对话并发送不调用工具的无害 prompt，四次均收到 `turn/completed`。首回合真实 `turn/start` 权限字段如下；这些是独立新对话的请求，不是上表同一已有 thread 紧跟的 turn：

| 模式 | 新对话首回合 `turn/start` 的权限字段 |
|---|---|
| Request | `approvalPolicy:"on-request"`, `approvalsReviewer:"user"`, `sandboxPolicy:{type:"workspaceWrite",writableRoots:[GPUI,V],networkAccess:false,excludeTmpdirEnvVar:false,excludeSlashTmp:false}`, `permissions:null`, `runtimeWorkspaceRoots:null` |
| Assist | `approvalPolicy:"on-request"`, `approvalsReviewer:"auto_review"`, 同上 `workspaceWrite`, `permissions:null`, `runtimeWorkspaceRoots:null` |
| Full | `approvalPolicy:"never"`, `approvalsReviewer:"user"`, `sandboxPolicy:null`, `permissions:":danger-full-access"`, `runtimeWorkspaceRoots:[GPUI,V]` |
| Custom | `approvalPolicy:"on-request"`, `approvalsReviewer:"user"`, `sandboxPolicy:{type:"dangerFullAccess"}`, `permissions:null`, `runtimeWorkspaceRoots:[GPUI,V]` |

该表中 `GPUI` 是 `/Users/zp/Desktop/GPUI`，`V` 是各自 thread 的 visualization 目录。这一批新对话的 Request/Assist `thread/settings/updated` 都回写了 workspaceWrite 且 `activePermissionProfile:null`；Full 回写 `:danger-full-access`；Custom 仍为 null。因此实现必须以 response/notification 保存有效 policy 与 profile provenance，不能只依赖 Composer 四值 enum。

尚未观测的边界保持阻塞，不从 bundle 推断伪造成实测：

- 四次新对话的 `metadata.sawThreadStart` 均为 `false`；ChatGPT App 的 native/prewarm 路径没有让 renderer observer 看到直接 `thread/start` request。`thread/start` 中命名 profile 使用 `permissions`、Custom 使用旧式 `sandbox` 仍只是 schema + 当前 bundle 静态结论。
- 真实 `permissionProfile/list` 请求为 `{cursor:null,limit:100,cwd:"/Users/zp/Desktop/GPUI"}`，只返回 `:read-only`、`:workspace`、`:danger-full-access`，均 `allowed:true`，`nextCursor:null`；本机 `profiles:{}`。只观测到内建 profile 的 `extends:null`，非 null `extends` 需要修改用户 config，本次未获授权，因此保持 blocked。
- renderer reload 沿用既有 native transport，没有观察到 native host 的 `initialize.capabilities.experimentalApi`。为接入 `thread/settings/update`，当前 GPUI 已显式声明 `experimentalApi:true`；命名 profile 的可用性可通过 `permissionProfile/list` 查询。

完整 schema/bundle/CDP 取证结论、trace SHA-256 和阻塞边界统一维护在本文件。四种已有 thread 证据是 `artifacts/chatgpt-permission-protocol-cdp-2026-08-30/select-existing-{request,assist,full,custom}-trace.json`；四种首回合证据是同目录的 `new-thread-{request,assist,full,custom}-trace.json`；profile/list 在 `reload-capture.json`。下表权限菜单的旧严格矩阵仍保留作诊断，但 8/8 未达原阈值不再隐藏 UI。

### P0 UI-first 对齐矩阵（2026-08-30）

本批先通过 CDP `http://127.0.0.1:9222` 采集真实 ChatGPT App，再实现纯 GPUI 的命令/网络审批、文件审批、文件变更活动、Diff Review、用户问答和权限模式候选表面；没有使用 WebView、HTML、PNG/JPEG 或截图充当产品 UI。证据清单在 `scripts/p0_ui_matrix_manifest.json`，校验器在 `scripts/verify_p0_ui_matrix.py`。所有审计结论统一维护在本文件；原始证据分别保存在 `artifacts/chatgpt-p0-ui-cdp-audit-2026-08-30/`、`artifacts/chatgpt-multifile-diff-cdp-audit-2026-08-30/`、`artifacts/chatgpt-user-input-multi-cdp-audit-2026-08-30/`、`artifacts/chatgpt-permissions-request-cdp-audit-2026-08-30/` 与 `artifacts/chatgpt-permission-protocol-cdp-2026-08-30/`。自然触发、受控协议重放和仅据重复结构推导的实现没有混称。

多文件与长 Diff 的增量证据全部来自自然 Composer 工作流，而不是受控协议重放：请求 `2106` 的两文件 `item/fileChange/requestApproval` 实测为 `736×210`，内部列表 `704×66`，两行各 `33px`；请求 `2117` 的八文件审批实测为 `736×344`，列表 viewport `704×200`、内容 `264px`、最大滚动 `64px`。前者真实点击“允许一次”后观测到同 request id 的 `serverRequest/resolved`、`item/completed` 与 `turn/diff/updated`，后者真实 Escape 后观测到关闭与 resolved；被动 renderer observer 仍未捕获 Electron bridge 的精确 renderer→host decision response object，因此不能从点击结果反推或声称该 wire payload 已验证。

同一自然工作流的精确 `turn/diff/updated` 为 3,780 bytes、123 个 `split('\n')` entries、两文件、`+16/-16` 与 80 行上下文，SHA-256 为 `9808146c5ab7541ec60f414633b66ab6204610fd3c96211e1d44efbd116801b5`。可跟踪的测试副本位于 `tests/fixtures/cdp/turn-diff-updated-two-file-123-entries.diff`，只供 capture/test harness；生产 `DiffReviewPresentation::from_unified_diff` 接受领域层真实 diff，不读取该 fixture。实测 Review viewport 为 `1107.641×1324`，两个文件区各 `1260.25px`，每个 56 个实际行、行高 `21.59375px`；跨文件滚动 `scrollTop=1100`，首文件折叠后为 `34px`。

候选 GPUI 已按这些证据改为完整重复结构：审批遍历任意文件并保留原生 `ScrollHandle`（八文件严格 200px cap）；完成态显示 `64.5px` header、1px 分隔线与每文件 36px 行；Review 遍历所有文件与所有上下文/删除/新增行，使用单一真实行号 gutter、文件折叠及整面板滚动。Review 与完成态文件行传递其自身 `DiffReviewPresentation`，Copy 使用真实展示路径，Open 使用解析后的真实文件路径；没有可验证的领域回滚时 Undo 保持移除。最新离线重拍还修复了 GPUI flex 将每个 `21.59375px` Diff 行独立量化为 22px 的累计漂移：行层现在按 `2 + index × 21.59375px` 原生定位，同时保持单文件 `1260.25px` 总高。两/八文件、完成态和长 Review 的 16 个完整 light/dark 状态均已逐项生成 GPUI crop、diff 和分数；这些分数现在是对齐诊断，不再单独决定 UI 是否可见。

严格比较器仍只比较完整控件或完整状态区域的未稀释裁剪。复核时撤销了用大面积不变背景抬分的结果；5 个命令关闭态和多个审批关闭态的旧宽裁剪已从 canonical evidence 删除，原始/debug 证据仍保留。旧 dark user-input default 也被证实与 hover crop 字节相同并降为 blocked。正式矩阵现有 15 个 required scenario、128 个独立主题/表面/状态条目：14 个 ready 单项通过，114 个保持诊断 blocked，ready 最低 `99.500764012311%`；校验器仍按其严格规则返回非零，但该结果不再作为 UI 可见性的发布开关。

| 表面 | 严格逐状态结果 | 诊断与协议状态 |
|---|---|---|
| 命令/网络审批 pending | dark default/approve-hover/decline-hover/options/options-focus：`99.377621 / 99.324033 / 99.380064 / 98.971475 / 98.958139%`；light：`99.276492 / 99.218457 / 99.279315 / 98.738763 / 98.720820%` | blocked；`item/commandExecution/requestApproval` 仍返回 `-32601` |
| 命令审批关闭态 | 真实 renderer 在 approved/declined/resolved 后卸载卡片；旧 `2200×194` 比较主要是背景和 Composer，不是状态局部 UI，已全部撤销；light approved 与 timeout 也没有可用真实参考 | 全部 blocked；不能用卸载后的大背景声明关闭态通过 |
| 文件修改审批 | 旧单文件 light default/approve-hover/decline-hover/options/options-focus：`99.409058 / 99.352807 / 99.410098 / 98.935915 / 98.920439%`；dark：`99.499727 / 99.447893 / 99.500764 / 99.108220 / 99.099238%`，其中 dark decline-hover 单项 `99.500764%` 独立 ready。自然两文件 default 完整 `736×210`：dark/light `99.004312473782 / 98.817138087119%`；自然八文件完整 `736×344` top：`98.353087495952 / 98.031921396803%`，bottom：`98.349940129827 / 98.028268056464%`；timeout 未触发 | surface gate 仍 blocked；top/bottom 是各自独立 GPUI 滚动状态，不能用 header/action 或旧单文件局部替代，`item/fileChange/requestApproval` 未接入 |
| `fileChange` activity | 旧单文件 completed light/dark strict `736×65`：`97.735994 / 98.017034%`；与 request 2106 对应的自然两文件 completed 完整 `736×138` dark/light：`98.069895412481 / 98.067142499722%`；started/failed 没有独立真实参考 | blocked；两个真实文件行均已渲染，Undo 因缺少真实反转领域操作继续移除，没有扩展生产 `item/started` / `item/completed` |
| Turn Diff | 旧 strict changed-file `989×73`：light `97.885788%`，dark `98.135025%`；自然双文件 123-entry 完整 `1108×1324` Review dark/light：top `98.608918103799 / 98.448636188436%`、collapsed `98.657277162977 / 98.496814271715%`、cross-file-scroll `98.451404736532 / 98.380059243187%`、bottom `98.479757270954 / 98.413659187613%` | blocked；累计分数行定位、`2533/1209px` bottom、完整 parser/112 行/行号/折叠/滚动均已验证，但每项仍低于门禁，`turn/diff/updated` 未接入 |
| 用户问答 | 单题 ready 为 dark hover/focus、light default/hover/focus、light skip-hover 共 6 项。自然两题 id `2132` 与 controlled renderer replay 另形成 16 个独立 required 条目；current-code 完整 `736×214` 卡片中，light/dark Q1 default `99.503277269190 / 99.525049929223%`、dark Q2 navigation `99.507917466729%`、light Previous 后答案持久化 `99.505676627281%` 独立 ready。light Q2 与 light skip 仅 `99.487569814598 / 99.492991816122%`；dark Previous/skip 无同主题 reference，submitted/dismissed/resolved 都是 unmount，timeout 没有自然参考 | surface gate 仍 blocked；4 个多题通过项不能替代其余 12 项，`item/tool/requestUserInput` 未接入 |
| 独立权限申请 | 默认 App 中受 feature gate 阻止；独立 `codex app-server --enable request_permissions_tool` + granular policy 自然发出 id `0`，同 id 拒绝后收到 resolved。真实 renderer R01–R23 来自 schema-valid 受控协议分发重放，不冒充自然触发。dark default/decline-hover/focus 分别 `99.570517 / 99.572046 / 99.518631%` ready；其余 pending/menu/filesystem/combined/closed 均逐项低于阈值，loading/error/timeout 没有可见实现 | surface gate blocked；纯 GPUI card 仅在 capture/test 开启，`item/permissions/requestApproval` 未接入 |
| Composer 权限模式 | light default/options/hover/focus：`94.269789 / 98.887532 / 98.764301 / 98.764301%`；dark default/options/hover：`96.217398 / 98.549133 / 98.506699%`；dark focus 无独立真实参考 | UI 已恢复为生产可见、可交互；分数仅作尽量对齐诊断，协议映射仍等待持久 session 与回写闭环 |

候选组件仍有完整性或证据缺口：长/多行命令预览仍只有已实采的单行几何；审批控件使用 surface-owned 逻辑焦点，Diff 尚无完整键盘操作。文件审批、完成态重复文件行和完整多行 Diff 的首项/首行限制已经移除，Review/Copy/Open 已改用真实领域 presentation/path；多题已实现 header 内 previous/`N of M`/next、真实 DOM 顺序的 Tab/Enter、答案保持、最终提交、逐题跳过、dismiss/resolved。Other 已改为 `EntityInputHandler` 原生输入并覆盖 IME、光标、选区、剪贴板、删除、方向键、Enter 与 secret mask。数值相似度现在用于指导继续对齐，不再隐藏已实现的 UI；缺少真实参考的 started/failed/timeout/error 状态仍不会凭空设计。

`serverRequest/resolved` 仍保持未定义，因为按 request id 管理的 pending registry、回复和卸载闭环尚未接入，而不是因为像素分数。协议回归测试仍锁定四类反向请求返回 `-32601`，`serverRequest/resolved` 与 `turn/diff/updated` 进入 undefined-method 错误，`fileChange` item 不提前生成领域事件；不会静默自动批准或伪造本地完成。

下方总表中，“是”表示消息已转换为内部协议并由应用消费；“已知（no-op）”表示适配器会显式接受该通知，但不生成 `AgentEvent`；“否”表示尚未定义或接入，实际收到时会进入未定义方法错误处理。

## 全部方法

### 客户端 → 服务端请求（ClientRequest，156 个）

| # | Method | 消息形式 | Params schema | 能力门槛 / 状态 | 内部方法 | 是否接入 |
|---:|---|---|---|---|---|:---:|
| 1 | `initialize` | 请求（有 `id`） | `InitializeParams` | 默认 | `initialize_connection`（`load_model_catalog` 与 `run_prompt` 共用） | 是 |
| 2 | `server/diagnostics` | 请求（有 `id`） | `ServerDiagnosticsParams` | 实验性 | — | 否 |
| 3 | `thread/start` | 请求（有 `id`） | `ThreadStartParams` | 默认 | `AgentRequest` → `drive_session`（首回合创建可复用 thread，仅发送 `cwd`、`model`、`serviceTier` 等 thread 字段） | 是 |
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
| 150 | `getConversationSummary` | 请求（有 `id`） | `GetConversationSummaryParams` | 默认；仅 TS 方法联合类型 | — | 否 |
| 151 | `gitDiffToRemote` | 请求（有 `id`） | `GitDiffToRemoteParams` | 默认；仅 TS 方法联合类型 | — | 否 |
| 152 | `getAuthStatus` | 请求（有 `id`） | `GetAuthStatusParams` | 默认；仅 TS 方法联合类型 | — | 否 |
| 153 | `fuzzyFileSearch` | 请求（有 `id`） | `FuzzyFileSearchParams` | 默认 | — | 否 |
| 154 | `fuzzyFileSearch/sessionStart` | 请求（有 `id`） | `FuzzyFileSearchSessionStartParams` | 实验性 | — | 否 |
| 155 | `fuzzyFileSearch/sessionUpdate` | 请求（有 `id`） | `FuzzyFileSearchSessionUpdateParams` | 实验性 | — | 否 |
| 156 | `fuzzyFileSearch/sessionStop` | 请求（有 `id`） | `FuzzyFileSearchSessionStopParams` | 实验性 | — | 否 |

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

### 服务端 → 客户端通知（ServerNotification，81 个）

| # | Method | 消息形式 | Params schema | 能力门槛 / 状态 | 内部方法 | 是否接入 |
|---:|---|---|---|---|---|:---:|
| 1 | `error` | 通知（无 `id`） | `ErrorNotification` | 默认 | `parse_agent_notification` → `AgentEvent::Error` → 重试活动行 / 错误 Notice（非终止） | 是 |
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
| 18 | `thread/settings/updated` | 通知（无 `id`） | `ThreadSettingsUpdatedNotification` | 默认 | `parse_agent_notification` → `AgentEvent::ThreadSettingsUpdated` → 静默同步有效模型与权限设置（非终止） | 是 |
| 19 | `thread/tokenUsage/updated` | 通知（无 `id`） | `ThreadTokenUsageUpdatedNotification` | 默认 | `PASSIVE_SERVER_METHODS` → no-op（不生成 `AgentEvent`） | 已知（no-op） |
| 20 | `turn/started` | 通知（无 `id`） | `TurnStartedNotification` | 默认 | `parse_agent_notification` → `AgentEvent::Started` → `ConversationPhase::Thinking`（非终止） | 是 |
| 21 | `hook/started` | 通知（无 `id`） | `HookStartedNotification` | 默认 | — | 否 |
| 22 | `turn/completed` | 通知（无 `id`） | `TurnCompletedNotification` | 默认 | `drive_session` 校验 thread/turn → `AgentEvent::Completed` / `Interrupted` / `Failed(String)` → 终止状态 | 是 |
| 23 | `hook/completed` | 通知（无 `id`） | `HookCompletedNotification` | 默认 | — | 否 |
| 24 | `turn/diff/updated` | 通知（无 `id`） | `TurnDiffUpdatedNotification` | 默认 | — | 否 |
| 25 | `turn/plan/updated` | 通知（无 `id`） | `TurnPlanUpdatedNotification` | 默认 | `PASSIVE_SERVER_METHODS` → no-op（不生成 `AgentEvent`） | 已知（no-op） |
| 26 | `item/started` | 通知（无 `id`） | `ItemStartedNotification` | 默认 | `drive_session` → `AgentEvent::AssistantMessageStarted { item_id }` / `AgentEvent::CommandStarted(CommandExecution)` → `ComposerView::apply_agent_event_batch` | 是（`agentMessage`、`commandExecution`） |
| 27 | `item/autoApprovalReview/started` | 通知（无 `id`） | `ItemGuardianApprovalReviewStartedNotification` | 默认 | — | 否 |
| 28 | `item/autoApprovalReview/completed` | 通知（无 `id`） | `ItemGuardianApprovalReviewCompletedNotification` | 默认 | — | 否 |
| 29 | `autoApprovalReview/strictReviewRequired` | 通知（无 `id`） | `StrictReviewRequiredNotification` | 默认 | — | 否 |
| 30 | `item/completed` | 通知（无 `id`） | `ItemCompletedNotification` | 默认 | `drive_session` → `AgentEvent::CommandCompleted(CommandExecution)` / `AgentEvent::TextDelta(String)` → `ComposerView::apply_agent_event_batch` | 是（`agentMessage`、`commandExecution`） |
| 31 | `rawResponseItem/completed` | 通知（无 `id`） | `RawResponseItemCompletedNotification` | 默认；仅 TS 方法联合类型 | — | 否 |
| 32 | `rawResponse/completed` | 通知（无 `id`） | `RawResponseCompletedNotification` | 默认；仅 TS 方法联合类型；内部用途 | — | 否 |
| 33 | `item/agentMessage/delta` | 通知（无 `id`） | `AgentMessageDeltaNotification` | 默认 | `drive_session` → `AgentEvent::TextDelta(String)` → `ComposerView::apply_agent_event_batch` | 是 |
| 34 | `item/plan/delta` | 通知（无 `id`） | `PlanDeltaNotification` | 默认 | — | 否 |
| 35 | `command/exec/outputDelta` | 通知（无 `id`） | `CommandExecOutputDeltaNotification` | 默认 | — | 否 |
| 36 | `process/outputDelta` | 通知（无 `id`） | `ProcessOutputDeltaNotification` | 默认 | — | 否 |
| 37 | `process/exited` | 通知（无 `id`） | `ProcessExitedNotification` | 默认 | — | 否 |
| 38 | `item/commandExecution/outputDelta` | 通知（无 `id`） | `CommandExecutionOutputDeltaNotification` | 默认 | `drive_session` → `AgentEvent::CommandOutputDelta { item_id, delta }` → `ComposerView::apply_agent_event_batch` | 是 |
| 39 | `item/commandExecution/terminalInteraction` | 通知（无 `id`） | `TerminalInteractionNotification` | 默认 | — | 否 |
| 40 | `item/fileChange/outputDelta` | 通知（无 `id`） | `FileChangeOutputDeltaNotification` | 默认；已弃用 | — | 否 |
| 41 | `item/fileChange/patchUpdated` | 通知（无 `id`） | `FileChangePatchUpdatedNotification` | 默认 | — | 否 |
| 42 | `serverRequest/resolved` | 通知（无 `id`） | `ServerRequestResolvedNotification` | 默认 | — | 否 |
| 43 | `item/mcpToolCall/progress` | 通知（无 `id`） | `McpToolCallProgressNotification` | 默认 | — | 否 |
| 44 | `mcpServer/oauthLogin/completed` | 通知（无 `id`） | `McpServerOauthLoginCompletedNotification` | 默认 | — | 否 |
| 45 | `mcpServer/startupStatus/updated` | 通知（无 `id`） | `McpServerStatusUpdatedNotification` | 默认 | `PASSIVE_SERVER_METHODS` → no-op（不生成 `AgentEvent`） | 已知（no-op） |
| 46 | `mcpServer/event/stream/notification` | 通知（无 `id`） | `McpServerEventStreamNotification` | 默认 | — | 否 |
| 47 | `account/updated` | 通知（无 `id`） | `AccountUpdatedNotification` | 默认 | — | 否 |
| 48 | `account/rateLimits/updated` | 通知（无 `id`） | `AccountRateLimitsUpdatedNotification` | 默认 | `PASSIVE_SERVER_METHODS` → no-op（不生成 `AgentEvent`） | 已知（no-op） |
| 49 | `app/list/updated` | 通知（无 `id`） | `AppListUpdatedNotification` | 默认 | — | 否 |
| 50 | `remoteControl/status/changed` | 通知（无 `id`） | `RemoteControlStatusChangedNotification` | 默认 | `PASSIVE_SERVER_METHODS` → no-op（不生成 `AgentEvent`） | 已知（no-op） |
| 51 | `externalAgentConfig/import/progress` | 通知（无 `id`） | `ExternalAgentConfigImportProgressNotification` | 默认 | — | 否 |
| 52 | `externalAgentConfig/import/completed` | 通知（无 `id`） | `ExternalAgentConfigImportCompletedNotification` | 默认 | — | 否 |
| 53 | `fs/changed` | 通知（无 `id`） | `FsChangedNotification` | 默认 | — | 否 |
| 54 | `item/reasoning/summaryTextDelta` | 通知（无 `id`） | `ReasoningSummaryTextDeltaNotification` | 默认 | — | 否 |
| 55 | `item/reasoning/summaryPartAdded` | 通知（无 `id`） | `ReasoningSummaryPartAddedNotification` | 默认 | — | 否 |
| 56 | `item/reasoning/textDelta` | 通知（无 `id`） | `ReasoningTextDeltaNotification` | 默认 | — | 否 |
| 57 | `thread/compacted` | 通知（无 `id`） | `ContextCompactedNotification` | 默认；已弃用 | — | 否 |
| 58 | `model/rerouted` | 通知（无 `id`） | `ModelReroutedNotification` | 默认 | `drive_session` → `AgentEvent::ModelRerouted` → `ComposerView::apply_agent_event_batch` | 是 |
| 59 | `model/verification` | 通知（无 `id`） | `ModelVerificationNotification` | 默认 | `drive_session` → `AgentEvent::ModelVerificationRequired` → `ComposerView::apply_agent_event_batch` | 是 |
| 60 | `turn/moderationMetadata` | 通知（无 `id`） | `TurnModerationMetadataNotification` | 默认 | — | 否 |
| 61 | `model/safetyBuffering/updated` | 通知（无 `id`） | `ModelSafetyBufferingUpdatedNotification` | 默认 | `drive_session` → `AgentEvent::ModelSafetyBufferingUpdated` → `ComposerView::apply_agent_event_batch` | 是 |
| 62 | `warning` | 通知（无 `id`） | `WarningNotification` | 默认 | `parse_agent_notification` → `AgentEvent::Warning` → 警告 Notice（非终止） | 是 |
| 63 | `guardianWarning` | 通知（无 `id`） | `GuardianWarningNotification` | 默认 | — | 否 |
| 64 | `deprecationNotice` | 通知（无 `id`） | `DeprecationNoticeNotification` | 默认 | — | 否 |
| 65 | `configWarning` | 通知（无 `id`） | `ConfigWarningNotification` | 默认 | `parse_agent_notification` → `AgentEvent::ConfigWarning` → 配置警告 Notice + 可选打开文件（非终止） | 是 |
| 66 | `fuzzyFileSearch/sessionUpdated` | 通知（无 `id`） | `FuzzyFileSearchSessionUpdatedNotification` | 默认 | — | 否 |
| 67 | `fuzzyFileSearch/sessionCompleted` | 通知（无 `id`） | `FuzzyFileSearchSessionCompletedNotification` | 默认 | — | 否 |
| 68 | `thread/realtime/started` | 通知（无 `id`） | `ThreadRealtimeStartedNotification` | 默认 | — | 否 |
| 69 | `thread/realtime/itemAdded` | 通知（无 `id`） | `ThreadRealtimeItemAddedNotification` | 默认 | — | 否 |
| 70 | `thread/realtime/item/started` | 通知（无 `id`） | `ThreadRealtimeItemStartedNotification` | 默认 | — | 否 |
| 71 | `thread/realtime/item/transcript/delta` | 通知（无 `id`） | `ThreadRealtimeItemTranscriptDeltaNotification` | 默认 | — | 否 |
| 72 | `thread/realtime/item/completed` | 通知（无 `id`） | `ThreadRealtimeItemCompletedNotification` | 默认 | — | 否 |
| 73 | `thread/realtime/transcript/delta` | 通知（无 `id`） | `ThreadRealtimeTranscriptDeltaNotification` | 默认 | — | 否 |
| 74 | `thread/realtime/transcript/done` | 通知（无 `id`） | `ThreadRealtimeTranscriptDoneNotification` | 默认 | — | 否 |
| 75 | `thread/realtime/outputAudio/delta` | 通知（无 `id`） | `ThreadRealtimeOutputAudioDeltaNotification` | 默认 | — | 否 |
| 76 | `thread/realtime/sdp` | 通知（无 `id`） | `ThreadRealtimeSdpNotification` | 默认 | — | 否 |
| 77 | `thread/realtime/error` | 通知（无 `id`） | `ThreadRealtimeErrorNotification` | 默认 | — | 否 |
| 78 | `thread/realtime/closed` | 通知（无 `id`） | `ThreadRealtimeClosedNotification` | 默认 | — | 否 |
| 79 | `windows/worldWritableWarning` | 通知（无 `id`） | `WindowsWorldWritableWarningNotification` | 默认 | — | 否 |
| 80 | `windowsSandbox/setupCompleted` | 通知（无 `id`） | `WindowsSandboxSetupCompletedNotification` | 默认 | — | 否 |
| 81 | `account/login/completed` | 通知（无 `id`） | `AccountLoginCompletedNotification` | 默认 | — | 否 |

## 本次验证结果

- 本机版本：`codex-cli 0.150.1`。
- 重新执行 `codex app-server generate-ts --experimental` 和默认 TypeScript schema 生成；实验输出为 156 个 ClientRequest、11 个 ServerRequest、1 个 ClientNotification、81 个 ServerNotification（合计 249），默认输出合计 190。
- 同时重新执行 `codex app-server generate-json-schema --experimental` 和默认 JSON Schema 生成；实验输出仍为 153 个 ClientRequest、11 个 ServerRequest、1 个 ClientNotification、79 个 ServerNotification（合计 244），默认输出合计 185。
- 与上一版清单相比，TypeScript 方法联合新增 `getConversationSummary`、`gitDiffToRemote`、`getAuthStatus` 3 个 ClientRequest，以及 `rawResponseItem/completed`、`rawResponse/completed` 2 个 ServerNotification；这 5 个方法均未进入 JSON Schema 的顶层方法联合，`RawResponseCompletedNotification` 的生成注释明确标注为内部用途。
- 对真实 app-server 以 `limit: 2` 调用 `model/list`：通过 4 页及连续 `nextCursor` 拉取到 7 个可见模型；响应包含 `displayName`、`isDefault`、`supportedReasoningEfforts`、`defaultReasoningEffort`、`serviceTiers`、`defaultServiceTier`，与本机生成 schema 一致。
- `python3 scripts/verify_p0_ui_matrix.py --self-test`：通过；正式矩阵仍复算出 128 项，其中 14 个 ready 单项通过、114 个诊断 blocked。正式比较器按其严格规则返回 exit code 1，但用户已明确该阈值仅作尽量对齐的参考，不再据此隐藏 UI。
- `cargo fmt --check`：通过。
- `cargo test`：主 crate 194 个、隔离 permissions crate 14 个测试全部通过；新增覆盖纯 GPUI 审批/文件/Diff/问答模型、原生 Other 输入、多文件滚动、完整 Diff、多题状态、权限模式生产可见与鼠标/键盘交互、初始化 capabilities，以及尚未接入协议的反向请求不得静默成功。
- `cargo test --features screenshot`：同一组 194 + 14 测试全部通过，截图构建路径可编译并保持生产权限映射关闭。
- `cargo check --all-targets`：通过。
- `git diff --check`：通过。

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
- 本机 `codex-cli 0.150.1` 生成的 TypeScript 协议类型与 `codex_app_server_protocol.schemas.json`（默认及 `--experimental` 两套）
