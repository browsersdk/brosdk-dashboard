# 测试与发布验证

本文件前半部分描述当前测试入口和安全约束；第 11 节起保留历史阶段验收门槛。当前产品状态与发布优先级以 [当前状态](status.md) 和 [正式发布路线](roadmap.md) 为准。

## 1. 测试凭据

本项目测试需要 BroSDK API Key。不要把真实 key 写进任何源码、文档、提交信息、日志或截图。

PowerShell 设置方式：

```powershell
$env:BROSDK_API_KEY = "<api-key>"
```

首次初始化安全存储 E2E 使用独立变量并通过隐藏输入提示获取，不把密钥放进命令参数：

```powershell
npm run e2e:credential
```

该 runner 使用唯一系统临时数据目录，验证初始化、DPAPI 密文、Manager 重建恢复和移除后的账号缓存隔离，退出时清理临时目录。普通桌面使用 `manager_configure_api_key` 写入平台安全存储；`BROSDK_API_KEY` 仅保留给测试和受管部署。

真实 Dashboard 环境启停 E2E：

```powershell
npm run e2e:dashboard:desktop
```

该 runner 驱动 Windows Tauri 窗口中的设置与环境控件；初始化后先在全部环境 stopped 时执行一次 SDK 自检并要求 `sdkSelfCheckObserved=true`，再启动目标环境。它必须等待目标环境所在表格行明确显示“运行中”，不能把启动中已经可用的停止按钮误判为 ready；随后新建绑定目标 envId 的单环境 AI 会话，验证 CDP 地址或 DLL 内部控制通道、AI Provider 设置入口，再新建全局 Chat 验证 Enter 发送。测试结束前会把目标环境恢复为 stopped，隔离启动时还会关闭进程并清理临时数据目录。可用 `-AgentLifecycle -TargetEnvironmentId <envId>` 改为从单环境 Agent 生成计划和批准启动；`npm run e2e:ai-assistant:desktop` 会复制正式数据目录中的 DPAPI 密文到临时目录，通过真实 Tauri 界面和真实 AI Provider 自动询问 stopped 环境是否启动，并断言回复不声称 ready、环境未被写操作改变。

可选测试变量：

```powershell
$env:BROSDK_E2E_ENV_ID = "<existing-env-id>"
$env:BROSDK_E2E_ALLOW_MUTATION = "0"
$env:BROSDK_E2E_USE_ONLY_ENV = "0"
$env:BROSDK_E2E_MANUAL_CLOSE_TIMEOUT_SECS = "0"
$env:BROSDK_E2E_SIMULATE_MANUAL_CLOSE = "0"
$env:BROSDK_WORK_DIR = Join-Path $PWD "runtime\sdk-work"
$env:BROSDK_EMBEDDED_PORT = "17891"
```

默认测试不得创建、修改或删除远端环境。需要做破坏性/写入测试时，必须显式设置：

```powershell
$env:BROSDK_E2E_ALLOW_MUTATION = "1"
```

阶段 13 的双环境自动验收使用独立 wrapper，由隐藏提示读取 API Key、自动设置 mutation 门禁，并在唯一临时 Manager 数据目录中运行：

```powershell
npm run e2e:multi-environment
```

该 runner 创建两个临时服务端环境并取得所有权，完成元数据更新、独立批量启停、两份远端指纹详情刷新、本地数据清理、服务端删除和最终全量对账。异常路径会尝试停止并删除所有已创建环境；报告只包含阶段、数量和布尔值。

## 2. 首批测试矩阵

| 测试 | 默认运行 | 说明 |
| --- | --- | --- |
| DLL load | 是 | 加载 `libs/windows_x64/brosdk.dll` 并检查符号 |
| get userSig | 是 | 使用 `BROSDK_API_KEY` 调 `sdk_get_user_sig` |
| sdk init/info | 是 | 独立 workDir 初始化并读取 SDK 信息 |
| env page | 是 | 读取环境列表，不修改数据 |
| embedded MCP capability | 是 | `sdk-host capabilities` 报告 DLL 内嵌 MCP；端口测试需设置 `BROSDK_EMBEDDED_PORT` |
| browser open/close | 否 | 需要 `BROSDK_E2E_ENV_ID` |
| env create/update/destroy | 否 | 需要 `BROSDK_E2E_ALLOW_MUTATION=1` |
| kernel install/update/remove | 否 | 需要隔离目录和明确下载源 |

## 3. Windows Smoke 预期流程

```text
load dll
  -> register callbacks
  -> sdk_get_user_sig
  -> sdk_init
  -> sdk_info
  -> sdk_env_page
  -> sdk_shutdown
```

失败时需要记录：

- 阶段；
- 稳定错误码；
- SDK 返回码；
- 脱敏 message；
- 是否有 sdk-host 退出码；
- 是否有 result/log callback。

Runtime Host 隔离 smoke：

```powershell
npm run sdk:runtime-smoke
```

该命令先构建 `sdk-host`，再验证 health/capability、正常 shutdown 和强制 kill。预期正常路径状态为 `stopped`，强制 kill 路径状态为 `degraded`，完成后没有残留 `sdk-host.exe`。可设置 `BROSDK_IPC_TRACE=1` 输出不含 payload 的 IPC 阶段诊断。

Dashboard 的同类入口位于“设置 -> 安全与诊断”。只要任一环境为 ready、starting、stopping、failed 或 unknown，按钮必须禁用；可执行时 Manager 使用安全存储中的 API Key，先停止长期 Runtime Host，再运行一次性 smoke，最后重启 Runtime Host。它不应出现在总览快捷操作中，也不能在环境运行时被当作普通健康检查。

Manager Domain smoke：

```powershell
npm run manager:smoke
```

该命令验证 SQLite 初始化、runtime host 启停、持久化 operation、snapshot、`events_since`、内核刷新和 DLL 全局 MCP。未设置 `BROSDK_API_KEY` 时，同步 operation 预期以 `SDK_HOST_ERROR` 失败并产生 queued/running/failed 事件；设置密钥时预期执行真实 `sdk_env_page` 并更新环境镜像，同时 `kernelRefresh.serverKernelListLoaded=true` 且 `kernelRefresh.count > 1` 表示 `/api/v2/browser/kernelList` 已进入内核页数据源。额外设置 `BROSDK_EMBEDDED_PORT` 时，还会发现全局工具并调用 `sdk.health`、`env.list`；缓存存在环境时再调用 `mcp.endpoint`。报告只输出协议、数量和成功布尔值，不输出 API Key。隔离测试数据应同时设置 `BROSDK_DATA_DIR`，不要使用不存在的数据库文件覆盖变量。

## 4. 生命周期 E2E 预期流程

前置条件：

- 已设置 `BROSDK_API_KEY`。
- 已设置 `BROSDK_E2E_ENV_ID`。
- 该环境归测试账号所有。

流程：

```text
sdk_init
  -> sdk_env_getinfo(envId)
  -> sdk_browser_open({ envs: [{ envId }] })
  -> wait browser-open-success
  -> sdk_browser_info contains envId
  -> sdk_browser_command Runtime.evaluate
  -> sdk_browser_snapshot safe baseline
  -> sdk_browser_env_check creates built-in check tab
  -> sdk_browser_snapshot page count increases
  -> sdk_browser_close({ envs: [envId] })
  -> wait browser-close-success
  -> sdk_browser_info no longer contains envId
  -> sdk_shutdown
```

自动 runner：

```powershell
npm run e2e:environment
```

