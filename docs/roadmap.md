# BroSDK Dashboard 新客户端路线图

## 1. 阶段总览

| 阶段 | 优先级 | 状态 | 结果 |
| --- | --- | --- | --- |
| 0. 项目骨架 | P0 | 已完成 | Tauri/Rust workspace、React Dashboard、基础构建跑通 |
| 1. DLL Smoke | P0 | 实现完成，联网验证受环境变量门禁 | Windows x64 能加载 `brosdk.dll`，完成 getUserSig/init/info/env_page |
| 2. Runtime Host | P0 | 已完成 | DLL 被隔离在子进程，Manager 通过 pipe/UDS 调用 SDK |
| 3. Manager Domain | P0 | 已完成 | SQLite、settings、operation、环境镜像和事件流完成 |
| 4. Dashboard MVP | P0 | 已完成 | 总览、环境列表、启动、停止、运行详情完成 |
| 5. 环境 E2E | P0 | 已完成 | 使用测试 API Key 完成列表、启动、ready、CDP command、手动关闭对账入口、停止 |
| 6. 完整菜单 | P1 | 已完成 | 指纹、代理、内核、操作、设置菜单与 Manager API 已补齐 |
| 7. 打包发布 | P1 | 已完成（安装器需本机工具链） | Windows 便携包、Tauri 资源、图标、WebView2、签名准备、升级策略 |
| 8. 跨平台 | P2 | 基础 adapter 已完成，等待平台动态库 | macOS/Linux 路径、UDS、系统 keyring、能力报告和交叉编译完成 |
| 9. AI Agent | P2 | 已完成 | DeepSeek/OpenAI 兼容 Chat、审批 Agent、持久化幂等、DLL MCP 只读 adapter |

## 2. 阶段 0：项目骨架

目标：建立可以长期维护的新仓库结构。

任务：

- 初始化 Rust workspace。
- 初始化 `apps/desktop` Tauri 2。
- 初始化 `apps/dashboard` React + TypeScript + Vite。
- 建立 `crates/domain`、`crates/manager`、`crates/sdk-ffi`、`crates/sdk-host`、`crates/sdk-client`。
- 复制当前 fingerprint 图标作为统一应用图标。
- 加入基础格式化、lint、测试脚本。

验收：

- `cargo test` 可运行。
- `npm run build --prefix apps/dashboard` 可运行。
- `cargo tauri dev` 能打开空 Dashboard。

## 3. 阶段 1：DLL Smoke

目标：证明当前 `libs/windows_x64` 动态库可由新项目可靠调用。

任务：

- 使用 `libloading` 查找 `sdk_get_user_sig`、`sdk_init`、`sdk_info`、`sdk_env_page`、`sdk_shutdown`、`sdk_free`。
- 报告 DLL 内嵌 HTTP/WS/MCP capability；需要启用时通过 `BROSDK_EMBEDDED_PORT` 把 `port` 传给 `sdk_init`。
- 实现安全 JSON bytes 入参和 SDK out buffer 释放。
- 从环境变量读取 `BROSDK_API_KEY`。
- 调 `sdk_get_user_sig` 获取 userSig。
- 用独立 workDir 调 `sdk_init`。
- 调 `sdk_info` 和 `sdk_env_page`。
- 调 `sdk_shutdown`。

验收：

- 不把 API Key、userSig 或完整 Authorization 写入日志。
- DLL 加载失败、符号缺失、网络失败、JSON 解析失败都有稳定错误码。
- 测试结束后无残留 `sdk-host` 进程。

## 4. 阶段 2：Runtime Host

目标：隔离 DLL 风险，消除动态端口对客户端内部链路的依赖。

任务：

- 新增 `sdk-host` 独立可执行文件。
- Windows 使用 named pipe，macOS/Linux 预留 UDS trait。
- Manager 启动、监督、停止 `sdk-host`。
- `sdk-host` 注册 result/log callbacks，并把事件推给 Manager。
- `sdk-host` 作为唯一可启用 DLL 内嵌 MCP 的进程；Manager 负责端口、生命周期和对外路由。
- 增加 request id 与 operation id 映射。
- 处理 `sdk-host` 退出、崩溃、超时和 appId 锁冲突。

