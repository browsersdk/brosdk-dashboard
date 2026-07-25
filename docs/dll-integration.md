# brosdk.dll 集成规划

## 1. 当前可用 ABI

目标项目当前只携带 Windows x64 动态库：

```text
libs/windows_x64/brosdk.dll
libs/windows_x64/brosdk.h
```

头文件暴露的是 C ABI 和 C++ `ISDK`。新客户端只绑定 C ABI，不依赖 C++ vtable，避免编译器 ABI 差异。

关键 C API：

| 能力 | 函数 |
| --- | --- |
| 回调 | `sdk_register_result_cb`、`sdk_register_log_cb`、`sdk_register_cookies_storage_cb`、`sdk_register_security_decision_cb` |
| 认证 | `sdk_get_user_sig`、`sdk_token_update` |
| 初始化 | `sdk_init`、`sdk_init_async`、`sdk_info`、`sdk_shutdown` |
| 环境 CRUD | `sdk_env_create`、`sdk_env_update`、`sdk_env_page`、`sdk_env_getinfo`、`sdk_env_destroy` |
| 浏览器生命周期 | `sdk_browser_open`、`sdk_browser_close`、`sdk_browser_info` |
| 内核与清理 | `sdk_browser_install`、`sdk_browser_cleanup` |
| CDP 与诊断 | `sdk_browser_command`、`sdk_browser_env_check`、`sdk_browser_snapshot`、`sdk_network_diagnostics`、`sdk_system_proxy_diagnostics` |
| 内存 | `sdk_free`、`sdk_malloc` |

## 2. 集成方式

使用 Rust `libloading` 动态加载，不在编译期链接 import lib：

1. `sdk-host` 启动后解析当前平台动态库路径。
2. 加载 `brosdk.dll`。
3. 查找全部必需符号，缺一即失败并上报 capability。
4. 注册 result/log/cookie/security callbacks。
5. 使用环境变量或系统密钥库中的 API Key 调用 `sdk_get_user_sig`。
6. 使用返回的 userSig 调用 `sdk_init`。
7. 所有 SDK 调用由 `sdk-host` 单线程串行入口接收，内部可按 SDK 语义等待异步回调。

当前 `sdk-host serve --endpoint <pipe-or-socket>` 已实现上述隔离入口。Manager 不传 API Key 到 IPC；host 进程从继承的 `BROSDK_API_KEY` 环境变量读取认证信息。IPC 响应和 callback event 在发送前统一脱敏。

不要让 Dashboard 或 Manager 直接持有 DLL 函数指针。

## 3. 认证与初始化链路

测试链路：

```text
BROSDK_API_KEY -> sdk_get_user_sig -> userSig -> sdk_init -> sdk_info
```

示例请求形态：

```json
{
  "apiKey": "<from BROSDK_API_KEY>"
}
```

`sdk_init` 请求至少包含：

```json
{
  "userSig": "<from sdk_get_user_sig>",
  "workDir": "<app-data>/sdk-work",
  "debug": true
}
```

如果需要启用 DLL 内嵌 HTTP/WS，可额外传入 `port`，但首版客户端内部不依赖它。Dashboard 与 Manager 的通讯走 Tauri command/event 或本项目自己的 loopback API。

补充：当前 DLL 已自带 MCP 功能，和内嵌 HTTP/WS 服务同属 `sdk_init` 的 `port` 启用路径。新客户端首版把它标记为 `embedded_mcp` capability；需要 smoke 或后续自动化验证时，由 `sdk-host` 在隔离进程中传入端口。Manager 仍然是 Dashboard 与自动化工具的策略边界，负责 envId 路由、operation 追踪和敏感信息脱敏。

阶段 9 已验证 DLL 的真实 MCP 协议路径：全局管理端点为 `/sdk/v1/mcp`，单环境自动化端点为 `/sdk/v1/mcp/env/{envId}`。Dashboard/Agent 不直接持有 MCP session；Manager adapter 负责严格 lifecycle、只读工具白名单、ready 状态校验、operation 追踪、URL 降级和响应脱敏。

## 4. 进程锁风险

根据 v2 文档，SDK 的同款产品多实例限制发生在 `sdk_init` 成功拿到后端 `appId` 之后：

```text
sdk_init -> 后端 init -> appId -> OS instance lock -> conflict may exit
```

Windows 锁名形态类似：

```text
BroSDK-Appid-01E4B0DB-3CCC-<appId>
```

设计要求：

- 桌面 Shell 自己先做应用级单实例，只允许一个 UI。
- `sdk-host` 是唯一加载 DLL 的进程。
- 如果 DLL 因 appId 锁冲突退出，Manager 报告“同款产品已在运行”，不能让 UI 消失。
- `sdk-host` 崩溃后，Manager 不自动无限重启；需要把环境状态对账为 unknown/failed 并提示用户处理。

## 5. 生命周期语义

`sdk_browser_open` 是异步接口。返回 reqId 只表示请求被 SDK 接受。

产品层状态必须等待：

- result callback 中的 `browser-open-success`；
- 或 `sdk_browser_info` 对账显示目标环境已运行且 CDP ready；
- 或超时后获取 SDK 失败事件与安全错误信息。

`sdk_browser_close` 同理，返回 reqId 不等于已关闭。关闭完成以 `browser-close-success` 和 `sdk_browser_info` 对账为准。

## 6. 事件归一化

SDK callback 形态：

```c
typedef void (*sdk_result_cb_t)(
  int32_t code,
  void *user_data,
  const char *data,
  size_t len
);
```