runner 在缺少 `BROSDK_API_KEY` 时使用隐藏输入，使用唯一临时 Manager 数据目录，并在未指定端口时自动分配 DLL MCP 端口。默认可设置 `BROSDK_E2E_ENV_ID`；如未设置且账号只有一个环境，wrapper 自动启用 `BROSDK_E2E_USE_ONLY_ENV=1`。runner 会先调用 `sdk_browser_info`，若该环境已经运行则拒绝接管或停止。设置 `BROSDK_E2E_SIMULATE_MANUAL_CLOSE=1` 时，runner 通过 CDP `Browser.close` 模拟用户关闭整个浏览器并验证 Manager 对账；需要真人关闭窗口时，改用 `BROSDK_E2E_MANUAL_CLOSE_TIMEOUT_SECS` 正整数。

ABI 注意事项：

- `sdk_browser_open` / `sdk_browser_close` 的同步非负返回值表示 accepted，不是 callback JSON 中的 `reqId`。
- lifecycle operation 必须通过 callback 的 `envId + type` 绑定，再记录真实 `reqId`。
- `sdk_browser_info` 会包含 Starting 条目；只有 callback success 或明确 CDP endpoint 才算 ready。
- 页面级 `Runtime.evaluate` 先执行 `Target.getTargets` 和 `Target.attachToTarget` 获取 `sessionId`，完成后执行 `Target.detachFromTarget`。

2026-07-25 真实验证：测试账号完成环境同步、callback ready、页面 session `Runtime.evaluate`、SDK close、CDP `Browser.close` 模拟手动关闭对账，以及配置端口下 DLL 内嵌 MCP 的 Streamable HTTP lifecycle 和 `tabs(list)` 调用。输出只记录布尔结果和环境数量，不记录 API Key、userSig、envId、页面 URL 或 MCP 正文。

验收：

- open accepted 时 UI/operation 状态为 `starting`。
- ready 只在 `browser-open-success` 或对账成功后出现。
- 手动关闭浏览器后，Dashboard 在下一次 callback/info 对账后更新为 `stopped` 或 `failed`。
- close accepted 时 UI/operation 状态为 `stopping`。
- 测试结束无残留浏览器进程和 `sdk-host` 进程。

Dashboard MVP 自动验收补充：桌面 1280px 与移动 390px 都要覆盖环境页搜索、状态筛选、详情开关和导航。移动端导航固定在底部，页面本身不得横向溢出；环境表允许在自己的滚动容器中横向滚动。

## 5. 回调测试

必须覆盖：

- result callback 早于同步返回；
- result callback 晚到；
- SDK 返回 accepted 后失败；
- SDK 无 callback 但 `sdk_browser_info` 对账成功；
- SDK 无 callback 且对账超时；
- callback JSON 缺 envId，只能通过 reqId 映射；
- callback JSON 解析失败。

## 6. 进程锁测试

根据 v2 文档，SDK 在 init 后基于 appId 做 OS 实例锁。测试方式：

1. 启动第一个 `sdk-host` 并完成 `sdk_init`。
2. 启动第二个 `sdk-host` 使用相同 API Key 初始化。
3. 观察第二个 host 的退出或错误。
4. 验证 Manager 和 Dashboard 仍然存活，并显示明确错误。

验收：

- 第二实例冲突不会关闭桌面主窗口。
- 错误信息不包含 API Key/userSig。
- 第一个 host 正常 `sdk_shutdown` 后锁释放。

## 7. 扩展测试

需要准备一个包含 `manifest.json` 的扩展目录。

测试点：

- Dashboard 使用系统目录选择器添加扩展目录。
- Manager 校验目录存在、不是符号链接、包含 manifest。
- `browser_open.envs[].extensions[]` 能传入目标扩展 id 和 data。
- 浏览器启动后扩展能读到预写 data。

## 8. 代理测试

测试点：

- 支持粘贴 `http://`、`https://`、`socks5://`、`socks5h://` URL 并解析。
- 凭据不写入日志和操作记录。
- SDK 网络诊断可以区分代理连通失败、DNS 慢、目标 TLS 失败。
- Dashboard 不把“配置了代理”直接等同于“无 DNS/WebRTC 泄漏”。

## 9. 日志与脱敏

日志中必须屏蔽：

- API Key；
- userSig；
- Authorization；
- Cookie；
- CDK/DEK；
- 代理密码；
- URL query 中的 token/password/key；
- 指纹 seed。

测试后执行 secret scan，至少检查当前仓库和运行日志目录。

## 10. 维护基线

阶段 9 AI 验收补充：

- `cargo test -p manager agent_execution_survives_reopen` 验证幂等执行记录跨重启保留。
- Chat 上下文只包含脱敏摘要，不包含数据目录、workDir、logDir、完整代理 URL、CDP endpoint、API Key 或 userSig。
- 同一 `idempotencyKey` 只能对应一个序列化计划；不同计划复用必须返回 `INVALID_AGENT_PLAN`。
- Agent 在调用工具前写入 reservation；中断或结果未落盘时相同 key 必须返回 `AGENT_EXECUTION_UNCERTAIN`，不能重复执行。
- `npm run ai:smoke` 同时验证普通只读回答和一次原生 function tool round；只记录模型、只读标志、工具调用布尔值和回答长度。
- MCP 单元测试必须验证 `initialize -> initialized notification -> tools/list -> tools/call -> DELETE` 和 session/version headers。
- 设置 `BROSDK_EMBEDDED_PORT` 的环境 E2E 必须完成一次 Manager 路由的 `tabs(list)`，输出只记录 `embeddedMcpToolVerified=true`，不记录 envId、页面 URL 或工具正文。

2026-07-25 阶段 9 最终验证结果：

- `npm run ai:smoke` 通过，仅输出模型、只读标志和回答长度。
- `npm run sdk:smoke` 通过，确认 `getUserSig` 请求使用 `role=user`，并完成 init/info/env_page/shutdown。
- `npm run sdk:runtime-smoke` 和带真实账号的 `npm run manager:smoke` 通过，host 优雅停止和异常退出降级均符合预期。
- 带 `BROSDK_EMBEDDED_PORT`、唯一环境选择和模拟手动关闭的 `npm run e2e:environment` 通过，结果包含 `runtimeEvaluateVerified=true`、`embeddedMcpReachable=true`、`embeddedMcpToolVerified=true`、`manualCloseVerified=true`。
- `npm run check`、`npm test`、`npm run build` 通过；Dashboard 在 1440x900 和 390x844 下完成 AI/MCP 导航、Chat/Agent 切换、禁用态、控制台和横向溢出检查。

维护者应先阅读 [文档中心](README.md)，再运行 `git status --short --branch`、`npm run security:tauri`、`npm run check`、`npm test` 和 `npm run build` 建立基线。产品 API Key 使用平台安全存储，测试 Key 只从进程环境读取；两者都不写入仓库、SQLite、日志或截图。

涉及 DLL 生命周期或 MCP 的改动必须继续通过隔离 host、Manager operation 和脱敏边界，不允许 Dashboard 直接调用 DLL/MCP/CDP。

涉及桌面 WebView 或 Tauri 配置的改动必须继续通过 `npm run security:tauri`。该检查要求生产 CSP 不为 `null`，包含本地资源、Tauri IPC、`object-src 'none'`、`form-action 'none'` 和 `frame-ancestors 'none'`，并禁止生产策略使用 `*`、远程网络源或 `unsafe-eval`。

