# Manager Domain 与 SQLite

## 1. 本地事实来源

Manager 使用 `runtime/data/manager.sqlite3` 作为默认本地事实来源，可通过 `BROSDK_DATA_DIR` 改写目录。数据库启用 WAL、foreign keys 和 5 秒 busy timeout。Dashboard 不直接访问 SQLite，只通过 Tauri command 获取 snapshot、operation 和递增事件。

当前 schema version 为 1：

| 表 | 用途 |
| --- | --- |
| `settings` | workDir、extensionDir、logDir、sdkApiUrl、debug |
| `environments` | 远端环境镜像、本地标签、generation、当前状态和 CDP |
| `operations` | queued/running/succeeded/failed/cancelled 状态机 |
| `runtime_snapshots` | 每个 envId 最近一次运行事实 |
| `proxy_profiles` | 本地代理 profile；只保存 secret reference |
| `fingerprint_profiles` | 本地指纹 profile JSON |
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

## 4. Snapshot 与增量事件

`manager_snapshot` 返回 SDK/runtime、环境镜像、最近 100 条 operation、settings、数据库路径和 `latestEventSequence`。Dashboard 可从该 sequence 调用 `manager_events_since`，每次最多读取 500 条事件。

host 进入 degraded 时，Manager 把 preparing/starting/ready/stopping 环境改为 unknown，并把 queued/running operation 标为 `HOST_DEGRADED`。Manager 不自动无限重启 host。

## 5. 同步与对账

`manager_sync_environments` 串行执行：

```text
queued -> initialize SDK once -> sdk_env_page -> upsert mirror -> succeeded/failed
```

`manager_reconcile_runtimes` 调用 `sdk_browser_info`，把存在于返回值中的环境对账为 ready，把本地活动但不再存在的环境改为 stopped。该路径用于手动关闭浏览器后的状态恢复。

数据库和事件 payload 不保存 API Key、userSig、代理密码、Cookie 或完整 Authorization。SDK 数据在 host 出口和 Manager 持久化入口都执行脱敏。