验收：

- 人为 kill `sdk-host` 后，Dashboard 不退出，环境状态进入 degraded/unknown。
- 同 `appId` 锁冲突不会关闭桌面 UI。
- 同一时间只有一个 `sdk_init` 流程。

## 5. 阶段 3：Manager Domain

目标：建立本地事实来源和操作模型。

任务：

- SQLite schema：settings、environments、operations、runtime_snapshots、proxy_profiles、fingerprint_profiles。
- Operation 队列：queued/running/succeeded/failed/cancelled。
- 环境远端镜像：`sdk_env_page` -> 本地列表。
- 设置：workDir、extensionDir、logDir、sdkApiUrl、debug。
- 事件：snapshot + incremental events。
- 诊断日志脱敏。

验收：

- 页面刷新后可以恢复 operation 状态。
- SDK callback 晚到不会把已停止环境误改回 ready。
- 手动关闭浏览器后，下一轮对账能更新状态。

## 6. 阶段 4：Dashboard MVP

目标：把当前 windows-webview 的核心体验搬进新客户端。

任务：

- 总览：SDK 初始化状态、运行环境数、最近操作、组件健康。
- 环境列表：搜索、状态筛选、远端 envId、本地标签、启动/停止按钮。
- 运行详情：状态、CDP、reqId、最后事件、错误信息。
- 操作记录：启动、停止、同步、失败详情。
- 设置：API Key 来源状态、workDir、扩展目录、日志目录。

验收：

- 窗口最小尺寸下无横向页面溢出。
- 启动/停止按钮不因动态文案挤出表格。
- `sdk_browser_open` accepted 后显示 starting，不直接显示 ready。

## 7. 阶段 5：环境 E2E

目标：使用真实 SDK 链路完成环境生命周期。

任务：

- `env_page` 拉取环境。
- 若设置了 `BROSDK_E2E_ENV_ID`，启动该环境。
- 等待 `browser-open-success` 或 `sdk_browser_info` ready。
- 调用 `sdk_browser_command` 执行简单 CDP 命令。
- 手动关闭浏览器对账。
- 调用 `sdk_browser_close` 停止环境。

验收：

- 新建、启动、停止、删除这类会修改远端/本地状态的测试必须由 `BROSDK_E2E_ALLOW_MUTATION=1` 显式开启。
- 默认 E2E 只执行读取和指定环境启动/停止。
- 所有测试输出不包含 API Key、userSig、代理密码、Cookie 明文。

实现结果：

- `npm run e2e:environment` 提供生命周期 runner；无 API Key 或目标环境时安全跳过。
- 默认要求 `BROSDK_E2E_ENV_ID`。仅在显式设置 `BROSDK_E2E_USE_ONLY_ENV=1`、账号镜像恰好一个环境且该环境当前未运行时，才允许自动选中。
- runner 通过 callback 的 `type` 字段识别生命周期事件，并按 `envId + open/close` 方向绑定 operation；DLL 的同步非负返回值只表示 accepted，不能当作 callback `reqId`。
- ready 必须来自 `browser-open-success` 或带有效 CDP endpoint 的 `sdk_browser_info`。Starting 条目不会被误判为 ready，活跃 start operation 也不会被空对账提前改回 stopped。
- CDP 验证使用 `Target.getTargets -> Target.attachToTarget -> Runtime.evaluate -> Target.detachFromTarget`，避免把页面命令发到 browser target。
- 设置 `BROSDK_E2E_SIMULATE_MANUAL_CLOSE=1` 后，runner 通过 CDP `Browser.close` 自动验证手动关闭对账；`BROSDK_E2E_MANUAL_CLOSE_TIMEOUT_SECS` 仍可保留真人关闭窗口。未触发手动关闭时自动执行 SDK close。
- 设置 `BROSDK_EMBEDDED_PORT` 时，runner 会确认 DLL 自带 MCP 端口实际开始监听。

