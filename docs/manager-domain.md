# Manager Domain 与 SQLite

## 1. 本地持久化与远端事实来源

Manager 使用 `runtime/data/manager.sqlite3` 持久化设置、operation、本地 profile、事件和可丢弃缓存，可通过 `BROSDK_DATA_DIR` 改写目录。SDK 服务端是环境配置的唯一事实来源；数据库启用 WAL、foreign keys 和 5 秒 busy timeout。Dashboard 不直接访问 SQLite，只通过 Tauri command 获取 snapshot、operation 和递增事件。

阶段 11 缓存规则：

- `environments`/`environment_details` 只缓存 DLL 从 SDK 服务端返回的脱敏数据，不允许本地名称或标签覆盖。
- 每次同步读取全部分页；全部成功后单事务替换缓存并删除远端已不存在的行。
- 任一分页失败时不写入部分结果，保留上一份缓存并标记 stale。
- generation、reqId、CDP、ready/stopped 属于当前设备运行态，可与缓存一起保存，但不能覆盖远端环境配置。

当前 schema version 为 6：

| 表 | 用途 |
| --- | --- |
| `settings` | dataDir、workDir、extensionDir、logDir、sdkApiUrl、debug、startupPolicy、embeddedMcpPort、aiBaseUrl、aiModel |
| `environments` | SDK 服务端环境的可丢弃脱敏缓存，以及当前设备 generation、状态和 CDP |
| `environment_cache_status` | 缓存 fresh/stale/empty、条数、最后成功/尝试时间和脱敏错误 |
| `operations` | queued/running/succeeded/failed/cancelled 状态机与脱敏 request snapshot |
| `runtime_snapshots` | 每个 envId 最近一次运行事实 |
| `proxy_profiles` | 本地代理 profile；只保存 secret reference |
| `fingerprint_profiles` | 兼容旧数据的本地指纹 profile；不参与普通环境创建、启动或远端指纹主流程 |
| `environment_details` | `sdk_env_getinfo` 的可丢弃脱敏指纹/代理/内核缓存 |
| `kernel_records` | SDK catalog 与本地 `.core.json` 合并视图 |
| `manager_events` | AUTOINCREMENT sequence 的增量事件流 |
| `schema_migrations` | 已应用 schema version |

### envId 身份规则

- SDK 服务端分配的 `envId` 是不可变唯一主键；环境名称允许重复，只用于展示和搜索。
- `environments.env_id`、`runtime_snapshots.env_id` 和 `environment_details.env_id` 都是主键；详情通过外键绑定环境并随环境缓存删除。
- Manager operation、generation、runtime callback、详情刷新、代理绑定和单环境 MCP 路由都只接受精确 envId，不用名称或数组位置反查。
- Dashboard snapshot 在渲染前拒绝空/重复环境 envId、重复详情 envId 和悬空详情绑定；表格选择、React key、指纹列及 mutation 参数始终使用 envId。

## 2. Operation 队列

所有 operation 先以 `queued` 写入数据库，再等待 FIFO Tokio Mutex。获得执行权后转为 `running`，最终进入 `succeeded`、`failed` 或 `cancelled`。operation 状态更新和对应 `operation.<status>` 事件写入同一个 SQLite transaction，页面刷新不会看到状态与事件不一致。

允许的转换：

```text
queued -> running -> succeeded
   |         +----> failed
   +--------------> cancelled / failed
```

用户取消只允许 `queued` operation。进入 `running` 后，Manager 和 SQLite 状态机都拒绝转为 `cancelled`，因为底层 SDK/DLL 请求已经开始，单纯修改数据库状态不能终止实际调用。失败或取消后的重试也只开放给 Manager 有确定重放实现的 `environment.sync`、`runtime.reconcile`、`environment.start`、`environment.stop` 和 `kernel.install`；其它 operation 不显示误导性的重试入口。

## 3. Environment generation

每次启动或停止操作先递增环境 generation，并把该 generation 写入 operation。SDK callback 只有在以下条件全部成立时才能修改环境状态：

- callback 能映射到 operation id；
- operation 仍是 queued/running；
- callback envId 或 operation envId 能定位环境；
- operation generation 等于环境当前 generation。

因此旧启动请求的晚到 `browser-open-success` 不会把已经停止或进入新一轮启动的环境改回 ready。callback 本身仍写入 `manager_events`，便于诊断。

启动/停止操作在调用 SDK 前进入 `running`，环境进入 `preparing`/`stopping`。`sdk_browser_open` / `sdk_browser_close` 的同步非负返回值是 accepted 状态码，不保证等于 callback `reqId`，也不会让环境进入 ready。Runtime Host 按 `envId + open/close` 方向暂存 operation，首个 lifecycle callback 到达后写入真实 `reqId` 并建立后续映射；如果 terminal callback 早于 host 同步响应，accepted 更新会检查 operation 当前状态，已经完成的 `succeeded/ready` 不会被回退。