涉及 AI 会话的改动必须验证默认不持久化：首次进入 AI 页不应恢复旧 `brosdk-dashboard.ai-conversations.v1`，发送新消息后也不写入本地存储；只有用户显式勾选“保存历史”后，才允许把最多 20 个会话、每个 80 条消息写入 WebView localStorage。Playwright 若需要预置长会话，必须同时设置 `brosdk-dashboard.ai-conversations.persistence.v1=enabled`。

> 以下内容保留已实现能力的历史回归门槛，不代表当前路线编号。新增工作应以 [正式发布路线](roadmap.md) 为准。

## 11. 阶段 10 环境创建验收

- 单元测试验证最小输入只包含 `proxyProfileId` 和 `kernelId`，Manager 构造的 SDK 请求只包含顶层 `kernel`、`kernelVersion` 与可选 `proxy`；`customerId`、`envName` 和 `finger` 必须省略。
- 代理测试使用受保护 secret round trip；断言 operation request、事件和错误文本均不包含代理密码或完整代理 URL。
- 无代理创建必须覆盖，确认请求省略 `proxy` 而不是发送伪造占位值。
- 不存在的 proxy profile、未知内核、缺少 major version 和非当前平台内核都必须在调用 DLL 前失败。
- Dashboard 自动测试覆盖打开创建面板、选择代理、选择内核、禁用态和取消，不在普通浏览器 mock 环境执行真实 mutation。
- 真实创建 E2E 内部设置 `BROSDK_E2E_ALLOW_MUTATION=1`，测试创建成功后清理该临时环境的本地数据、删除服务端环境并再次 `env_page` 对账。
- 真实创建 E2E 命令为 `npm run e2e:environment-create`；未设置 `BROSDK_API_KEY` 时包装脚本用隐藏提示读取，使用唯一临时 Manager 数据目录，并在退出时清理凭据环境变量和目录。不能在命令或仓库文件中硬编码 API Key。

2026-07-26 Manager 子阶段结果：28 个 Manager 测试、6 个 sdk-ffi 测试、4 个 sdk-host 测试及 environment-e2e 辅助测试通过；workspace all-targets 编译通过。新增覆盖包括 `role=user`、最小 DTO、无代理省略、不可用内核拒绝、业务码检查、错误脱敏、字符串/数字 envId 和本地删除清理。

Dashboard 子阶段结果：5 个环境创建组件测试通过；production build 通过。Browser 插件实测打开创建带、切换代理、按钮取消和 Esc 取消；1440x900、390x844 下 `documentElement.scrollWidth === clientWidth`，无控制台 warning/error。普通浏览器预览仅验证交互，提交按钮保持禁用，不触发远端 mutation。

真实 E2E 子阶段结果：门禁辅助测试 2 项通过。真实 DLL 使用 `chrome-134-windows-x86_64` 和本机网络完成创建、镜像确认、本地数据清理、服务端删除和 `env_page` 再对账；`localDataCleanupSucceeded/cleanupAttempted/cleanupSucceeded=true`，测试前后账号环境数均为 1。

## 12. 阶段 11 远端缓存与 MCP 验收

- 环境分页测试至少覆盖两页、空列表、重复 envId、`total` 缺失、第二页失败和条数上限。
- 成功同步必须删除缓存中服务端已不存在的环境；失败同步不得写入部分结果，cache status 必须为 stale。
- schema migration 必须清除旧 `local_label`/`tags_json` 覆盖，Dashboard 搜索和展示只使用服务端名称与 envId。
- 全局 MCP 测试覆盖 `/sdk/v1/mcp` 的 initialize、initialized、tools/list、只读 tools/call、DELETE，并确认 mutation 工具不会被 Dashboard 直通。
- 单环境 MCP 测试覆盖全局 endpoint 的 `env.* + arguments.envId` 路由、advertised tools 与可调用目录一致；非 ready 环境、管理 mutation、未公布工具、非 object 或超限参数必须拒绝。
- 真实测试至少验证全局 `sdk.health`、`env.list`、`mcp.endpoint`，以及 ready 环境的 `tabs(list)` 和一个页面读取工具；报告不输出 envId、页面正文、URL query、API Key 或 userSig。

2026-07-26 远端缓存子阶段结果：Manager 35 个测试通过，覆盖多页合并、重复 envId、空列表、缺少 total、总数变化、重复页无进展、条数上限、原子替换、远端删除、双重脱敏、失败保留和 v5 迁移。`npm run check`、`npm test`、`npm run build` 均通过；真实 Manager smoke 在独立临时数据目录完成首次自动刷新和显式刷新，operation 为 succeeded、缓存为 `sdk-server/fresh/1`，runtime 正常停止，临时数据库已清理。环境页在 1440x900 与 390x844 下无重叠、无页面级横向溢出，控制台无应用 warning/error。MCP 全局/单环境验收留给下一子阶段。

2026-07-26 Manager MCP 子阶段结果：MCP client 4 个测试和 Manager 40 个测试（含二进制测试）通过，覆盖全局/环境 endpoint、session lifecycle、工具元数据解析、参数归一化、mutation 拒绝和未激活端口。真实隔离 smoke 通过 `getUserSig(role=user)` 初始化 DLL，发现 16 个全局工具、Manager 放行 9 个，协议为 `2025-11-25`；`sdk.health`、`env.list`、`mcp.endpoint` 均成功，未输出环境 ID 或工具正文，runtime 正常停止，临时数据库和端口已清理。单环境扩展读取与 Dashboard 视觉验收留给下一子阶段。

2026-07-26 Dashboard/单环境 MCP 子阶段结果：新增 5 个 MCP 组件测试，Dashboard 共 10 个测试通过；真实生命周期 E2E 发现 18 个单环境工具、Manager 放行 7 个，并完成 `tabs(list)`、从脱敏响应提取 page id、`read(page)` 和 SDK close，报告不包含 envId、页面正文或 URL。Browser 插件完成 DOM/交互/console 检查；因当前截图 API 不可用，截图和几何测量使用本机 Chrome Playwright，1440x900 与 390x844 均无 framework overlay、应用 warning/error、页面级或 MCP 控件横向溢出。测试数据目录和 MCP 端口均已清理。

## 13. 阶段 12 首次初始化验收

- `npm run e2e:credential` 必须验证合法 API Key 完成 `getUserSig(role=user) -> init -> env_page`，并且报告不输出 API Key、userSig、路径或后端正文。
- 加密文件不得包含 API Key 明文字节；Manager 重建后必须从安全存储恢复并完成初始化。
- 移除凭据后 Host 为 stopped，环境、详情、运行态、operation 和旧环境绑定为空；本地代理配置和内核记录可保留。
- 环境变量来源显示为“系统环境”，Dashboard 禁止更换和移除，避免写入一个重启后不会生效的覆盖值。

2026-07-26 首次初始化子阶段结果：`npm run check`、workspace 63 个 Rust 测试、Dashboard 13 个组件测试和 production build 通过。真实凭据 E2E 同步 1 个环境，`encryptedAtRest=true`、`restartLoaded=true`、`accountStateCleared=true`。Browser 插件完成密码显示切换、预览禁用态和 console 检查；CDP 设备视口复核 390x844 时页面宽度为 390/390、初始化面板边界为 24..366，1440x900 截图也无重叠。

## 14. 阶段 12 环境与远端指纹验收