## 8. 阶段 6：完整菜单

目标：补齐本地客户端的全部菜单能力。

任务：

- 指纹：读取、预览、导入/导出、与环境绑定关系展示。
- 代理：HTTP/SOCKS5 URL 粘贴解析、凭据保护、网络诊断。
- 内核：安装、更新、卸载、缓存清理、无下载源状态。
- 操作：筛选、重试、取消、日志查看。
- 设置：数据目录、扩展目录、启动策略、诊断包。

验收：

- 每个菜单有至少一个自动化测试覆盖主要读写链路。
- 内核列表不知道最新版本时显示“未知”，不能误报可更新。
- 代理和扩展路径使用系统选择器，保留高级手输入口。

实现结果：

- 指纹支持本地 JSON profile 新建、编辑、导入、导出、删除和环境绑定；远端详情通过 `sdk_env_getinfo` 缓存为脱敏摘要，运行环境可打开 DLL 内置指纹检查页。
- 代理支持 HTTP/HTTPS/SOCKS5/SOCKS5H URL 解析；密码在 Windows 通过 DPAPI 保存为文件密文，SQLite 只保存 `secret_ref`；接通 DLL 网络与系统代理诊断。
- 内核合并 SDK catalog 和 `<workDir>/**/cores/**/.core.json` 本地扫描结果；接通安装、缓存清理和受运行状态保护的本地卸载。缺失下载 URL 或最新版本时显示“未知”。
- operation 保存脱敏 request snapshot，页面支持状态/类型/文本筛选、详情、取消和受支持类型重试。
- 设置支持数据/SDK/扩展/日志目录、启动策略、Debug 与 DLL MCP 端口；使用原生目录选择器，数据目录迁移通过 SQLite 在线备份并在重启后生效；可导出不含密钥、Cookie 和代理密码的诊断包。
- 自动验证覆盖 Rust 单元测试、workspace clippy、Dashboard production build，以及 1440x900 与 390x844 浏览器截图/交互检查。

## 9. 阶段 7：Windows 发布

目标：让普通用户可以安装和运行。

任务：

- 便携包和安装包布局。
- WebView2 Runtime 检查。
- fingerprint 图标统一应用到 exe、窗口、任务栏和安装器。
- 用户数据目录默认 `%LOCALAPPDATA%/BroSDK Dashboard`。
- 日志和诊断包导出。
- 签名、版本清单、升级/回滚方案。

验收：

- 双击 exe 直接打开客户端，不出现空白页。
- 端口占用不影响内部启动。
- 卸载可选择保留或清理用户数据。

## 10. 阶段 8：跨平台

目标：在不改 Dashboard 和 Manager domain 的前提下增加平台。

任务：

- 加入 `libs/macos_universal` 或明确 arch 目录。
- 加入 `libs/linux_x64`。
- 实现 UDS、进程树、系统密钥库、文件权限和打包 adapter。
- 验证 SDK 动态库导出符号与 Windows 一致。

验收：

- 同一 E2E 测试可在不同平台运行。
- 平台不支持能力时返回 capability，不在 UI 中静默失败。

## 11. 阶段 9：AI Agent

目标：在环境管理稳定后加入自动化。

任务：

- Chat 模式只读：概览、环境、指纹、代理、内核、操作、诊断。
- Agent 模式受控操作：启动、停止、代理测试、环境诊断。
- MCP 工具统一显式 `envId` 参数。
- 写操作加入 idempotency key、expected state、必要时审批。
- 浏览器任务基于 `sdk_browser_command`、snapshot 和后续 MCP adapter。

验收：

- Chat 模式无法触发任何写操作。
- Agent 不能把 accepted 说成 ready。
- Agent 操作都有 operation id 和可追溯事件。