`sdk_browser_info` 对账不会在活跃 start operation 期间因列表为空把环境提前改回 stopped；Starting/无 CDP 条目也不会被当作 ready。

## 4. Snapshot 与增量事件

`manager_snapshot` 返回 SDK/runtime、环境缓存、`environmentCache` 新鲜度、最近 100 条 operation、settings、数据库路径和 `latestEventSequence`。进程启动时持久化缓存先视为 stale；API Key 可用时首次 snapshot 自动尝试刷新一次。Dashboard 可从该 sequence 调用 `manager_events_since`，每次最多读取 500 条事件。

未配置 API Key 时 snapshot 不启动 Runtime Host，Dashboard 只渲染首次初始化页。`manager_configure_api_key` 使用候选 Key 新建 Host，依次完成 `sdk_get_user_sig(role=user)`、`sdk_init` 和完整 `sdk_env_page`；全部成功后才写入平台安全存储并替换账号缓存。`manager_clear_api_key` 与成功换号会清除环境、详情、runtime snapshot、operation、event、Agent 幂等记录和旧环境绑定，保留本地代理/内核资源。环境变量来源是测试/受管部署覆盖项，不能从 Dashboard 更改。

host 进入 degraded 时，Manager 把 preparing/starting/ready/stopping 环境改为 unknown，并把 queued/running operation 标为 `HOST_DEGRADED`。Manager 不自动无限重启 host。

## Profile 与凭据

- 指纹 profile 表保留旧数据兼容和后续专家能力；普通 Dashboard 不再以本地 JSON profile 作为环境指纹事实或创建输入。
- 代理 profile 在 SQLite 中保存 scheme、host、port、username、环境绑定和 `secret_ref`。Windows 密码通过当前用户 DPAPI 加密后写入 `<dataDir>/secrets/*.bin`，不会以明文进入 SQLite、事件或诊断包。
- AI Provider 的 Base URL 和模型属于非敏感 settings；AI API Key 使用独立平台 secret reference。`BROSDK_AI_API_KEY/BROSDK_AI_BASE_URL/BROSDK_AI_MODEL` 可作为受管部署覆盖项，Dashboard 不覆盖环境变量来源。
- 环境详情缓存只保留指纹、代理和浏览器/内核摘要，Cookie、token、secret 等字段不进入本地详情表。

## 设置迁移

修改 `dataDir` 时 Manager 使用 SQLite backup API 写入新目录，复制受保护凭据，并写入平台配置指针；当前进程继续使用原连接，下次启动切换到新目录。`workDir`、`extensionDir`、`logDir` 在保存时创建并校验非空。

## 5. 同步与对账

`manager_sync_environments` 串行执行：

```text
queued -> initialize SDK once -> sdk_env_page(page=1..N) -> atomic replace -> succeeded
                                                       \-> preserve cache + stale on failure
```

分页默认每页 200 条，最多 500 页/100000 个唯一 envId。Manager 根据 `data.total`（兼容 count）继续拉取，按 envId 去重；总数中途变化、空页提前结束、重复页无新增或超过上限都视为失败。缓存写入在 Manager 持久化入口再次脱敏，成功事务会删除服务端已不存在的 environment、runtime snapshot 和级联 detail。

`browser-open-success` 首先完成 lifecycle operation 并把环境置为 ready；如果事件 payload 含明确 DevTools URL 或非零调试端口，同一事务直接保存 CDP。若 callback 没有 endpoint，Manager 随后查询一次 `sdk_env_getinfo`，未命中再短时轮询 `sdk_browser_info`。三路数据共用严格解析器，只接受 CDP/Debugger 专用地址键和端口键，兼容数值、数字字符串、命名变体与 JSON 编码子对象，不接受普通 `port`。

`manager_reconcile_runtimes` 调用 `sdk_browser_info`，把带明确 CDP endpoint 的环境对账为 ready，并可为已经 ready 的环境补充后到的 `remoteDebuggingPort`；把本地活动但不再存在的环境改为 stopped。只包含 envId 且端口为 0 的条目代表 DLL 仍跟踪该环境，但不能伪造 TCP 地址；已由 callback 确认 ready 的环境保持 ready，并在 Dashboard 标记为 DLL 内部 CDP/MCP 控制通道。该路径用于手动关闭浏览器后的状态恢复。

AI Chat/Agent 的环境上下文按选中的精确 `envId` 构造。Dashboard 可以显示本地完整 CDP 地址，但 Manager 发送给模型前只保留安全 origin；`ready`、`-` 和其它非地址值不会被当成 endpoint。pipe-only ready 环境发送 `controlChannel=sdk-browser-command`、`cdpAvailable=false`，外部模型不能据此获得本地 DevTools 控制路径。