- `manager_refresh_environment_detail` 必须只请求指定 `envId`，要求环境已在服务端镜像中，并把 operation/event 绑定该环境。
- `sdk_env_getinfo` 必须校验业务 `code=200`；缓存只能包含递归脱敏 Finger、掩码代理、Browser 内核和允许的元数据，不得出现 Cookie、Storage、上传路径、DEK、token、secret 或代理密码。
- 指纹页的数据源是 `environmentBindings.remoteFingerprint`，环境详情同时显示 remote kernel/proxy/metadata；本地指纹 JSON CRUD 不再出现在普通主流程。
- 浏览器预览使用 `http://127.0.0.1:1420/?preview=workspace&page=fingerprints` 或 `page=environments`，只用于布局/交互测试，所有本机动作仍必须 disabled。

2026-07-26 环境/指纹子阶段结果：真实凭据 E2E 同步 1 个环境并完成聚焦 `sdk_env_getinfo`，`focusedDetailLoaded=true`，同时保留 `encryptedAtRest/restartLoaded/accountStateCleared=true`。Dashboard 16 个组件测试和 workspace 65 个 Rust 测试通过；`npm run check`、Clippy 和 production build 通过。Browser 插件确认环境详情与结构化指纹 DOM、禁用态和 console 无错误；截图 API 不可用后使用 Chrome CDP 复核 1440x900 与精确 390x844 设备视口，页面宽度 390/390、环境详情右边界在视口内，移动 9 项主导航无内部溢出。

## 15. 阶段 12 环境运维动作验收

- stopped 环境才能清理本地数据或删除服务端环境；ready 环境才能做页面诊断和打开指纹检查页。Dashboard 对清理与删除分别显示行内二次确认。
- 本地清理调用 `sdk_browser_cleanup({envs:[envId]})`，返回前必须移除 userDataDir、cleanupPath 和逐项 envId；服务端删除调用 `sdk_env_destroy`，两者不能互相替代。
- 页面诊断请求固定关闭 HTML、截图和 emitEvents，限制最多 32 页；返回只允许 status、pageCount、failedPages、页面 status 与 origin。
- 单元测试必须使用带 path/query/token、标题、target/session/snapshot ID 和 chunk 的伪响应，证明所有正文和标识都被丢弃。

2026-07-26 运维动作子阶段结果：新增环境详情 3 个组件测试与 Manager 2 个响应摘要测试；Dashboard 19 个组件测试与 workspace 67 个 Rust 测试全部通过。真实临时环境 E2E 完成 create -> local cleanup -> destroy -> env_page，对账后账号环境数恢复到 1，报告不含环境 ID、本地路径或 DLL 原响应。真实 ready 页面诊断留给最终生命周期 E2E。

2026-07-26 最终生命周期结果：真实唯一环境完成 callback ready、CDP evaluate、内置指纹检查页新标签、字段白名单页面诊断、DLL MCP `tabs/read` 和 SDK close；检查页后安全诊断为 3 页，`fingerprintCheckOpened/pageDiagnosticVerified/environmentStopped=true`，MCP 广告 18 个/Manager 放行 7 个。Dashboard 19 项、Rust workspace 69 项测试、Clippy 和 production build 通过；runner 清理临时 Manager 数据目录且无残留 `sdk-host`。报告不含 envId、页面 URL/正文、API Key 或 userSig。

## 16. 阶段 13 多环境验收

- 批量启动/停止测试至少覆盖空列表、重复 envId、超过 20 个、状态不允许、全部 accepted 和单项 SDK 失败；每个环境必须产生独立 operation/generation。
- 元数据更新测试只允许 envName/serial，覆盖 Unicode 字符计数、serial 字节长度、运行态拒绝、后端业务码失败和同步后的服务端名称。
- 指纹对比组件覆盖 2-4 个环境、相同/不同/未知字段、选择上限和详情缓存缺失。
- 真实 E2E 必须使用两个临时环境，最终停止、清理并删除；报告只记录数量和布尔结果，不输出 envId、名称、序号、页面内容或凭据。

2026-07-26 批量生命周期子阶段结果：新增 3 个批量栏组件测试和 2 个 Manager preflight 测试；Dashboard 22 项、Rust workspace 71 项测试、Clippy 与 production build 通过。应用内浏览器验证多选后显示 `1 可启动/0 可停止`，预览态 mutation 禁用、清除选择可用且 console 无错误。真实双环境启停留给阶段 13 最终 E2E。

2026-07-26 远端元数据子阶段结果：`sdk_env_update` 已接入 FFI、Host、Manager、Tauri 和环境详情侧栏；普通请求只含 envId/envName/serial，且只允许 stopped 环境。Dashboard 25 项、Manager library 48 项、创建 E2E binary 3 项测试、Clippy 和 production build 通过。真实临时环境完成 create -> update -> server confirmation -> page/detail mirror -> local cleanup -> destroy -> env_page，`metadataUpdateSucceeded/metadataMirrored/cleanupSucceeded=true`，最终环境数恢复到 1。实测当前 `getEnvInfo` 对刚更新的 serial 返回空字符串；Manager 只在详情值为空时使用服务端分页或已核对回显的更新响应补全，不接受 UI 乐观写入。桌面与 390x844 应用内浏览器 DOM 完整、mutation 预览态禁用且 console 无 warning/error；当前浏览器后端未提供 screenshot 方法。

2026-07-26 指纹对比子阶段结果：详情/对比模式使用同一份服务端脱敏绑定数据，对比最多 4 个环境，只列固定白名单字段并逐行标记相同/不同/未知；缺少详情不会被推断为相同。新增 3 个组件测试后 Dashboard 28 项测试与 production build 通过，覆盖两环境比较、选择上限和所选详情刷新。应用内浏览器用两个预览环境验证桌面及 390x844 DOM，字段分组和状态完整、预览 mutation 禁用、console 无 warning/error；对比表由局部滚动容器承载，浏览器后端未提供 screenshot/element geometry 方法。真实双环境创建、启停和清理已在阶段 13 最终 E2E 完成。

2026-07-26 双环境最终 E2E 结果：新增 `npm run e2e:multi-environment`，使用隐藏 API Key 和唯一临时 Manager 数据目录创建两个环境。两个环境分别完成元数据服务端确认，批量启动和停止各产生两个独立 operation，均到达 callback ready、刷新非空脱敏指纹详情、停止、本地清理和服务端删除；最终 `env_page` 对账前后环境数均为 1，清理为 2/2。报告没有 envId、名称、序号、页面内容或凭据，退出后无残留 `sdk-host` 和临时目录。Dashboard 28 项、Rust workspace 80 项测试、Clippy 与 production build 全部通过，阶段 13 完成。

## 17. 阶段 14 Dashboard envId 与浏览器 E2E

- `envId` 是环境表及详情缓存的唯一主键；名称允许重复。Dashboard snapshot 身份守卫拒绝空/重复环境 envId、重复详情 envId 和悬空详情绑定。
- 普通环境标签统一显示“名称 · envId”；aria 操作名使用“动作 名称 (envId)”。测试和自动化不能再只按名称定位环境。
- 可重复浏览器 E2E 命令为 `npm run e2e:dashboard`。它使用系统 Chrome，自动启动严格独占的 `127.0.0.1:1430` Vite 服务，并在结束时关闭服务。
- E2E 使用 `?preview=workspace&scenario=duplicate-names` 的脱敏同名环境场景，不调用 Tauri 或 DLL mutation；测试 1440x900 和 390x844，覆盖环境表、指纹、MCP 与代理绑定。
- Playwright 失败产物写入已忽略的 `target/playwright/dashboard`。成功运行不会把 API Key、userSig、真实账号 envId 或页面正文写入仓库。