## 12. 当前状态

阶段 0-9 的仓库内规划已完成。当前没有已批准的阶段 10；后续新增需求应先在本文件补充阶段目标、任务、风险和验收标准，再按“实现 -> 自动测试 -> 文档 -> Git 提交”的循环推进。

阶段 7 实现结果：

- Tauri Windows bundle 已启用 NSIS/MSI 配置，便携包使用相同的 Dashboard、`sdk-host.exe` 和 `brosdk.dll` 资源布局。
- 运行时支持便携目录、安装目录、Tauri `resources`/`resources/bin` 和目标三元组 sidecar 名称发现。
- 发布脚本兼容当前 Windows PowerShell，生成 `RELEASE-MANIFEST.json` 和 ZIP，并由 `npm run release:verify` 校验文件存在性、SHA-256 和大小。
- WebView2 使用 `embedBootstrapper`；正式发布仍需在 Windows 构建机安装 NSIS/WiX，证书由发布环境注入。

阶段 8 实现结果：

- 平台 resolver 按 `windows_x64`、`windows_arm64`、`macos_universal`、`linux_x64` 解析动态库目录和文件名。
- Windows 使用 named pipe + DPAPI；macOS/Linux 使用 UDS + 系统 keyring（Keychain/Secret Service），不再回退到明文 secret 文件。
- `SdkCapabilities` 报告 support status、unsupported reason、动态库、IPC 和密钥后端；缺少平台动态库时明确 unavailable。
- Windows workspace 测试通过，并对 `x86_64-unknown-linux-gnu`、`x86_64-apple-darwin` 完成平台核心 crates 的交叉 `cargo check`。

阶段 9 实现结果：

- 新增 `ai-agent` crate，兼容 OpenAI `/chat/completions`；默认支持 DeepSeek `https://api.deepseek.com` 与 `deepseek-v4-flash`，通过 `BROSDK_AI_API_KEY`、`BROSDK_AI_BASE_URL`、`BROSDK_AI_MODEL` 配置。
- Chat 只发送脱敏环境/操作/能力摘要并标记 `readOnly=true`，不包含 API Key、userSig、Cookie、代理密码或完整 URL。
- Chat 上下文覆盖环境、指纹、代理、内核、操作、设置和 runtime 诊断摘要；目录、完整代理 URL、CDP endpoint 与敏感字段不会发送给模型。
- Agent 先生成结构化计划，执行要求显式批准、`envId`、`expectedState` 和 `idempotencyKey`；白名单动作复用 Manager operation，重复 key 返回原执行结果。
- Agent 执行先在 SQLite 预留 key 再调用工具；重启后已完成的相同计划可回放，不同计划复用同一 key 会被拒绝，中断/不确定执行 fail closed，禁止同 key 再次执行。
- 启动/停止状态语义明确写入结果：accepted/starting 不等于 ready，必须等待 SDK callback 或 reconcile。
- `npm run ai:smoke` 只输出模型名、只读标志和回答长度，不输出模型回答或密钥。
- DLL MCP adapter 严格完成 Streamable HTTP lifecycle，只允许 ready 环境执行 `browser_state(get)` 与 `tabs(list/current)`；调用有 operation，URL 降为 origin，响应脱敏。
- `npm run e2e:environment` 在设置 `BROSDK_EMBEDDED_PORT` 时会实际调用 `/sdk/v1/mcp/env/{envId}` 的 `tabs(list)`，不再只检查 TCP 监听。
- 2026-07-25 使用真实测试配置完成 DeepSeek smoke 和环境 E2E；报告确认 `readySource=sdk_callback`、`runtimeEvaluateVerified=true`、`embeddedMcpToolVerified=true`、`manualCloseVerified=true`。
- Dashboard AI/MCP 页面通过 1440x900 与 390x844 Playwright 检查：导航与 Chat/Agent 模式切换有效、预览态写按钮保持禁用、控制台无应用错误、页面无横向溢出。
