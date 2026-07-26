# 测试与交接说明

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

该 runner 驱动 Windows Tauri 窗口中的环境表按钮；已有已初始化窗口时直接复用，没有窗口时启动隔离桌面进程并通过隐藏 API Key 完成初始化。它必须等待明确的“运行中”状态，不能把启动中已经可用的停止按钮误判为 ready；随后验证 AI 环境 envId、CDP 地址或 DLL 内部控制通道、AI Provider 设置入口、停止状态和操作中心身份。测试结束前会把目标环境恢复为 stopped，隔离启动时还会关闭进程并清理临时数据目录。

可选测试变量：

```powershell
$env:BROSDK_E2E_ENV_ID = "<existing-env-id>"
$env:BROSDK_E2E_ALLOW_MUTATION = "0"
$env:BROSDK_E2E_USE_ONLY_ENV = "0"
$env:BROSDK_E2E_MANUAL_CLOSE_TIMEOUT_SECS = "0"
$env:BROSDK_E2E_SIMULATE_MANUAL_CLOSE = "0"
$env:BROSDK_WORK_DIR = "D:\go\src\browsersdk\brosdk-dashboard\runtime\sdk-work"
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

Manager Domain smoke：

```powershell
npm run manager:smoke
```

该命令验证 SQLite 初始化、runtime host 启停、持久化 operation、snapshot 和 `events_since`。未设置 `BROSDK_API_KEY` 时，同步 operation 预期以 `SDK_HOST_ERROR` 失败并产生 queued/running/failed 事件；设置密钥时预期执行真实 `sdk_env_page` 并更新环境镜像。额外设置 `BROSDK_EMBEDDED_PORT` 时，还会发现全局工具并调用 `sdk.health`、`env.list`；缓存存在环境时再调用 `mcp.endpoint`。报告只输出协议、数量和成功布尔值。隔离测试数据应同时设置 `BROSDK_DATA_DIR`，不要使用不存在的数据库文件覆盖变量。

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

## 10. 新会话接力提示

阶段 9 AI 验收补充：

- `cargo test -p manager agent_execution_survives_reopen` 验证幂等执行记录跨重启保留。
- Chat 上下文只包含脱敏摘要，不包含数据目录、workDir、logDir、完整代理 URL、CDP endpoint、API Key 或 userSig。
- 同一 `idempotencyKey` 只能对应一个序列化计划；不同计划复用必须返回 `INVALID_AGENT_PLAN`。
- Agent 在调用工具前写入 reservation；中断或结果未落盘时相同 key 必须返回 `AGENT_EXECUTION_UNCERTAIN`，不能重复执行。
- `npm run ai:smoke` 只记录模型、只读标志和回答长度。
- MCP 单元测试必须验证 `initialize -> initialized notification -> tools/list -> tools/call -> DELETE` 和 session/version headers。
- 设置 `BROSDK_EMBEDDED_PORT` 的环境 E2E 必须完成一次 Manager 路由的 `tabs(list)`，输出只记录 `embeddedMcpToolVerified=true`，不记录 envId、页面 URL 或工具正文。

2026-07-25 阶段 9 最终验证结果：

- `npm run ai:smoke` 通过，仅输出模型、只读标志和回答长度。
- `npm run sdk:smoke` 通过，确认 `getUserSig` 请求使用 `role=user`，并完成 init/info/env_page/shutdown。
- `npm run sdk:runtime-smoke` 和带真实账号的 `npm run manager:smoke` 通过，host 优雅停止和异常退出降级均符合预期。
- 带 `BROSDK_EMBEDDED_PORT`、唯一环境选择和模拟手动关闭的 `npm run e2e:environment` 通过，结果包含 `runtimeEvaluateVerified=true`、`embeddedMcpReachable=true`、`embeddedMcpToolVerified=true`、`manualCloseVerified=true`。
- `npm run check`、`npm test`、`npm run build` 通过；Dashboard 在 1440x900 和 390x844 下完成 AI/MCP 导航、Chat/Agent 切换、禁用态、控制台和横向溢出检查。

新会话可以直接从这里开始：

1. `cd D:\go\src\browsersdk\brosdk-dashboard`
2. 阅读 `docs/README.md`、`docs/architecture.md`、`docs/dll-integration.md`、`docs/roadmap.md`。
3. 运行 `git status --short --branch`，确认是否存在未提交工作。
4. 运行 `npm run check`、`npm test`、`npm run build` 建立基线。
5. 当前阶段 0-14 已完成；新增范围先更新 `docs/roadmap.md`，再实施、测试、更新文档并独立提交。
6. 产品 API Key 使用平台安全存储，测试 Key 只从进程环境读取；两者都不写入仓库、SQLite、日志或截图。

涉及 DLL 生命周期或 MCP 的改动必须继续通过隔离 host、Manager operation 和脱敏边界，不允许 Dashboard 直接调用 DLL/MCP/CDP。

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
- 单环境 MCP 测试覆盖 advertised tools 与 Manager 白名单交集；非 ready 环境、未公布工具和变更工具必须拒绝。
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