2026-07-26 Dashboard E2E 子阶段结果：组件测试 33 项、Playwright 6 项全部通过；production build 通过。应用内浏览器完成页面身份、非空 DOM、同名 envId 交互和 console 检查；当前后端没有 screenshot 方法，桌面与移动视觉/溢出由 Playwright 两个项目验证。

2026-07-26 阶段 14 最终结果：真实双环境 runner 显式断言两个临时 envId 非空且唯一，报告 `uniqueEnvironmentIds=true` 且不输出真实标识。两个环境再次完成独立更新、批量启停、callback ready、远端指纹详情、本地清理和服务端删除；环境数从 1 恢复为 1，清理 2/2。最终 Dashboard 33 项组件测试、Playwright 6 项、Rust workspace 81 项测试、`npm run check`、Clippy 和 production build 全部通过，退出后无临时数据目录、测试端口或 `sdk-host` 残留。

2026-07-26 首次初始化交互回归：纯浏览器根页面不再展示无法调用 Manager 的禁用 API Key 表单，改为可点击的工作台预览入口；真实 API Key 初始化仍只在 Tauri 桌面运行时提供。Windows 桌面窗口已验证输入后按钮启用，并使用隐藏测试凭据完成初始化、加载环境工作台和启动隔离 `sdk-host`。Playwright 首次启动回归在桌面与 390px 项目各执行一次，Dashboard E2E 共 8 项。

2026-07-26 Dashboard 功能 E2E 补充：`npm run e2e:dashboard` 只验证无后端浏览器预览的布局、身份和选择交互，不能作为 SDK mutation 验收。新增 `npm run e2e:dashboard:desktop`，在 Windows 可访问性树中驱动真实 Tauri 环境表，验证启动按钮可用、点击后出现可用停止按钮、再停止并恢复启动按钮；可复用已初始化窗口，干净环境下会启动前端/桌面进程并通过隐藏 API Key 完成首次初始化。报告不输出 API Key、envId、名称、CDP 或页面内容。

2026-07-26 指纹展示回归：指纹详情只展示服务端 DTO 中有明确用户语义的浏览器、系统、设备和主要指纹表面字段。Canvas、WebGL、WebRTC、AudioContext、字体等枚举转换为可读模式；对象或 JSON 编码对象只显示“已配置”。未知字段、“其它”分组、MAC、WebRTC IP、字体列表和 perturb 等内部参数不进入 DOM、title 或对比单元格。

## 18. 阶段 15 操作中心与故障恢复验收

- 操作中心使用 `envId` 筛选和绑定环境，同名环境的 option、表格单元格、详情和测试属性必须保留不同 envId。
- 用户取消只允许 queued operation。Manager 与 Store 都必须拒绝 running -> cancelled，防止 SDK/DLL 请求继续执行时界面误报取消。
- 重试入口只对应 Manager `retry_operation` 已实现的类型；未知类型、详情刷新、MCP、诊断和清理等失败 operation 不显示重试。
- 浏览器预览只验证筛选、详情和禁用态；真实 mutation 继续由 `npm run e2e:dashboard:desktop` 执行。

2026-07-26 阶段 15 结果：新增 3 个操作中心组件测试和 1 个 Manager 状态策略测试。Browser 插件确认同名环境按 envId 过滤为 2/4、详情显示精确环境身份且 console 无 warning/error；截图能力仍不可用。`npm run e2e:dashboard` 在桌面与 390x844 共 10 项通过；真实 Tauri 启停 E2E 返回 `operationIdentityObserved=true`，不输出真实 envId、名称或凭据。最终 Dashboard 36 项、Rust workspace 82 项、Playwright 10 项测试，以及 `npm run check`、Clippy 和 production build 全部通过。

## 19. 阶段 16 AI 配置与环境上下文验收

- Provider 配置覆盖默认值、settings、安全存储和 `BROSDK_AI_*` 环境变量来源；环境变量来源必须只读显示。
- AI API Key 保存后不回显，SQLite、事件和受保护 secret 文件均不得包含明文字节；清除只影响安全存储来源。
- AI 环境选择器必须显示“名称 · envId”，Chat/Agent 请求携带所选 envId。模型上下文只允许脱敏 CDP origin 或 `sdk-browser-command` 控制通道。
- 本地 UI 对外部 CDP 地址提供复制；`remoteDebuggingPort=0` 的 ready 环境显示“未暴露 TCP 地址 / DLL 内部 CDP / MCP”，不能显示 `ready`、`-` 或伪造 URL。
- `npm run e2e:dashboard:desktop` 必须完成 start -> 明确运行中 -> AI 环境信息 -> Provider 设置 -> stop -> 操作中心，并把目标环境恢复为 stopped。

2026-07-26 阶段 16 结果：Dashboard 43 项、Rust workspace 86 项、Playwright 桌面/移动 12 项通过；`npm run check`、Clippy、production build 和真实 Tauri UI E2E 全部通过。真实环境使用 DLL 内部 CDP pipe，桌面报告 `aiEnvironmentContextObserved/aiProviderSettingsObserved=true`，不输出 API Key、真实 envId 或 CDP 内容。

## 20. 阶段 17 CDP 多源回填验收

- sdk-host callback 归一化必须保留 `data.remoteDebuggingPort`，但继续脱敏 Authorization、token、Cookie 和凭据。
- callback、`sdk_env_getinfo`、`sdk_browser_info` 共用同一 endpoint 解析规则；非零数值/数字字符串与 JSON 编码子对象可用，普通代理端口、`fpBlockPort` 和端口扫描白名单不可用。
- 详情刷新只允许给 ready 环境补充 CDP，不能改变 generation、reqId、current operation 或 `browser-open-success` last event。
- `npm run e2e:dashboard:desktop` 报告 `cdpEndpointObserved`：真实地址为 true，内部 pipe/fallback 为 false；两种情况都必须完成 start -> ready -> AI -> stop 并恢复 stopped。

2026-07-26 阶段 17 结果：直接 C API 在运行中验证仓库 DLL 2.0.0.8，success callback 与 BrowserInfo 的 `remoteDebuggingPort` 均为 0，getEnvInfo 未返回 CDP 字段，因此桌面报告 `cdpEndpointObserved=false`；未伪造 TCP 地址。新增 Manager/CDP/Store 和 sdk-host 回调测试后，Dashboard 43 项、Rust workspace 89 项、Playwright 12 项通过；Browser 插件完成 AI 环境切换和 console 验证，截图 API 不可用时由 Playwright 完成视觉/响应式覆盖。`npm run check`、Clippy、production build 和真实 Tauri E2E 全部通过。

## 21. 阶段 18 Windows 安装交付验收

- `npm run release:windows` 必须同时产出 NSIS、便携 ZIP 和 `WINDOWS-RELEASE-MANIFEST.json`；默认命令不因可选 WiX/MSI 缺失而失败。
- `npm run release:verify` 必须验证便携目录、ZIP 内容、NSIS 版本、大小和 SHA-256。正式发布额外使用 `-RequireSignature`，内部未签名构建只报告状态。
- `npm run release:test:installer` 不读取凭据，只验证临时静默安装、必需资源、首次初始化页和静默卸载。
- `npm run release:test:installer:full` 通过安全提示读取 API Key，必须针对安装后的 release 完成初始化、环境 ready、AI 环境上下文、Provider 设置、停止和操作中心验证。
- `npm run release:test:msi` 必须对每个语言 MSI 完成无产品注册的 administrative extraction，并检查 Dashboard、sidecar 和 DLL。
- 安装测试检测到已有 BroSDK Dashboard 时必须拒绝运行，不能覆盖或卸载用户现有版本。
- 静默卸载保留默认用户数据且不弹框；测试使用唯一 `BROSDK_DATA_DIR` 并在退出时清理。

