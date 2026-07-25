# Manager Domain 与 SQLite

## 1. 本地持久化与远端事实来源

Manager 使用 `runtime/data/manager.sqlite3` 持久化设置、operation、本地 profile、事件和可丢弃缓存，可通过 `BROSDK_DATA_DIR` 改写目录。SDK 服务端是环境配置的唯一事实来源；数据库启用 WAL、foreign keys 和 5 秒 busy timeout。Dashboard 不直接访问 SQLite，只通过 Tauri command 获取 snapshot、operation 和递增事件。

阶段 11 缓存规则：

- `environments`/`environment_details` 只缓存 DLL 从 SDK 服务端返回的脱敏数据，不允许本地名称或标签覆盖。
- 每次同步读取全部分页；全部成功后单事务替换缓存并删除远端已不存在的行。
- 任一分页失败时不写入部分结果，保留上一份缓存并标记 stale。
- generation、reqId、CDP、ready/stopped 属于当前设备运行态，可与缓存一起保存，但不能覆盖远端环境配置。

当前 schema version 为 4：

| 表 | 用途 |
| --- | --- |
| `settings` | dataDir、workDir、extensionDir、logDir、sdkApiUrl、debug、startupPolicy、embeddedMcpPort |
| `environments` | SDK 服务端环境的可丢弃脱敏缓存，以及当前设备 generation、状态和 CDP |
| `operations` | queued/running/succeeded/failed/cancelled 状态机与脱敏 request snapshot |
| `runtime_snapshots` | 每个 envId 最近一次运行事实 |
| `proxy_profiles` | 本地代理 profile；只保存 secret reference |
| `fingerprint_profiles` | 本地指纹 profile JSON |
| `environment_details` | `sdk_env_getinfo` 的可丢弃脱敏指纹/代理/内核缓存 |
| `kernel_records` | SDK catalog 与本地 `.core.json` 合并视图 |
| `manager_events` | AUTOINCREMENT sequence 的增量事件流 |
| `schema_migrations` | 已应用 schema version |

## 2. Operation 队列

所有 operation 先以 `queued` 写入数据库，再等待 FIFO Tokio Mutex。获得执行权后转为 `running`，最终进入 `succeeded`、`failed` 或 `cancelled`。operation 状态更新和对应 `operation.<status>` 事件写入同一个 SQLite transaction，页面刷新不会看到状态与事件不一致。

允许的转换：

```text
queued -> running -> succeeded
   |         |----> failed
   |         +----> cancelled
   +--------------> cancelled / failed
```

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

`manager_snapshot` 返回 SDK/runtime、环境镜像、最近 100 条 operation、settings、数据库路径和 `latestEventSequence`。Dashboard 可从该 sequence 调用 `manager_events_since`，每次最多读取 500 条事件。

host 进入 degraded 时，Manager 把 preparing/starting/ready/stopping 环境改为 unknown，并把 queued/running operation 标为 `HOST_DEGRADED`。Manager 不自动无限重启 host。

## Profile 与凭据

- 指纹 profile 只保存用户选择的 JSON 对象和环境绑定；导入/导出格式为 `brosdk-dashboard.fingerprint.v1`。
- 代理 profile 在 SQLite 中保存 scheme、host、port、username、环境绑定和 `secret_ref`。Windows 密码通过当前用户 DPAPI 加密后写入 `<dataDir>/secrets/*.bin`，不会以明文进入 SQLite、事件或诊断包。
- 环境详情缓存只保留指纹、代理和浏览器/内核摘要，Cookie、token、secret 等字段不进入本地详情表。

## 设置迁移

修改 `dataDir` 时 Manager 使用 SQLite backup API 写入新目录，复制受保护凭据，并写入平台配置指针；当前进程继续使用原连接，下次启动切换到新目录。`workDir`、`extensionDir`、`logDir` 在保存时创建并校验非空。

## 5. 同步与对账

`manager_sync_environments` 串行执行：

```text
queued -> initialize SDK once -> sdk_env_page -> upsert mirror -> succeeded/failed
```

`manager_reconcile_runtimes` 调用 `sdk_browser_info`，把存在于返回值中的环境对账为 ready，把本地活动但不再存在的环境改为 stopped。该路径用于手动关闭浏览器后的状态恢复。

`manager_create_environment` 串行执行：校验 proxy profile 和本地已安装内核，临时恢复受保护代理 URL，调用 `sdk_env_create`，校验后端 `code=200` 与 `data.envId`，立即 upsert 创建结果，再尽力执行 `sdk_env_page` 完整对账。远端创建已经成功但后续分页同步失败时，operation 仍成功并标记镜像刷新延后，避免盲目重试造成重复环境。

创建 operation 的 request snapshot 只保存 `proxyProfileId` 和 `kernelId`。后端 DTO、完整代理 URL、API Key、userSig 和原始响应均不进入 operation request。测试清理使用 `sdk_env_destroy`，成功后事务删除本地 environment/runtime snapshot，environment detail 通过外键级联删除。

数据库和事件 payload 不保存 API Key、userSig、代理密码、Cookie 或完整 Authorization。SDK 数据在 host 出口和 Manager 持久化入口都执行脱敏。