`data/len` 是 UTF-8 JSON。`code` 不是稳定 reqId/eventId 来源，必须以 JSON body 为事实来源。

Manager 内部统一转换为：

```json
{
  "source": "brosdk",
  "code": 0,
  "eventName": "browser-open-success",
  "reqId": 123,
  "envId": "10001",
  "operationId": "uuid",
  "payload": {},
  "receivedAt": "2026-07-25T00:00:00.000Z"
}
```

当前实现使用 host 内部 `request_operations` 映射保存异步调用返回的 reqId。callback 即使早于同步函数返回，也会先进入原始字节队列，等调用返回并建立映射后再归一化，因此不会因早到事件丢失 operation 关联。

如果 SDK 事件没有明确 `envId`，Manager 要用 reqId 与 operation 映射补齐；补不齐时保留为全局事件并进入诊断日志。

## 7. 内存所有权

所有由 SDK 返回的 `char **out_data` 都必须：

1. 立即按 `out_len` 拷贝为 Rust owned bytes。
2. 使用 `sdk_free(ptr)` 释放。
3. 解析 UTF-8 JSON。
4. 对敏感字段脱敏后再写入日志。

禁止用 `CString::from_raw` 释放 SDK 分配的内存。

## 8. 环境 CRUD 映射

首版将 SDK 后端环境作为远端事实，Manager 做本地镜像和增强字段：

| Dashboard 操作 | DLL 调用 | 产品语义 |
| --- | --- | --- |
| 拉取环境 | `sdk_env_page` | 同步远端列表，合并本地标签、备注、最近状态 |
| 查看详情 | `sdk_env_getinfo` | 获取已解密环境详情，用于指纹/代理/内核预览 |
| 新建环境 | `sdk_env_create` | 写入远端后保存本地镜像 |
| 修改环境 | `sdk_env_update` | 远端成功后更新本地镜像 |
| 删除环境 | `sdk_env_destroy` | 运行中禁止删除，先停止再删除 |
| 启动环境 | `sdk_browser_open` | 创建 operation，等待 ready event |
| 停止环境 | `sdk_browser_close` | 创建 operation，等待 close event |

## 9. 扩展参数

v2 文档说明 `browser_open.envs[].extensions[]` 支持：

```json
{
  "id": "abcdefghijklmnopabcdefghijklmnop",
  "name": "my-ext",
  "packType": 0,
  "component": false,
  "data": {
    "key": "value"
  }
}
```

边界：

- `extensions[]` 选择本次启动要加载的扩展并传入 data。
- 扩展本体来自 SDK 扫描的本地扩展目录。
- `data` 是 `map<string,string>`，SDK 启动前写入扩展 LevelDB。
- 空 key/value 不会写入。
- 客户端 UI 应让用户选择扩展目录，并展示 manifest 校验结果。

## 10. 代理和 DNS

SDK 会把浏览器代理参数改为本地 SOCKS5 桥：

```text
--proxy-server=socks5://127.0.0.1:<local_bridge_port>
```

真实链路由 SDK 的 proxy/bridgeProxy 决策决定。默认 `fallback-as-proxy` 下：

- `global=false` 时，`bridgeProxy` 可能被提升为最终出口代理。
- `global=true` 时，通常使用 `proxy`。
- 目标站点 DNS 会被浏览器侧 DNS guard 压住，并交给本地桥或上游代理；第一跳代理如果是域名，仍依赖本机 DNS。

Dashboard 需要展示“路由策略”和“实际泄漏检测”两个层次，不能只根据配置推断 DNS/WebRTC 一定安全。

## 11. 内核管理

DLL 提供：

- `sdk_browser_install`
- `sdk_browser_cleanup`
- `sdk_browser_info`

v2 内核逻辑会在启动时按环境核心信息自动解析、下载、校验和安装。Dashboard 首版应提供：

- 当前运行环境使用的核心版本。
- 内核安装目录/缓存目录。
- 手动安装/更新操作入口。
- 缓存清理。
- 安装失败的阶段、URL 是否缺失、hash 是否失败、解压是否失败。

如果 DLL 无法给出完整 catalog，Dashboard 不应凭空显示“可更新”；只能显示“未知/需刷新/无下载源”。

## 12. CDP 与 MCP

公共 C ABI 直接支持 `sdk_browser_command`、`sdk_browser_snapshot` 和 `sdk_browser_env_check`，这些可以作为首版自动化能力基础。DLL 还自带 MCP endpoint，可通过初始化端口启用，作为 `sdk-host` capability 接入。

MCP 分三步：

1. 首版 `sdk-host capabilities` 明确报告 DLL 内嵌 MCP 可用性。
2. Manager 暴露本地 MCP adapter，通过 SDK C API 执行环境列表、CDP 命令、截图、快照等工具。
3. 后续如复用 DLL 内嵌 MCP endpoint，由 Manager 配置端口和生命周期，而不是让 Dashboard 直接依赖 DLL 内嵌端口。

MCP 工具统一使用 `envId` 参数路由，不要求客户端在请求头携带 generation。Manager 负责把 `envId` 映射到当前 ready 的运行实例。

## 13. 最小 ABI Smoke Test

第一批测试只做低风险调用：

1. 加载 DLL。
2. 检查必需符号存在。
3. 注册 log/result callback。
4. 使用 `BROSDK_API_KEY` 调用 `sdk_get_user_sig`。
5. 调用 `sdk_init`。
6. 调用 `sdk_info`。
7. 调用 `sdk_env_page`。
8. 调用 `sdk_shutdown`。

这批测试不能创建、修改、删除或启动环境，除非设置 `BROSDK_E2E_ALLOW_MUTATION=1`。