2026-07-26 阶段 18 结果：默认 NSIS 与便携发布、可选 `zh-CN/en-US` MSI、统一清单验证和无凭据首次启动烟雾测试全部通过；两个 MSI 均完成 administrative extraction 和资源检查。完整安装版 E2E 使用隐藏输入的测试 API Key 完成 `getUserSig(role=user) -> init -> env_page -> start -> callback ready -> AI/operation -> stop -> stopped`，然后静默卸载；报告不含 API Key、userSig、envId 或页面正文。当前 DLL 未暴露非零 CDP TCP 地址，结果保持 `cdpEndpointObserved=false`。测试后无安装注册、临时安装目录、Manager 数据目录或 `sdk-host` 残留。

## 22. 阶段 19 AI 会话与 Agent 执行验收

- AI 页面必须同时存在“会话”和只读“会话作用域”；阶段 28 后关联环境只能在新建会话弹窗中选择。
- 会话支持新建、切换、清空、删除和重载恢复；本地最多 20 个会话、每个 80 条，发送历史最多 40 条。
- Manager 必须限制 history 为 user/assistant、最多 40 条、单条 16 KiB、总计 128 KiB。
- 全局会话遇到用户文本中的单个已知 envId 时定位该环境；单环境会话不得覆盖绑定 envId，多个已知 envId 必须拒绝为单动作计划。
- `expectedState` 和 `idempotencyKey` 由 Manager 基于最新环境镜像生成，不能信任模型值；批准时仍做并发状态复验。
- 真实 Agent E2E 使用目标环境当前 stopped 状态生成计划，批准后看到 environment.start operation，等待 ready，再恢复 stopped。
- capability 只列实际 FFI 绑定；Cookie/security callback 与 token update 在接通前不得报告。

2026-07-26 阶段 19 结果：Dashboard 46 项、Rust workspace 92 项和 Playwright 桌面/移动 12 项通过，production build 通过。真实 Tauri 使用 `-AgentLifecycle -TargetEnvironmentId 2044366881367789568` 执行精确中文启动指令并恢复 stopped。当时允许文本 envId 覆盖旧关联环境；阶段 28 已将该行为限定为全局会话，单环境会话固定绑定。当前 DLL 端口仍为 0，`cdpEndpointObserved=false`。

## 23. 阶段 20 多环境 Agent 与完整单环境 MCP 验收

- `mcp-client` 的统一入口接收 `Option<envId>`；阶段 25 后两种作用域都请求 `/sdk/v1/mcp`，环境调用使用 `env.*` 并在 JSON arguments 中注入 envId。
- 全局仍只允许 9 个管理读取。ready 单环境的 `allowedTools` 必须等于 DLL 当次广告目录；不能把 17、18 或 19 写成产品常量。
- 任意单环境工具参数必须是 JSON object，最大 64 KiB、最多 16 层、单字符串最多 16 KiB；常用读取继续有结构化表单，其余工具使用高级 JSON 参数区。
- Agent 支持 `mcp.call`。全局会话使用显式 envId，单环境会话固定使用绑定 envId；Manager 写入最新 `expectedState`，执行时仍走 reservation、operation 和 DLL `tools/list` 校验。
- 每个 AI 会话独立保存“每次批准/自动执行”，默认每次批准。自动执行只省略 UI 的逐次点击，不跳过 Manager 状态、action、幂等和 ready 门禁。
- 执行尝试后不得再次显示同一计划的批准按钮；失败视为状态可能不确定，必须重新生成计划。
- `npm run e2e:multi-environment` 从当前用户安全存储复制 SDK/AI 加密 secret 到唯一临时数据目录，不把明文放入命令行；创建两个临时环境，结束时停止、清理、删除并核对账号环境总数恢复。

2026-07-26 阶段 20 结果：真实双环境 E2E 覆盖 Agent 手动和自动启停、两个环境均 ready/stopped、每环境最少 18 个工具、Agent MCP tabs 调用、远端指纹刷新和补偿清理。该阶段允许文本覆盖旧关联环境的行为已在阶段 28 收紧为“仅全局会话可按显式 envId 定位，单环境会话固定绑定”。查询参数路由也已由阶段 25 的全局 `env.tabs + arguments.envId` 替代。临时环境清理 2/2，账号环境数 1 -> 1，无 `sdk-host` 残留。

## 24. 阶段 21 Windows Runtime Host 后台化验收

- Dashboard 发起的 `sdk-host serve`、`capabilities` 和 `smoke` 必须使用同一后台进程配置，Windows 不得创建可见终端窗口。
- `CREATE_NO_WINDOW` 不能破坏 stdout/stderr 重定向；一次性 JSON 命令、启动错误和 supervisor 状态必须继续可观测。
- Windows 行为测试必须在子进程内部验证 `GetConsoleWindow() == 0`，不能只断言配置常量存在。
- `cargo run -p sdk-client --bin runtime-host-smoke` 必须覆盖运行、health、优雅停止和强制退出，结束后不得残留 `sdk-host`。

2026-07-27 阶段 21 结果：长期和一次性 Host 启动均通过统一后台进程工厂设置 `CREATE_NO_WINDOW`。Windows 子进程行为测试确认无 console handle 且 stdout 捕获正常；真实 runtime-host smoke 通过，Rust workspace 94 项和 Clippy 全部通过，退出后无 `sdk-host` 残留。

## 25. 阶段 22 Windows 桌面生命周期验收

- `BroSDK Dashboard.exe` 和 `sdk-host.exe` 的 PE `Subsystem` 必须为 2；`npm run release:verify` 和安装测试都必须执行该门禁。
- `npm run e2e:tray` 必须真实发送主窗口关闭请求，确认窗口隐藏但进程不退出，再通过托盘恢复，并从右键菜单退出。
- 托盘退出等待 runtime 优雅停止必须有上限；测试结束不得残留 Dashboard、`sdk-host` 或 `brosdk-dashboard-tray-e2e-*` 临时目录。
- debug 和便携 release 都执行托盘 E2E；`npm run release:test:installer` 继续覆盖安装后首次启动与静默卸载。

2026-07-27 阶段 22 结果：旧便携产物实测 Dashboard/host 为控制台 `Subsystem=3`；修复后 debug 和新便携产物均为 GUI `Subsystem=2`。两种构建的托盘隐藏/恢复/退出通过，release 清单和 NSIS 临时安装、首次启动、静默卸载通过，无进程或临时目录残留。

## 26. 阶段 23 环境启动回调进度验收

- Store 必须从 DLL callback 的 `data.percent` 或 `data.progress` 读取 0-100 进度，并把安全摘要同步到当前环境和 operation；非终态事件不能把 operation 提前标为 succeeded。
- Dashboard 只在 starting 环境显示进度条，页面不可显示 callback payload JSON；桌面和移动 Playwright 都断言 `aria-valuenow` 与可见百分比一致。
- `npm run e2e:environment` 必须在 Manager 事件流中观察真实 `browser-open` 中间进度；报告只允许输出 `startProgressCallbackObserved`，不得输出 envId、payload 或凭据。
- 终态仍只由 `browser-open-success`/失败事件决定，progress callback 不替代 ready 语义。

