# Codex app-server 接入

## 基线与口径

核对基线：`codex-cli 0.153.0`（2026-09-08）。方法与字段来自该 CLI 生成的 schema，接入状态来自仓库实现。schema 随 CLI 版本生成，见[官方协议说明](https://learn.chatgpt.com/docs/app-server#message-schema)；升级时重新导出并核对：

```bash
codex --version
codex app-server generate-json-schema --out artifacts/app-server-schema/default
codex app-server generate-json-schema --experimental --out artifacts/app-server-schema/experimental
```

共 **248** 个方法：155 个客户端请求、11 个服务端请求、1 个客户端通知、81 个服务端通知。表中“默认”表示方法出现在默认 schema，“实验”表示仅出现在 experimental schema；字段以 experimental schema 为准。运行时启用 `experimentalApi=true`。

| 状态 | 数量 | 判定 |
|---|---|---|
| 已接入 | 69 | 表中声明的产品行为已连通协议、领域数据和 UI／副作用；不表示消费全部可选字段 |
| 后端已接入 | 2 | 已实现读取或校验，尚无对应可见 UI 调用方或展示 |
| 部分接入 | 3 | 只支持部分类型、有效变体或限定生命周期窗口 |
| 未接入 | 174 | 客户端不发送；服务端请求按原 id 回复 `-32601` 并终止当前连接，服务端通知直接报错并终止连接 |

未接入行的“—”沿用上述规则。`tool/requestUserInput` 是兼容别名，不计入本版本 schema 的 248 项。

## 连接与状态

- **连接**：`ChatApp` 持有一个共享 manager。每个 generation 启动一个 `codex app-server --stdio`，只握手一次；单 reader 读取 stdout，stdin 串行写入完整 JSONL。所有 RPC 共用递增 request id，响应可乱序。
- **线程与轮次**：首次提示词执行 `thread/start → turn/start`；既有线程在当前 generation 未加载时先 resume，之后直接 start turn。同一线程最多一个活动 turn，不同线程可并行；start/resume/fork 共用串行生命周期注册表。
- **归属与提前事件**：轮次事件按 `threadId + turnId` 路由；server request 按原始字符串／数字 id 记录所属轮次。`turn/start` 响应前的事件按 wire 顺序缓存，取得响应后验证并回放；错配 id、字段或枚举报错。
- **审批与输入**：保留数字／字符串 request id 的区别，按到达顺序显示一张请求卡；键盘只响应当前可见请求。响应写入最多尝试一次，提交后等待 `serverRequest/resolved` 释放 responder；写入失败显示错误并阻止重复提交。文件审批关联同轮次、同 item 的原始 changes／patch，不使用聚合 turn diff 或当前磁盘内容代替。会话或轮次切换使旧点击失效；终态清理自身请求、响应句柄及临时关联。连接仅保留有上限的已释放 id／thread 标记，忽略已知重复或迟到的 resolved。
- **自动复核**：复核记录与人工审批请求分开，所有复核通知统一通过线程订阅与单调快照分发，避免轮次通道关闭时的竞争。未绑定 turn 的提前通知按完整标识等待真实 turn/start 响应，复核通知不会抢占启动中的轮次；已结束轮次的迟到通知经线程订阅更新原活动或历史快照。中断／失败／完成后本地结束等待展示，保留服务端原始状态与时间，不伪造完成通知；后续真实结果仍可补全。重复开始、重复完成和较旧完成消息不会回退已有结果。视图复用完整复核键，有目标项时随对应工具展示（MCP 拒绝独立展示），无目标项时独立展示；通过态隐藏但保留数据。当前 schema 未提供复核历史 item，应用重启后仅恢复服务端实际返回的历史，不从 rollout 或本地数据库补造复核。
- **终止与恢复**：turn 完成、中断或业务失败不关闭共享进程。EOF、崩溃、写失败或致命协议错误使旧 generation 的 pending RPC 和活动轮次各失败一次；回收旧进程后，下一次显式操作可重建连接，不自动重放提示词。应用退出时幂等终止并 wait 子进程。
- **状态通知**：应用／线程状态通过 `AgentConnectionEvent` 快照订阅，轮次事件进入各自 `AgentRun`。工作区通知可先于 RPC 响应；内存覆盖层防止迟到列表撤销重命名、移动、归档或删除。
- **工作区与历史**：以服务端稳定 id 管理项目和线程；置顶使用服务端 `Pinned` 分区，当前 schema 无 `isPinned`。历史先 `thread/read(includeTurns=false)`，再分页读取 `thread/turns/list(itemsView=full)`；实际非 full 的轮次由 `thread/items/list` 补全。不维护本地会话数据库。
- **临时侧边聊天**：`thread/fork → thread/inject_items` 完成后才允许发送。父历史仅供参考，侧边说明禁止延续父任务或调用子 agent；新消息明确要求的修改才属于侧边请求。关闭使用 `thread/unsubscribe`；临时 id 只在所属 generation 使用，失效后保留可读消息，禁止 resume。

代码入口：协议位于 [src/agent/codex/](../src/agent/codex/)，领域类型位于 [src/agent/](../src/agent/)，工作区合并位于 [src/workspace.rs](../src/workspace.rs)，会话归约位于 [src/conversation/](../src/conversation/)。方法表的“入口”相对于 `src/agent/codex/`，省略 `.rs`。

## Item 与历史兼容

实时 `item/started`／`item/completed` 支持下表除 `webSearch` 外的类型。未知实时类型报错；历史额外支持 `webSearch`，其他未知类型保留为 `ThreadHistoryItem::Unsupported`。因此两个实时 item 方法仍标为“部分接入”。

| 类型 | 数据与兼容处理 | 展示行为 |
|---|---|---|
| `userMessage` | 校验 text/image/localImage/audio/localAudio/skill/mention；文本统一换行、解码显示转义并移除附件包络，历史保留图片顺序 | 实时不重复添加用户消息；恢复文本与附件 |
| `agentMessage` | 历史保留 phase；最终答复优先取最后一条 final_answer，旧历史回退到最后一条未标注消息 | 仅已完成且可识别最终答复的轮次折叠过程前缀 |
| `reasoning` | 按 item.id 与 summaryIndex/contentIndex 保存稀疏增量，保留开始／完成时间；不跨 item/index 合并 | 展示 summary，缺省时展示 content；完成后显示耗时 |
| `commandExecution` | 保留 command、cwd、exitCode、commandActions；旧历史缺少 actions/cwd 时用空列表／线程目录 | 读取、搜索、列目录与 shell 分别显示，输出归属对应命令 |
| `fileChange` | 保留 path、kind、diff；实时接受 patchUpdated 和 turn 聚合 diff；历史按路径汇总 | 文件卡及固定历史差异使用原始 patch；本机 Git 面板另由 Git/gh 提供工作区数据 |
| `imageView` | id/path；同 id 原位更新 | 缩略图与全局原图预览 |
| `imageGeneration` | 当前 status 为 in_progress/completed/failed，result 必需；读取 nullable revisedPrompt/savedPath/transparentBackground/failure。唯一 typed failure 为 usageLimitExceeded{limitId,resetsAt}；旧命名仅在历史路径兼容 | 优先 savedPath，文件不可用时物化 base64；中断移除未完成 loader，不伪造 failed item |
| `contextCompaction` | 按 item.id 更新；历史恢复为已完成活动 | 独立压缩上下文活动 |
| `collabAgentToolCall`、`collabToolCall`、`subAgentActivity` | 当前、相邻版本与旧历史映射为 AgentCollaboration。按载荷中的工具／接收者状态更新；稳定 item.id 原位更新，旧离散事件按 agentThreadId 合并 | 每个接收者独立状态；子任务失败不结束父轮次，支持只读嵌套子会话面板 |
| `mcpToolCall` | 保留 server/tool/status/arguments/appContext/pluginId/result/error；兼容旧 metadata、mcpAppResourceUri 与字符串 error，连接器自定义 JSON 不丢字段 | 按稳定 item.id 更新；完成快照保留已收到的 progress，错误可见 |
| `webSearch`（仅历史） | 恢复查询与结果；实时类型尚未接入 | 恢复搜索活动 |

当前 schema 中的 hookPrompt、functionCallOutput、plan、dynamicToolCall、webSearch、sleep、enteredReviewMode、exitedReviewMode 尚未接入实时 item 路径。协作枚举、字段校验与历史别名见 `items.rs`；历史解码见 `workspace_protocol.rs`。样式、尺寸和交互入口见 README 与组件实现。

## 方法总表

### 客户端请求（155）

| 方法 | API | 状态 | 已实现行为与限制 | 入口 |
|---|---|---|---|---|
| `account/bedrock/discover` | 实验 | 未接入 | — | — |
| `account/bedrock/setup` | 实验 | 未接入 | — | — |
| `account/login/cancel` | 默认 | 未接入 | — | — |
| `account/login/start` | 默认 | 未接入 | — | — |
| `account/logout` | 默认 | 未接入 | — | — |
| `account/rateLimitResetCredit/consume` | 默认 | 未接入 | — | — |
| `account/rateLimits/read` | 默认 | 未接入 | — | — |
| `account/read` | 默认 | 未接入 | — | — |
| `account/sendAddCreditsNudgeEmail` | 默认 | 未接入 | — | — |
| `account/usage/read` | 默认 | 未接入 | — | — |
| `account/workspaceMessages/read` | 默认 | 未接入 | — | — |
| `app/installed` | 默认 | 未接入 | — | — |
| `app/list` | 默认 | 未接入 | — | — |
| `app/read` | 默认 | 未接入 | — | — |
| `collaborationMode/list` | 实验 | 未接入 | — | — |
| `command/exec` | 默认 | 未接入 | — | — |
| `command/exec/resize` | 默认 | 未接入 | — | — |
| `command/exec/terminate` | 默认 | 未接入 | — | — |
| `command/exec/write` | 默认 | 未接入 | — | — |
| `config/batchWrite` | 默认 | 未接入 | — | — |
| `config/mcpServer/reload` | 默认 | 未接入 | — | — |
| `config/read` | 默认 | 未接入 | — | — |
| `config/value/write` | 默认 | 未接入 | — | — |
| `configRequirements/read` | 默认 | 未接入 | — | — |
| `environment/add` | 实验 | 未接入 | — | — |
| `environment/info` | 实验 | 未接入 | — | — |
| `environment/status` | 实验 | 未接入 | — | — |
| `experimentalFeature/enablement/set` | 默认 | 未接入 | — | — |
| `experimentalFeature/list` | 默认 | 未接入 | — | — |
| `externalAgentConfig/detect` | 默认 | 未接入 | — | — |
| `externalAgentConfig/import` | 默认 | 未接入 | — | — |
| `externalAgentConfig/import/readHistories` | 默认 | 未接入 | — | — |
| `externalAgentConfig/import/recordHistory` | 默认 | 未接入 | — | — |
| `feedback/upload` | 默认 | 未接入 | — | — |
| `fs/copy` | 默认 | 未接入 | — | — |
| `fs/createDirectory` | 默认 | 未接入 | — | — |
| `fs/getMetadata` | 默认 | 未接入 | — | — |
| `fs/readDirectory` | 默认 | 未接入 | — | — |
| `fs/readFile` | 默认 | 未接入 | — | — |
| `fs/remove` | 默认 | 未接入 | — | — |
| `fs/unwatch` | 默认 | 未接入 | — | — |
| `fs/watch` | 默认 | 未接入 | — | — |
| `fs/writeFile` | 默认 | 未接入 | — | — |
| `fuzzyFileSearch` | 默认 | 未接入 | — | — |
| `fuzzyFileSearch/sessionStart` | 实验 | 未接入 | — | — |
| `fuzzyFileSearch/sessionStop` | 实验 | 未接入 | — | — |
| `fuzzyFileSearch/sessionUpdate` | 实验 | 未接入 | — | — |
| `hooks/list` | 默认 | 未接入 | — | — |
| `initialize` | 默认 | 已接入 | 每个连接 generation 一次；发送 clientInfo、experimentalApi=true、requestAttestation=false。 | `manager` |
| `marketplace/add` | 默认 | 未接入 | — | — |
| `marketplace/remove` | 默认 | 未接入 | — | — |
| `marketplace/upgrade` | 默认 | 未接入 | — | — |
| `mcpServer/event/stream/start` | 实验 | 未接入 | — | — |
| `mcpServer/event/stream/stop` | 实验 | 未接入 | — | — |
| `mcpServer/oauth/login` | 默认 | 未接入 | — | — |
| `mcpServer/resource/read` | 默认 | 未接入 | — | — |
| `mcpServer/tool/call` | 默认 | 未接入 | — | — |
| `mcpServerStatus/list` | 默认 | 未接入 | — | — |
| `memory/reset` | 实验 | 未接入 | — | — |
| `mock/experimentalMethod` | 实验 | 未接入 | — | — |
| `model/list` | 默认 | 已接入 | limit=50、includeHidden=false；遍历 nextCursor，拒绝循环游标；返回模型、默认值、推理强度及服务档位。 | `manager/catalog` |
| `modelProvider/capabilities/read` | 默认 | 未接入 | — | — |
| `permissionProfile/list` | 默认 | 后端已接入 | cursor=null、limit=100、cwd；返回 id/allowed/extends；存在下一页时报错。后端可调用，当前无可见 UI 调用方。 | `manager/catalog` |
| `plugin/install` | 默认 | 未接入 | — | — |
| `plugin/installed` | 默认 | 未接入 | — | — |
| `plugin/list` | 默认 | 未接入 | — | — |
| `plugin/reconcile` | 默认 | 未接入 | — | — |
| `plugin/read` | 默认 | 未接入 | — | — |
| `plugin/search` | 实验 | 未接入 | — | — |
| `plugin/share/checkout` | 默认 | 未接入 | — | — |
| `plugin/share/delete` | 默认 | 未接入 | — | — |
| `plugin/share/list` | 默认 | 未接入 | — | — |
| `plugin/share/save` | 默认 | 未接入 | — | — |
| `plugin/share/updateTargets` | 默认 | 未接入 | — | — |
| `plugin/skill/read` | 默认 | 未接入 | — | — |
| `plugin/uninstall` | 默认 | 未接入 | — | — |
| `process/kill` | 实验 | 未接入 | — | — |
| `process/resizePty` | 实验 | 未接入 | — | — |
| `process/spawn` | 实验 | 未接入 | — | — |
| `process/writeStdin` | 实验 | 未接入 | — | — |
| `project/create` | 实验 | 已接入 | 发送 idempotencyKey、name、roots[{path}]；以 result.project 更新工作区。 | `manager/workspace` |
| `project/delete` | 实验 | 已接入 | 按 projectId 删除；同步移除项目及关联列表状态。 | `manager/workspace` |
| `project/import` | 实验 | 未接入 | — | — |
| `project/list` | 实验 | 已接入 | 按 position 升序分页；提供侧栏项目数据。 | `manager/workspace` |
| `project/move` | 实验 | 已接入 | projectId、nullable beforeProjectId；使用服务端顺序。 | `manager/workspace` |
| `project/read` | 实验 | 未接入 | — | — |
| `project/update` | 实验 | 已接入 | 按需发送 name、roots；以 result.project 更新工作区。 | `manager/workspace` |
| `remoteControl/client/list` | 实验 | 未接入 | — | — |
| `remoteControl/client/revoke` | 实验 | 未接入 | — | — |
| `remoteControl/disable` | 实验 | 未接入 | — | — |
| `remoteControl/enable` | 实验 | 未接入 | — | — |
| `remoteControl/pairing/start` | 实验 | 未接入 | — | — |
| `remoteControl/pairing/status` | 实验 | 未接入 | — | — |
| `remoteControl/status/read` | 实验 | 未接入 | — | — |
| `review/start` | 默认 | 未接入 | 启动模型代码评审；本机 Git 审查面板使用 Git/gh 与已有 turn diff，不调用此方法。 | — |
| `server/diagnostics` | 实验 | 未接入 | — | — |
| `skills/config/write` | 默认 | 未接入 | — | — |
| `skills/extraRoots/set` | 默认 | 未接入 | — | — |
| `skills/list` | 默认 | 未接入 | — | — |
| `thread/approveGuardianDeniedAction` | 默认 | 未接入 | — | — |
| `thread/archive` | 默认 | 已接入 | 按 threadId 归档；通知与列表合并规则见“连接与状态”。 | `manager/workspace` |
| `thread/backgroundTerminals/clean` | 实验 | 未接入 | — | — |
| `thread/backgroundTerminals/list` | 实验 | 未接入 | — | — |
| `thread/backgroundTerminals/terminate` | 实验 | 未接入 | — | — |
| `thread/compact/start` | 默认 | 未接入 | — | — |
| `thread/decrement_elicitation` | 实验 | 未接入 | — | — |
| `thread/delete` | 默认 | 已接入 | 按 threadId 删除；从所有侧栏集合移除。 | `manager/workspace` |
| `thread/fork` | 默认 | 已接入 | 仅用于临时侧边聊天：ephemeral=true、excludeTurns=true、threadSource=user，携带 cwd、说明及可选 model/effort/serviceTier。验证新 id、ephemeral 和先到的 thread/started；无持久化分叉 UI。 | `manager/side_conversation` |
| `thread/goal/clear` | 默认 | 未接入 | — | — |
| `thread/goal/get` | 默认 | 未接入 | — | — |
| `thread/goal/set` | 默认 | 未接入 | — | — |
| `thread/increment_elicitation` | 实验 | 未接入 | — | — |
| `thread/inject_items` | 默认 | 已接入 | 向新侧边线程注入 user message，标记父历史仅供参考；不开始 turn。失败时释放临时 fork，不交付可发送的线程。 | `manager/side_conversation` |
| `thread/items/list` | 默认 | 已接入 | 按 threadId、nullable turnId 升序分页；补全非 full 的历史轮次。 | `manager/workspace` |
| `thread/list` | 默认 | 已接入 | 分页读取最近、归档、项目与分区列表；保留前后游标及 projectId/sectionId 的省略、null、值三态。 | `manager/workspace` |
| `thread/loaded/list` | 默认 | 未接入 | — | — |
| `thread/memoryMode/set` | 实验 | 未接入 | — | — |
| `thread/metadata/update` | 默认 | 已接入 | projectId 省略表示不变，空字符串表示移出项目，非空 id 表示分配；读取 result.thread。 | `manager/workspace` |
| `thread/name/set` | 默认 | 已接入 | threadId、name；重命名会话。 | `manager/workspace` |
| `thread/queue/add` | 实验 | 未接入 | — | — |
| `thread/queue/delete` | 实验 | 未接入 | — | — |
| `thread/queue/list` | 实验 | 未接入 | — | — |
| `thread/queue/reorder` | 实验 | 未接入 | — | — |
| `thread/queue/start` | 实验 | 未接入 | — | — |
| `thread/queue/update` | 实验 | 未接入 | — | — |
| `thread/read` | 默认 | 已接入 | includeTurns=false；读取线程上下文，历史另行分页。 | `manager/workspace` |
| `thread/realtime/appendAudio` | 实验 | 未接入 | — | — |
| `thread/realtime/appendSpeech` | 实验 | 未接入 | — | — |
| `thread/realtime/appendText` | 实验 | 未接入 | — | — |
| `thread/realtime/listVoices` | 实验 | 未接入 | — | — |
| `thread/realtime/start` | 实验 | 未接入 | — | — |
| `thread/realtime/stop` | 实验 | 未接入 | — | — |
| `thread/resume` | 默认 | 已接入 | threadId、excludeTurns=true；当前 generation 未加载时执行一次，返回 id 必须匹配；失败不回退为新建。 | `manager/turn` |
| `thread/revert` | 默认 | 未接入 | — | — |
| `thread/rollback` | 默认 | 未接入 | — | — |
| `thread/search` | 实验 | 已接入 | 非空 searchTerm、archived、分页和排序；返回 thread 与 snippet。 | `manager/workspace` |
| `thread/searchOccurrences` | 实验 | 未接入 | — | — |
| `thread/section/move` | 默认 | 已接入 | threadId、nullable sectionId/beforeThreadId；用于置顶和取消置顶。 | `manager/workspace` |
| `thread/settings/update` | 实验 | 已接入 | 更新已有线程权限：approvalPolicy、approvalsReviewer、profile 或 sandboxPolicy。先注册 waiter，等待对应 thread/settings/updated；临时线程沿用所属 generation。 | `manager/catalog`、`permissions` |
| `thread/shellCommand` | 默认 | 未接入 | — | — |
| `thread/start` | 默认 | 已接入 | 首条提示词才新建；发送 cwd、projectId、historyMode=paginated、ephemeral=false、serviceName、model、serviceTier；采用 result.thread.id。 | `manager/turn` |
| `thread/timeline/list` | 实验 | 未接入 | — | — |
| `thread/turns/list` | 默认 | 已接入 | 按 threadId 升序分页，itemsView=full；保留实际 itemsView 和双向游标，按需补取 item。 | `manager/workspace` |
| `thread/unarchive` | 默认 | 已接入 | 按 threadId 取消归档；读取 result.thread 并刷新列表。 | `manager/workspace` |
| `thread/unsubscribe` | 默认 | 已接入 | 只关闭本应用创建的临时线程；先请求中断自身轮次，接受 unsubscribed/notSubscribed/notLoaded；不影响父线程。 | `manager/side_conversation` |
| `threadSection/create` | 默认 | 已接入 | name、可选 appearance{icon,color}；返回 section，当前用于建立 Pinned 分区。 | `manager/workspace` |
| `threadSection/delete` | 默认 | 未接入 | — | — |
| `threadSection/list` | 默认 | 已接入 | 遍历分区分页；以服务端 Pinned 的稳定 id 实现置顶。 | `manager/workspace` |
| `threadSection/update` | 默认 | 未接入 | — | — |
| `turn/interrupt` | 默认 | 已接入 | 定向 threadId/turnId，每轮最多发送一次；等待真实 interrupted 终态，保持共享连接。 | `manager/turn` |
| `turn/settings/update` | 实验 | 未接入 | — | — |
| `turn/start` | 默认 | 已接入 | 文本及 localImage 输入、路径上下文、model/effort/serviceTier、可选 plan/default collaborationMode；新线程首轮附权限字段。以 result.turn.id 建立轮次归属。 | `manager/turn` |
| `turn/steer` | 默认 | 未接入 | — | — |
| `windowsSandbox/readiness` | 默认 | 未接入 | — | — |
| `windowsSandbox/setupStart` | 默认 | 未接入 | — | — |

### 客户端通知（1）

| 方法 | API | 状态 | 已实现行为与限制 | 入口 |
|---|---|---|---|---|
| `initialized` | 默认 | 已接入 | initialize 成功后发送一次 params={}，无 id。 | `manager` |

### 服务端请求（11）

| 方法 | API | 状态 | 已实现行为与限制 | 入口 |
|---|---|---|---|---|
| `account/chatgptAuthTokens/refresh` | 默认 | 未接入 | — | — |
| `applyPatchApproval` | 默认 | 未接入 | 旧版文件审批；不由现有展示组件接管。 | — |
| `attestation/generate` | 默认 | 未接入 | — | — |
| `currentTime/read` | 实验 | 未接入 | — | — |
| `execCommandApproval` | 默认 | 未接入 | 旧版命令审批；不与 v2 item 请求混用。 | — |
| `item/commandExecution/requestApproval` | 默认 | 已接入 | kind 缺省为 command，支持 writeStdin；保留 approvalId、startedAtMs、nullable environmentId/cwd/command/reason、网络 host/protocol 与 additionalPermissions。availableDecisions 缺省／null 使用历史决策及服务端建议；显式空列表显示错误。按有序决策及完整策略载荷校验 accept、acceptForSession、decline、cancel、execpolicy 和网络 allow/deny，拒绝未提供的决策；cancel 不改写为 decline。 | `approvals`、`requests`、`registry` |
| `item/fileChange/requestApproval` | 默认 | 已接入 | 校验 threadId/turnId/itemId/startedAtMs，保留 nullable reason/grantRoot。原始 item changes 到达前仅允许拒绝；支持 accept、acceptForSession、decline、cancel，原 id 回传并等待 resolved。文件行打开对应原始补丁；grantRoot 是 schema 标注的不稳定提示，不由客户端自行扩大写入权限。 | `approvals`、`requests`、`registry`、`dispatch` |
| `item/permissions/requestApproval` | 默认 | 已接入 | 校验 thread/turn/item、cwd、startedAtMs、nullable environmentId/reason；保留 read/write、entries、glob 深度、path/glob/special path 与 nullable network。允许只返回请求子集及 turn/session scope，拒绝返回空权限。 | `requests`、`permissions`、`registry` |
| `item/tool/call` | 默认 | 未接入 | — | — |
| `item/tool/requestUserInput` | 默认 | 已接入 | 保留 question id/header/question/options/isOther/isSecret、isBlocking、nullable autoResolutionMs；返回 question id → 字符串数组的 answers，Debug 隐去答案；兼容 tool/requestUserInput 别名。 | `requests`、`registry` |
| `mcpServer/elicitation/request` | 默认 | 未接入 | — | — |

### 服务端通知（81）

| 方法 | API | 状态 | 已实现行为与限制 | 入口 |
|---|---|---|---|---|
| `account/login/completed` | 默认 | 未接入 | — | — |
| `account/rateLimits/updated` | 默认 | 已接入 | 应用级稀疏快照：合并窗口、credits、spend control 等可用字段；nullable 字段不清除已知值，不依附活动轮次。 | `manager/dispatch`、`notifications` |
| `account/updated` | 默认 | 未接入 | — | — |
| `app/list/updated` | 默认 | 未接入 | — | — |
| `autoApprovalReview/strictReviewRequired` | 默认 | 已接入 | 按 thread/turn/startedAtMs 保存独立复核提示，同一时间去重；只展示额外安全检查状态，无 request id 或人工审批 responder，不改变 turn 终态。 | `auto_approval`、`manager/dispatch` |
| `command/exec/outputDelta` | 默认 | 未接入 | — | — |
| `configWarning` | 默认 | 已接入 | 应用级 summary 及可选 details/path/range；无活动轮次仍显示配置警告。 | `manager/dispatch`、`notifications` |
| `deprecationNotice` | 默认 | 未接入 | — | — |
| `error` | 默认 | 已接入 | 定向轮次的 error.message、details、willRetry；显示错误信息，终态仍等待 turn/completed。 | `notifications` |
| `externalAgentConfig/import/completed` | 默认 | 未接入 | — | — |
| `externalAgentConfig/import/progress` | 默认 | 未接入 | — | — |
| `fs/changed` | 默认 | 未接入 | — | — |
| `fuzzyFileSearch/sessionCompleted` | 默认 | 未接入 | — | — |
| `fuzzyFileSearch/sessionUpdated` | 默认 | 未接入 | — | — |
| `guardianWarning` | 默认 | 已接入 | 线程级 message，允许无活动轮次；同一当前轮次去重，通用消息保留原文，反复拒绝提示显示状态分隔行。schema 无 turnId/reviewId，不推定归属或终态。 | `auto_approval`、`manager/dispatch` |
| `hook/completed` | 默认 | 未接入 | — | — |
| `hook/started` | 默认 | 未接入 | — | — |
| `item/agentMessage/delta` | 默认 | 已接入 | 按 thread/turn/item 追加 delta，进入所属会话的文本流。 | `notifications` |
| `item/autoApprovalReview/completed` | 默认 | 已接入 | 以 threadId/turnId/reviewId 原位更新；保留完整 action、nullable targetItemId/rationale/riskLevel/userAuthorization、startedAtMs/completedAtMs 与 decisionSource=agent。处理 approved/denied/timedOut/aborted，不替代 turn/completed。 | `auto_approval`、`notifications`、`manager/dispatch` |
| `item/autoApprovalReview/started` | 默认 | 已接入 | 支持 command/execve/writeStdin/applyPatch/networkAccess/mcpToolCall/requestPermissions 七类动作及共享的五种状态；targetItemId 可缺失或为 null。开始通知不得覆盖已完成结果。 | `auto_approval`、`notifications`、`manager/dispatch` |
| `item/commandExecution/outputDelta` | 默认 | 已接入 | 按 itemId 追加命令输出 delta。 | `dispatch` |
| `item/commandExecution/terminalInteraction` | 默认 | 已接入 | 保留 itemId、processId；stdin 仅转为“是否写入”的布尔值，正文不进入领域或 UI 状态；复用命令活动。 | `dispatch` |
| `item/completed` | 默认 | 部分接入 | 接入类型见“Item 与历史兼容”；以 item 载荷状态更新，不以通知名称推定成功。未知实时类型使连接失败。 | `dispatch`、`items` |
| `item/fileChange/outputDelta` | 默认 | 已接入 | deprecated；校验 thread/turn/item/delta，不再产生内容事件。 | `dispatch` |
| `item/fileChange/patchUpdated` | 默认 | 已接入 | 按 itemId 替换 changes[path/diff/kind]，刷新文件卡与差异统计。 | `dispatch`、`items` |
| `item/mcpToolCall/progress` | 默认 | 已接入 | 按 itemId 追加 message，完成快照保留进度；孤立 progress 不创建工具项。 | `dispatch` |
| `item/plan/delta` | 默认 | 未接入 | — | — |
| `item/reasoning/summaryPartAdded` | 默认 | 已接入 | 按 itemId 和非负 summaryIndex 建立槽位；孤立增量不创建 reasoning 项。 | `dispatch` |
| `item/reasoning/summaryTextDelta` | 默认 | 已接入 | 按 itemId/summaryIndex 追加 delta；仅合并相邻且同 item/index 的事件。 | `dispatch` |
| `item/reasoning/textDelta` | 默认 | 已接入 | 按 itemId/contentIndex 追加 delta；summary 为空时以 content 展示正文。 | `dispatch` |
| `item/started` | 默认 | 部分接入 | 接入类型见“Item 与历史兼容”；创建或原位更新活动。userMessage 只校验，不重复显示用户提交；未知实时类型使连接失败。 | `dispatch`、`items` |
| `mcpServer/event/stream/notification` | 默认 | 未接入 | — | — |
| `mcpServer/oauthLogin/completed` | 默认 | 未接入 | — | — |
| `mcpServer/startupStatus/updated` | 默认 | 已接入 | 按 app 或 thread/server 保存 starting/ready/failed/cancelled；threadId/error/failureReason 可省略或 null，仅接受 reauthenticationRequired 原因；不结束 turn。 | `manager/dispatch`、`notifications` |
| `model/rerouted` | 默认 | 已接入 | 定向轮次的 fromModel/toModel/reason，更新实际模型与提示。 | `notifications` |
| `model/safetyBuffering/updated` | 默认 | 已接入 | 保留 model/useCases/reasons/showBufferingUi、nullable fasterModel；更新所属会话的安全检查状态。 | `notifications` |
| `model/verification` | 默认 | 已接入 | 读取 verifications[]；目标 Composer 显示账户验证要求并进入失败状态。 | `notifications` |
| `modelProvider/authRecoveryCompleted` | 默认 | 未接入 | — | — |
| `modelProvider/authRecoveryStarted` | 默认 | 未接入 | — | — |
| `process/exited` | 默认 | 未接入 | — | — |
| `process/outputDelta` | 默认 | 未接入 | — | — |
| `project/changed` | 默认 | 已接入 | projectId、created/updated/deleted；刷新或移除项目，可先于 RPC 响应。 | `manager/dispatch` |
| `remoteControl/status/changed` | 默认 | 后端已接入 | 校验 status/serverName/installationId、nullable environmentId，保存连接快照；无 Composer UI。 | `manager/dispatch`、`notifications` |
| `serverRequest/resolved` | 默认 | 已接入 | 按原类型 requestId 找到所属轮次，再核对 thread/item/kind，释放命令／文件／权限审批或输入 responder 与活动 owner。已知同线程的重复及终态后迟到通知幂等忽略；未知 id 或错配 thread 报错；过期 handle 始终不可回复。 | `manager/dispatch`、`manager/connection`、`requests`、`registry` |
| `skills/changed` | 默认 | 未接入 | — | — |
| `thread/archived` | 默认 | 已接入 | 按 threadId 移除最近、项目及置顶条目，刷新归档；覆盖迟到快照。 | `manager/dispatch` |
| `thread/closed` | 默认 | 已接入 | 从当前 generation 的已加载集合移除并发布关闭状态；侧边聊天保留消息，禁用发送。 | `manager/dispatch` |
| `thread/compacted` | 默认 | 未接入 | — | — |
| `thread/deleted` | 默认 | 已接入 | 按 threadId 从所有集合移除；迟到列表不得恢复已删除线程。 | `manager/dispatch` |
| `thread/environment/connected` | 默认 | 未接入 | — | — |
| `thread/environment/disconnected` | 默认 | 未接入 | — | — |
| `thread/goal/cleared` | 默认 | 部分接入 | 仅兼容既有线程 resume bootstrap：从 thread/resume 开始至随后 turn/start 响应处理完毕，要求 threadId 匹配；窗口外报错，不建立 goal 状态。 | `manager/dispatch`、`notifications` |
| `thread/goal/updated` | 默认 | 未接入 | 未建立 goal 领域状态或 UI。 | — |
| `thread/name/updated` | 默认 | 已接入 | threadId、可省略或 null 的 threadName；即时更新名称并覆盖迟到快照。 | `manager/dispatch` |
| `thread/project/updated` | 默认 | 已接入 | threadId、必需但 nullable 的 projectId；移动或移出项目并覆盖迟到快照。 | `manager/dispatch` |
| `thread/queue/changed` | 默认 | 未接入 | — | — |
| `thread/realtime/closed` | 默认 | 未接入 | — | — |
| `thread/realtime/error` | 默认 | 未接入 | — | — |
| `thread/realtime/item/completed` | 默认 | 未接入 | — | — |
| `thread/realtime/item/started` | 默认 | 未接入 | — | — |
| `thread/realtime/item/transcript/delta` | 默认 | 未接入 | — | — |
| `thread/realtime/itemAdded` | 默认 | 未接入 | — | — |
| `thread/realtime/outputAudio/delta` | 默认 | 未接入 | — | — |
| `thread/realtime/sdp` | 默认 | 未接入 | — | — |
| `thread/realtime/started` | 默认 | 未接入 | — | — |
| `thread/realtime/transcript/delta` | 默认 | 未接入 | — | — |
| `thread/realtime/transcript/done` | 默认 | 未接入 | — | — |
| `thread/reverted` | 默认 | 未接入 | — | — |
| `thread/settings/updated` | 默认 | 已接入 | 只同步目标线程的 model/effort/serviceTier/cwd 和有效权限；包含 permissions 时才满足权限更新 waiter。 | `manager/dispatch`、`notifications` |
| `thread/started` | 默认 | 已接入 | 校验 params.thread.id，关联当前 start/resume/fork；RPC 响应是最终 id 来源。已加载线程的迟到通知不得绑定到下一次生命周期请求。 | `manager/dispatch` |
| `thread/status/changed` | 默认 | 已接入 | 按 threadId 保存 notLoaded/idle/systemError/active；active 仅接受 waitingOnApproval/waitingOnUserInput，不替代 turn 终态。 | `manager/dispatch`、`notifications` |
| `thread/tokenUsage/updated` | 默认 | 已接入 | 按 threadId/turnId 保存 tokenUsage.total/last 与可选 context window；不创建活动或结束轮次。 | `notifications` |
| `thread/unarchived` | 默认 | 已接入 | 从归档移除，刷新最近及项目列表；覆盖迟到快照。 | `manager/dispatch` |
| `turn/completed` | 默认 | 已接入 | 接受 completed/interrupted/failed；失败读取 message/details。每轮只发送一个终态并清理自身请求，其他轮次及共享连接继续存活。 | `dispatch`、`manager/connection` |
| `turn/diff/updated` | 默认 | 已接入 | 所属轮次最新聚合 unified diff；保留原始 patch，刷新文件卡与“上一轮”范围；空 diff 不清除已有 item changes。 | `dispatch` |
| `turn/moderationMetadata` | 默认 | 未接入 | — | — |
| `turn/plan/updated` | 默认 | 未接入 | 未建立计划进度 UI；发送 plan collaborationMode 不代表接入此通知。 | — |
| `turn/started` | 默认 | 已接入 | 要求 turn.status=inProgress；可早于 turn/start 响应，验证后使所属会话进入流式状态。 | `notifications`、`manager/turn` |
| `warning` | 默认 | 已接入 | message、可选 threadId；应用级警告无活动轮次仍可见，线程级只进入目标 Composer。 | `manager/dispatch`、`notifications` |
| `windows/worldWritableWarning` | 默认 | 未接入 | — | — |
| `windowsSandbox/setupCompleted` | 默认 | 未接入 | — | — |

## 维护与验证

修改方法、有效变体、兼容别名或失败处理时，同步更新本表与对应测试；升级 CLI 时核对四个 schema union（ClientRequest、ServerRequest、ClientNotification、ServerNotification），保持方法唯一、方向／API 分类和状态统计一致。只有形成表中声明的产品路径后才标记“已接入”。

```bash
cargo test agent::codex
cargo test workspace::
cargo test conversation::
cargo test components::composer
cargo test side_ -- --test-threads=1
```

协议解析与反向请求回归在 `src/agent/codex/tests.rs`；共享进程、乱序响应、线程隔离、清理与退出回收在 `manager/tests.rs`，临时侧边线程在 `manager/tests/side_conversation.rs`。实际模型请求测试默认忽略；常规回归使用 scripted transport 或 fake backend。构建与界面验收入口见 [README.md](../README.md)。