`manager_create_environment` 串行执行：校验 proxy profile 和本地已安装内核，临时恢复受保护代理 URL，调用 `sdk_env_create`，校验后端 `code=200` 与 `data.envId`，立即 upsert 创建结果，再尽力执行 `sdk_env_page` 完整对账。远端创建已经成功但后续分页同步失败时，operation 仍成功并标记镜像刷新延后，避免盲目重试造成重复环境。

`manager_refresh_environment_detail(envId)` 只读取一个缓存中存在的环境，调用 `sdk_env_getinfo` 并校验 `code=200`。持久化采用显式响应边界：保留递归脱敏后的 `finger`、从 `browser` 直接读取的内核、去除密码的代理摘要，以及 `envName/serial/enableDevtools/enableStorage` 元数据；不递归猜测 `kernel`，不保存 Cookie、Storage、上传路径或 DEK。响应中的 CDP endpoint 不进入详情 JSON，而是通过 ready-only 运行态事务写入 environment/runtime snapshot；该事务保留 generation、reqId、current operation 和 last event，并只追加不含完整地址的 `runtime.cdp-hydrated` 来源事件。operation 与事件都绑定该 `envId`，因此多环境界面不会因为查看一个环境而串行读取全部环境。

`manager_update_environment_metadata` 只接受 `envId/envName/serial`，要求环境处于 stopped，并按服务端规则校验名称最多 32 个 Unicode 字符、序号最多 64 个 UTF-8 字节。远端写入成功还必须校验业务 `code=200` 和响应回显与请求完全一致，之后重新分页并刷新详情。若旧版 `getEnvInfo` 把序号返回为空，snapshot 只用环境分页或已确认的更新响应补全；表单值不会直接成为本地事实。

环境本地数据清理与服务端删除是两个不同 operation。`environment.local-data-cleanup` 要求 stopped，调用 `sdk_browser_cleanup({envs:[envId]})`，只返回计数摘要；DLL 原响应中的 userDataDir、cleanupPath 和逐项 envId 不进入 Dashboard/event。`environment.destroy` 同样要求 stopped，调用 `sdk_env_destroy` 并删除本地镜像。两者都不自动代替另一个动作。

`environment.page-diagnostic` 要求 ready，调用 `sdk_browser_snapshot` 时固定关闭 HTML、截图和 callback chunks。Manager 将结果压成 pageCount、failedPages、page status 和 URL origin；页面标题、完整 URL、snapshotId、targetId、sessionId 与 chunks 不持久化也不返回前端。

`fingerprint.check` 同样要求 ready。DLL 原始 target/session/CDP 注入结果在 Manager 内压缩为 opened/newTab/source，不进入 Dashboard、SQLite 或事件。

批量环境启停不创建共享生命周期 operation。Manager 先拒绝空列表、重复 envId、超过 20 个以及状态不允许的整批请求，再按输入顺序调用单环境 start/stop；每个子项拥有独立 operation、generation、pending callback 和失败状态。批量返回只汇总 requested/accepted/failed 与子 operation，不把 DLL 原生 batch 的一个 operation 绑定到多个环境。

创建 operation 的 request snapshot 只保存 `proxyProfileId` 和 `kernelId`；元数据更新 snapshot 只保存 `envId/envName/serial`。后端完整 DTO、代理 URL、customerId、API Key、userSig 和原始响应均不进入 operation request。测试清理使用 `sdk_env_destroy`，成功后事务删除本地 environment/runtime snapshot，environment detail 通过外键级联删除。

数据库和事件 payload 不保存 API Key、userSig、代理密码、Cookie 或完整 Authorization。SDK 数据在 host 出口和 Manager 持久化入口都执行脱敏。

## 6. DLL MCP 路由

Manager 是 DLL MCP 的唯一客户端边界。请求显式区分 `global` 与 `environment` scope：全局 scope 连接 `/sdk/v1/mcp`，不能携带 Manager 路由 envId；环境 scope 连接 `/sdk/v1/mcp/env/{envId}`，且只接受缓存中处于 ready 的环境。工具发现和调用分别写入 `mcp.tools-discover`、`mcp.global-tool-call` 或 `mcp.environment-tool-call` operation。

Manager 在建立 MCP session 前执行工具与参数白名单校验。全局只开放健康、环境读取、浏览器状态和任务读取；环境级只开放有尺寸、数量和超时上限的读取工具。DLL 广告 mutation 并不自动获得权限，环境生命周期与远端环境写入仍走既有 Manager operation。MCP 响应返回 Dashboard 前再次脱敏，URL 只保留 origin；operation request 仅记录 scope、工具名和参数键，不保存页面内容或参数值。