2026-07-27 阶段 23 结果：真实唯一环境报告 `startProgressCallbackObserved=true`、`readySource=sdk_callback`，随后 CDP evaluate、18 个 MCP 工具、指纹检查和停止均通过。Dashboard 51 项、Playwright 桌面/移动 16 项、Rust workspace 99 项、check、Clippy、runtime smoke、production/release 构建通过；测试结束目标环境已恢复 stopped，无进程或本轮临时目录残留。

## 27. 阶段 24 客户端重启状态与单实例验收

- Store 测试必须构造遗留 `environment.start` running operation 和 starting 环境，重开后 operation 变为 `CLIENT_RESTARTED`，BrowserInfo 只返回 envId/端口 0 时环境恢复 ready。
- 同一遗留环境不在 BrowserInfo 列表时必须恢复 stopped；两种情况都清除旧 operation 绑定和过期 CDP 地址。
- `npm run e2e:tray` 在首个窗口隐藏后启动同一可执行文件，第二个进程必须正常退出并唤醒首个窗口；随后仍须完成托盘恢复和菜单退出，测试结束无 Dashboard/sdk-host 残留。
- 应用单实例使用 Tauri identifier，不替代 DLL appId 锁；必须确认 single-instance 插件先于会初始化 Manager/runtime 的其它插件注册。

2026-07-27 阶段 24 结果：遗留 starting/ready 的有/无 BrowserInfo 两条恢复路径均由 Store 测试通过，Rust workspace 共 101 项测试及目标 Clippy 通过。`npm run e2e:tray` 真实启动第二个相同进程，报告 `secondInstanceRedirected=true`，随后托盘恢复和菜单退出通过；测试后无 Dashboard、sdk-host 或临时目录残留。

## 28. 阶段 25 新版全局多环境 MCP 验收

- 环境 discovery 和 call 的首个 HTTP 请求路径必须都是 `/sdk/v1/mcp`，不得带 `?envId=`；调用 JSON 必须包含 `name=env.tabs` 和 Manager 选中的 arguments.envId。
- 用户参数中已有不同 envId 时必须由 Manager 覆盖；旧基础名称 `tabs` 必须规范为 `env.tabs`。
- 环境目录只能包含浏览器 `env.*` 工具，必须排除 `env.list/resolve/get/create/update/destroy`；全局 mutation 继续走 Manager operation。
- MCP 页面显示真实 `env.*` 名称且结构化参数表单可用；Agent 新计划优先生成带前缀工具名。
- `npm run e2e:environment` 必须发现 allowed=advertised 的 18 个浏览器工具，通过 `env.tabs/env.read` 后停止环境，报告中不输出 envId 或页面正文。

2026-07-27 阶段 25 结果：Dashboard 51 项、Rust workspace 101 项、Playwright 桌面/移动 16 项、check 与 Clippy 全部通过。真实 DLL 生命周期 E2E 报告 `embeddedMcpAdvertisedToolCount=18`、`embeddedMcpAllowedToolCount=18`、`embeddedMcpToolVerified/embeddedMcpReadVerified/environmentStopped=true`；全局管理集合未进入单环境白名单，凭据和真实 envId 未写入输出。

## 29. 阶段 27 AI 原生工具与安全自检验收

- AI Provider 首轮请求必须携带标准 function `tools`；返回 `tool_calls` 后，Chat 只执行绑定的读取工具，并以 `role=tool/tool_call_id` 完成第二轮回答。第二轮再次请求工具必须拒绝。
- Chat 不得绑定 `env.navigate/env.act/env.evaluate/upload/download` 等 mutation；Agent 可以从所选 ready 环境的运行时目录选择这些工具，但模型调用只能先生成一个计划。
- DLL `inputSchema` 必须进入模型工具定义；单环境 schema 移除 `envId/env_id`。模型参数伪造 envId 时，Manager 计划仍绑定用户选择或文本解析出的精确环境。
- `npm run e2e:dashboard` 必须验证总览无 `SDK Smoke`、设置页存在“运行 SDK 自检”且有运行环境时禁用；真实桌面 runner 必须在环境启动前完成自检。
- `npm run docs:screenshots` 必须生成三张 README PNG，并对每个页面检查控制台 warning/error 和横向溢出。

2026-07-27 阶段 27 结果：DeepSeek `deepseek-v4-flash` 完成标准 `tools -> tool_calls -> role=tool` 回合；真实 Tauri 报告 `sdkSelfCheckObserved/agentPlanObserved/agentApprovalInvoked/agentOperationObserved/readyObservedInDashboard/stoppedObservedInDashboard=true`。底层环境 E2E 经全局 endpoint 发现并放行 18/18 个单环境工具，`env.tabs/env.read`、启动进度、页面诊断和指纹检查通过，环境最终 stopped。Dashboard 51 项、Rust workspace 110 项、Playwright 桌面/移动 18 项、check、Clippy、production build、三张 README 截图和 Windows NSIS/便携发布校验全部通过；截图和报告不含真实 envId、API Key、userSig 或 MCP 响应正文。

## 30. 阶段 28 AI 会话作用域与自动 Agent 验收

- 新建会话必须显式选择全局或单环境；创建后主页面不得存在可编辑关联环境控件，重载和切换会话必须保持原作用域。
- 全局会话的模型 tools 只能来自全局 discovery；单环境会话只能来自绑定环境 discovery，不能混入 `env.list/resolve/get/create/update/destroy`。
- 单环境 Agent 文本指定其它 envId 必须 fail closed；全局 Agent 可指定一个已知 envId，多个目标仍拒绝。
- `npm run e2e:ai-assistant` 使用真实 SDK/AI 凭据，依次验证全局 Chat、单环境 Chat、Chat mutation guard 和自动 Agent 重启；结束必须恢复目标环境初始状态。
- Dashboard 单元测试验证 Enter 发送、Shift+Enter 换行；Playwright 在 1440x900 与 390x844 验证会话弹窗、不可变作用域、输入区和发送按钮无溢出。

2026-07-27 阶段 28 结果：真实 DeepSeek 报告 `globalChatReplyVerified/environmentChatReplyVerified/chatMutationReplyVerified=true`，自动重启观察到 stop、start 和最终 ready，使用 2 个工具回合并恢复初始状态。真实 Tauri 报告初始化、自检、ready/stopped、单环境上下文、Provider 设置、全局 Chat Enter 回复和 operation 身份均通过；DLL 未暴露 TCP CDP，`cdpEndpointObserved=false`。Dashboard 52 项、Rust workspace 114 项、Playwright 18 项、check/Clippy、production build、三张 README 截图、NSIS/便携构建和 release verify 全部通过；无 Dashboard、sdk-host 或临时 UI E2E 目录残留。

## 31. 阶段 29 AI 会话最新消息跟随验收

- Dashboard 组件测试必须在追加 AI 回复后验证消息容器滚动到当前 `scrollHeight`，不能只检查回复文本存在。
- Playwright 必须预置足够长的本地会话，先确认 `scrollHeight > clientHeight`，再等待底部距离不超过 2px。
- 桌面 1440x900 与移动 390x844 都必须覆盖；页面不得出现 console error、pageerror 或请求失败。
- 会话 id 切换和消息集合变化都应触发滚动，以覆盖历史会话、Chat 回复及 Agent 执行状态更新。

2026-07-27 阶段 29 结果：Dashboard 53 项、Playwright 桌面/移动 20 项、TypeScript check 和 production build 通过。48 条历史消息在两个视口均形成真实滚动区域并自动到达底部。

## 32. 阶段 30 MCP 自动激活与 AI 状态强对账验收

