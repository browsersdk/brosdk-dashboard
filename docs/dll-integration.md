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
5. 使用 Manager 注入到子进程环境的 API Key 调用 `sdk_get_user_sig`。
6. 使用返回的 userSig 调用 `sdk_init`。
7. 所有 SDK 调用由 `sdk-host` 单线程串行入口接收，内部可按 SDK 语义等待异步回调。

当前 `sdk-host serve --endpoint <pipe-or-socket>` 已实现上述隔离入口。Manager 不传 API Key 到 IPC：桌面凭据从平台安全存储读取，测试凭据可从父进程环境读取，二者都只通过新建 `sdk-host` 的子进程环境注入。IPC 响应和 callback event 在发送前统一脱敏。

不要让 Dashboard 或 Manager 直接持有 DLL 函数指针。

## 3. 认证与初始化链路

测试链路：

```text
BROSDK_API_KEY -> sdk_get_user_sig -> userSig -> sdk_init -> sdk_info
```

桌面首次初始化链路：

```text
API Key 输入 -> Manager 候选验证 -> sdk-host 子进程环境 -> sdk_get_user_sig(role=user)
              -> sdk_init -> env_page 全量同步 -> 平台安全存储
```

候选验证或同步失败时不保存新凭据；更换/移除凭据时先停止 Host，再清空环境、详情、运行态和旧环境绑定。userSig 始终由 DLL 初始化链路持有，不进入 Dashboard、SQLite 或 Manager IPC。

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

阶段 11 源码审计确认 DLL 的真实 MCP 分层并不缺少单环境操作：全局管理端点 `/sdk/v1/mcp` 已注册 `env.list/resolve/get/create/update/destroy`、`browser.open/close/cleanup/status/install`、`task.list/get` 和 `mcp.endpoint`；单环境自动化端点 `/sdk/v1/mcp/env/{envId}` 已注册 `browser_state`、`tabs`、`snapshot/diff/read/grep/screenshot/pdf/wait` 及导航、交互、上传下载等工具。Manager client 现已接通两个 endpoint，并负责严格 lifecycle、动态工具发现、读写分层、ready 状态校验、operation 追踪、URL 降级和响应脱敏；Dashboard/Agent 不直接持有 MCP session。

Manager 的全局只读策略允许 `sdk.health`、`sdk.info`、`env.list`、`env.resolve`、`env.get`、`browser.status`、`task.list`、`task.get`、`mcp.endpoint`。单环境策略允许 `browser_state`、`tabs`、`snapshot`、`diff`、`read`、`grep`、`screenshot`，并限制 action、页号、文本/节点上限和截图尺寸；其余工具即使被 DLL 广告，也不会被透传。2026-07-26 真实 DLL smoke 使用协议 `2025-11-25` 发现 16 个全局工具、允许 9 个，并完成三个全局只读调用。

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
| 拉取环境 | `sdk_env_page` | 完整分页缓存远端列表，叠加当前设备最近运行状态 |
| 查看详情 | `sdk_env_getinfo` | 获取已解密环境详情，用于指纹/代理/内核预览 |
| 新建环境 | `sdk_env_create` | 写入远端后保存本地镜像 |
| 修改环境 | `sdk_env_update` | 远端成功后更新本地镜像 |
| 删除环境 | `sdk_env_destroy` | 运行中禁止删除，先停止再删除 |
| 启动环境 | `sdk_browser_open` | 创建 operation，等待 ready event |
| 停止环境 | `sdk_browser_close` | 创建 operation，等待 close event |

创建/更新参数与 `browser-open-server` 的第三方接口保持一致：`sdk_env_create` 使用 userSig 访问 `/sdk/v1/env/create`，服务端 `/api/v2/browser/create` 使用 API Key，但两者最终复用 `BrowserApi.Create` 和 `dto.FingerReqDto`。Dashboard 普通创建只向 Manager 提交 `proxyProfileId? + kernelId`，Manager 解析为：

```json
{
  "kernel": "Chrome",
  "kernelVersion": "134",
  "proxy": "socks5://user:password@host:port"
}
```

未选择代理时省略 `proxy`。`customerId`、`envName` 和完整 `finger` 不发送；服务端从 userSig 上下文识别调用方，并由 `FingerReqDto.Valid()` 补齐默认指纹。代理 URL 只存在于 Manager 到 Runtime Host 的瞬时请求，Host 返回值和持久化入口都会脱敏。

`sdk_env_create` 的 C 返回码只能证明传输层调用完成。服务端业务失败仍可能随 JSON 返回，因此 Manager 必须额外校验 `code=200`，并在成功时提取 `data.envId`。

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

Dashboard 的页面诊断使用 `sdk_browser_snapshot` 的最小模式：`includeHtml=false`、`includeScreenshot=false`、`emitEvents=false`，最多读取 32 个 page target。原响应仍包含 target/session/snapshot ID、标题和完整 URL，因此 Manager 必须只保留 page status、数量和 origin；不能把原响应交给前端或 operation/event。

`sdk_browser_env_check` 会创建新标签并返回 target/session/CDP 注入细节。Manager 只向 Dashboard 返回 `{opened,newTab,source}`，真实生命周期 E2E 通过检查前后的安全快照页数增加来确认内置指纹页确实出现。

`sdk_browser_cleanup` 的 `envs` 与 `cores` 语义必须分开：`envs` 删除非运行环境的本机 user-data-dir，`cores` 删除内核下载缓存；两者都不删除服务端环境。服务端删除只能调用 `sdk_env_destroy`。环境正在运行或启停中时，DLL 会以 busy 拒绝本地清理，Manager 还会在调用前用环境状态做第一层门禁。

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
