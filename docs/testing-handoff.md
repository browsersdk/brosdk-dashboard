# 测试与交接说明

## 1. 测试凭据

本项目测试需要 BroSDK API Key。不要把真实 key 写进任何源码、文档、提交信息、日志或截图。

PowerShell 设置方式：

```powershell
$env:BROSDK_API_KEY = "<api-key>"
```

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

该命令验证 SQLite 初始化、runtime host 启停、持久化 operation、snapshot 和 `events_since`。未设置 `BROSDK_API_KEY` 时，同步 operation 预期以 `SDK_HOST_ERROR` 失败并产生 queued/running/failed 事件；设置密钥时预期执行真实 `sdk_env_page` 并更新环境镜像。

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
  -> sdk_browser_close({ envs: [envId] })
  -> wait browser-close-success
  -> sdk_browser_info no longer contains envId
  -> sdk_shutdown
```

自动 runner：

```powershell
npm run e2e:environment
```

默认必须设置 `BROSDK_E2E_ENV_ID`。如测试账号只有一个环境，可显式设置 `BROSDK_E2E_USE_ONLY_ENV=1`；runner 会先调用 `sdk_browser_info`，若该环境已经运行则拒绝接管或停止。设置 `BROSDK_E2E_SIMULATE_MANUAL_CLOSE=1` 时，runner 通过 CDP `Browser.close` 模拟用户关闭整个浏览器并验证 Manager 对账；需要真人关闭窗口时，改用 `BROSDK_E2E_MANUAL_CLOSE_TIMEOUT_SECS` 正整数。若设置了 `BROSDK_EMBEDDED_PORT`，runner 会确认 DLL 自带 MCP 端口实际监听。

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
5. 当前阶段 0-9 已完成；新增范围先更新 `docs/roadmap.md`，再实施、测试、更新文档并独立提交。
6. 所有 API Key 只从环境变量读取，不写入仓库、日志或截图。

涉及 DLL 生命周期或 MCP 的改动必须继续通过隔离 host、Manager operation 和脱敏边界，不允许 Dashboard 直接调用 DLL/MCP/CDP。