- `embeddedMcpPort=null` 且 DLL capability 可用时，初始化必须获得非零自动端口并让 snapshot 报告 `mcp.active=true`；显式端口仍优先于环境变量和自动端口。
- AI Chat 状态读取、Agent 规划、自动运行和批准执行都必须在读取环境状态前调用 DLL 全局 MCP `browser.status`；对账失败时返回错误，不能回退到 SQLite 缓存继续决策。Dashboard 启动/恢复对账仍使用 `sdk_browser_info`。
- `npm run e2e:ai-assistant` 不得设置 `BROSDK_EMBEDDED_PORT`，必须报告 `automaticMcpActivated/agentStartObserved=true`，并继续验证自动重启和状态恢复。
- 设置页在桌面与移动 Playwright 中必须显示“留空则自动选择”，不能再暗示空值会关闭 Agent 所需的 MCP。

2026-07-27 阶段 30 真实结果：DeepSeek `deepseek-v4-flash` 与当前 DLL 报告自动 MCP 激活、stopped 环境启动、重启 stop/start/ready、全局/单环境 Chat 和初始状态恢复全部通过；未向 runner 注入固定端口。Dashboard 53 项、Rust workspace 115 项、Playwright 20 项、check/Clippy、production build 和托盘/单实例 E2E 通过，无 Dashboard、sdk-host 或隔离测试目录残留。

## 33. 阶段 31 AI 全局 MCP 生命周期与停止状态验收

- 单元测试必须覆盖全局 `browser.status` 的 `structuredContent`/文本 payload 解析，以及运行列表中不存在的环境被对账为 stopped。
- 活跃 open operation 在状态列表中出现目标前保持 starting，出现后成功；活跃 close operation 在目标消失前保持 stopping，消失后成功，不能被中间轮询提前完成。
- Agent 工具目录必须来自 DLL 动态发现的 `browser.open/browser.close`。全局会话 schema 要求 envId；单环境会话 schema 不暴露 envId，并拒绝文本中的其它已知环境。
- `npm run e2e:ai-assistant` 必须先在目标实际 stopped 时询问“是否已经启动”，断言回复不声称 ready、没有生命周期写步骤且环境仍 stopped；后续启动和重启的执行摘要必须包含 `transport=dll-global-mcp`。

2026-07-27 阶段 31 真实结果：DeepSeek `deepseek-v4-flash` 在目标环境实际 stopped 时调用实时状态工具并明确回答未启动，随后全局 MCP 启动和自动重启通过，最终恢复初始状态。报告包含 `stoppedStatusReplyVerified=true`、`agentLifecycleUsedGlobalMcp=true`；实测并交付的 DLL 为 2.1.0.0，源码目录与发布包哈希一致。Dashboard 53 项、Rust workspace 120 项、Playwright 20 项、check/Clippy、production build 全部通过，凭据和目标 envId 未写入报告。

## 34. 阶段 32 AI 全局环境 MCP 与步骤可观测性验收

- 全局 Agent 工具目录必须包含 DLL 全局 endpoint 广告的 `env.*` 浏览器工具，模型函数名为 `mcp_global_*`，schema 显式要求 `envId`；单环境 Agent 继续隐藏 envId 并由 Manager 注入。
- Chat 模式仍只绑定读取工具；`env.navigate/env.act` 等会改变页面状态的工具只能进入 Agent。
- `mcp.call` 执行时必须区分全局读取工具、全局 `env.*` 浏览器工具和单环境浏览器工具。全局 `env.*` 调用从参数读取 envId，要求目标 ready，并把 operation 绑定到该 envId；`browser.status(envId)` 这类状态读取不能被误判为页面 mutation。
- 自动 Agent 步骤 UI 必须显示工具名、envId、operation id 和脱敏后的工具参数，避免用户只能看到多个无法区分的 `mcp.call`。
- `npm run e2e:ai-assistant` 必须覆盖真实全局会话导航：环境 ready 后要求打开 `https://example.com/`，断言模型使用 `env.navigate`，且没有再次调用生命周期 `browser.open/browser.close`。

2026-07-27 阶段 32 真实结果：DeepSeek `deepseek-v4-flash` 真实 AI E2E 通过，报告包含 `globalNavigateToolObserved=true`、`globalNavigateAvoidedLifecycle=true`、`inactiveEnvGetHandled=true`、`initialStateRestored=true`。Dashboard AI 组件测试新增自动 Agent MCP 参数展示断言，Manager 单元测试新增全局 `env.navigate` 绑定、schema envId、Chat 过滤 mutation、全局状态读取不强制 ready 和全局环境工具执行校验。桌面 E2E 脚本修复 ready baseline 竞态、停止状态中文否定误判和自动/手动执行模式切换。

## 35. 内核安装反馈回归

- `npm run test --workspace apps/dashboard -- App.test.tsx` 必须覆盖真实桌面点击“安装 Chrome”后，`installKernel` Promise 未返回时顶部“内核安装进度”和行内“等待 SDK 受理”已经可见，状态列主徽标显示“安装中”；还要覆盖 SDK 成功回调后聚焦行显示“安装完成”。
- `cargo test -p sdk-host install_progress --lib` 不适用于 `sdk-host`，该包只有二进制测试目标；使用 `cargo test -p sdk-host install_progress` 覆盖 `sdk_browser_install` 返回 `CL_DONE` 后，host 只在 pending install 身份唯一时绑定回调 reqId，身份歧义时留给 Manager 按内核字段匹配。
- `cargo test -p manager kernel_install_progress_updates_operation_message --lib` 必须覆盖 `kernel.install` 中间回调刷新 operation message，成功回调再切换到 `succeeded`。
- `cargo test -p manager stale_kernel_install_operation_becomes_retryable_failure --lib` 必须覆盖 SDK 已受理但无下载进度回调时，Manager 将 operation 收敛为 `failed / SDK_INSTALL_TIMEOUT`，操作中心可重试。
- `npm run e2e --workspace apps/dashboard -- -g "kernel"` 必须在桌面和移动视口断言 `browser-install · Downloading · 42%` 可见、状态列主徽标显示“安装中”、安装按钮禁用、同版本 linux/macos/windows catalog 只显示当前 `platform/arch`、页面无横向溢出和无应用控制台错误。
- 如果 `sdk_browser_install` 返回 `CL_DONE` 而不是可跟踪 reqId，优先检查 `sdk-host` 是否把首个安装回调的 `reqId` 绑定到 pending install operation；不要改成前端自行下载内核包，DLL 已负责下载、校验、解压和注册。
- `cargo test -p manager kernel --lib` 必须覆盖内核最新状态对账：本地和远端 `versionCode` 相同且 digest 相同时，即使展示版本字符串不同也保持 `installed`；`versionCode` 或 `sha256/md5` 任一不一致都标记 `update-available`；Manager 直接安装当前 `installed` 内核应返回不可用错误。
- `npm run test --workspace apps/dashboard -- App.test.tsx` 必须覆盖当前已安装内核的安装按钮禁用并提示“已是最新版本”。

2026-07-28 结果：Dashboard 59 项组件测试、Playwright 桌面/移动 32 项、Rust workspace 测试、Rustfmt、Clippy、production build、Browser 插件内核矩阵可视验证、真实 API Key `npm run manager:smoke`（`serverKernelListLoaded=true/count=12`）、Windows NSIS/便携构建和 `npm run release:verify` 均通过。

2026-07-29 结果：Dashboard 60 项组件测试、Manager 95 项 lib 测试、Rust workspace 测试、Rustfmt、Manager Clippy、Dashboard production build 和内核页 Playwright 桌面/移动 6 项均通过。
