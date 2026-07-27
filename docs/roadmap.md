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
| 7. 打包发布 | P1 | 已完成 | Windows 便携包、Tauri 资源、图标、WebView2、签名准备、升级策略 |
| 8. 跨平台 | P2 | 基础 adapter 已完成，等待平台动态库 | macOS/Linux 路径、UDS、系统 keyring、能力报告和交叉编译完成 |
| 9. AI Agent | P2 | 已完成 | DeepSeek/OpenAI 兼容 Chat、受控 Agent、持久化幂等；MCP 能力在阶段 20 扩展 |
| 10. 环境创建交互收敛 | P0 | 已完成 | 创建环境只要求选择代理和内核版本，真实 DLL 创建/删除及镜像对账通过 |
| 11. 远端事实源与 MCP 双层路由 | P0 | 已完成 | 环境配置以 SDK 服务端为准，本地仅保留可丢弃缓存；DLL 全局与单环境 MCP 已接通并通过真实验收 |
| 12. 首次初始化与环境工作台 | P0 | 已完成 | API Key 安全初始化、环境详情、远端指纹、运维动作和真实生命周期 E2E 完成 |
| 13. 多环境工作流 | P0 | 已完成 | 批量启停、受限远端元数据编辑、跨环境指纹对比和双环境 E2E |
| 14. Dashboard envId 身份与 E2E | P0 | 已完成 | envId 唯一主键、同名环境可辨识交互和桌面/移动浏览器 E2E |
| 15. 操作中心与故障恢复 | P0 | 已完成 | 按 envId 追踪 operation，取消/重试策略与真实执行能力一致 |
| 16. AI 配置与环境上下文 | P0 | 已完成 | Dashboard 安全配置 AI Provider，并向用户和模型提供边界明确的环境运行摘要 |
| 17. CDP 运行态多源回填 | P0 | 已完成 | callback/getEnvInfo/BrowserInfo 三路回填，端口 0 保持内部控制通道 |
| 18. Windows 安装交付闭环 | P0 | 已完成 | NSIS、便携包、可选 MSI、哈希清单、安装版 Dashboard E2E 与静默卸载 |
| 19. AI 会话与 Agent 执行可靠性 | P0 | 已完成 | 会话历史、新建/清空、envId 目标校正、真实状态前置条件和接口覆盖审计 |
| 20. 多环境 Agent 与完整单环境 MCP | P0 | 已完成 | 运行时全工具目录、会话级自动执行和双环境 Agent/MCP E2E；路由已在阶段 25 升级为全局 `env.*` |
| 21. Windows Runtime Host 后台化 | P0 | 已完成 | sdk-host 全部启动路径不创建终端窗口，保留 IPC 与诊断输出捕获 |
| 22. Windows 桌面生命周期 | P0 | 已完成 | Dashboard/Host GUI subsystem、关闭到托盘、托盘恢复/退出和 release PE 门禁 |
| 23. 环境启动回调进度 | P0 | 已完成 | DLL percent/statusName 进入 operation 与环境表，真实 callback 和响应式 E2E 通过 |

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

目标：建立本地 operation/运行状态来源和操作模型；阶段 11 再把环境配置收敛为服务端事实源。

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
- 环境列表：搜索、状态筛选、服务端名称、远端 envId、启动/停止按钮。
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

## 12. 阶段 10：环境创建交互收敛

目标：补齐远端环境创建能力，并把普通用户的创建流程收敛为“选择代理 + 选择内核版本”。

产品原则：

- 代理可选；不选择时使用本机网络。
- 内核版本必选；默认预选最新的本地可用版本。
- `customerId`、环境名称和指纹细项不由普通用户填写，也不由 Manager 伪造。
- 创建请求省略环境名称；列表在远端名称为空时使用 `环境 <envId>` 作为本地展示回退。
- 语言、时区、UA、Canvas、WebGL 等指纹参数不在创建界面暴露，由后端根据代理 IP 和 SDK 默认策略生成。
- 高级指纹编辑继续保留在独立“指纹”模块，不混入环境创建主流程。

任务：

- 为 `sdk_env_create` 补齐 FFI、runtime host、Manager operation 和 Tauri command 链路。
- 定义 `EnvironmentCreateInput`，只接受 `proxyProfileId` 和 `kernelId`。
- Manager 从受保护的代理 profile 恢复完整代理 URL，仅在调用 DLL 的瞬间使用；operation、事件和错误日志只记录 profile id 与脱敏摘要。
- Manager 校验内核已安装且可用于当前平台，构造服务端 DTO 的顶层 `kernel` 与 `kernelVersion`。
- Manager 省略 `customerId`、`envName` 和 `finger`，由 userSig 上下文与服务端默认策略处理；调用成功后立即同步环境镜像。
- Dashboard 环境页增加紧凑创建面板；主表单只展示代理与内核两个选择控件。
- 无本地可用内核时明确引导到内核页，不提交不完整请求。

风险：

- `sdk_env_create` 是远端写操作，自动测试必须受 `BROSDK_E2E_ALLOW_MUTATION=1` 门禁。
- 代理密码必须经过系统密钥库读取，不能进入 SQLite operation request、Manager event、前端 snapshot 或日志。
- DLL 返回成功不等于本地镜像已更新；创建结果必须再执行 `env_page` 同步并对账。
- 后端可能调整默认字段要求；Manager 应保留稳定的最小请求构造测试，并把后端错误作为脱敏失败 operation 展示。

验收：

- 普通创建界面只有代理和内核版本两个业务参数。
- 不选择代理也能提交；没有可用内核时不能提交。
- operation request 不含代理 URL、密码、API Key、userSig 或完整后端响应。
- 创建成功后环境出现在列表中，并产生可追踪的 `environment.create` operation。
- `npm run check`、`npm test`、`npm run build` 通过。
- 1440x900 与 390x844 下创建面板无重叠、无横向溢出，键盘焦点和错误状态可用。
- 真实创建/删除验证只在 mutation 门禁开启时运行，并在测试后清理创建的环境。

Manager/Runtime Host 子阶段完成（2026-07-26）：

- 已按 `doc.json` 的 `browser-third-server` 契约和 `browser-open-server/modules/open/router/browser` 源码复核 `FingerReqDto`、认证与响应结构。
- 已补齐 `sdk_env_create`/`sdk_env_destroy` FFI、HostCommand、Manager operation、Tauri 创建命令和本地镜像写入/删除。
- `getUserSig` 固定发送 `role=user`；创建请求只含顶层 `kernel`、`kernelVersion` 和可选 `proxy`。
- 后端响应必须满足 `code=200` 且包含 `data.envId`；失败消息先脱敏再进入 operation。
- `cargo test -p domain -p sdk-ffi -p sdk-host -p manager` 与 `cargo check --workspace --all-targets` 通过。

Dashboard 交互子阶段完成（2026-07-26）：

- 环境工具栏新增“新建环境”，展开同页紧凑创建带，不引入向导或高级参数。
- 代理默认“本机网络”，内核默认最新的当前平台本地已安装版本；仅展示 Manager 可接受的 Chrome/Firefox/Chromium/Broium core。
- 无可用内核时禁用选择并提供“前往内核”；提交期间锁定表单，业务失败保留选择，成功后刷新并选中新环境。
- 新增 Vitest + Testing Library，覆盖双字段边界、默认排序、代理/内核 ID 提交、无内核跳转、按钮与 Esc 取消。
- Browser 插件在 1440x900 与 390x844 完成交互和视觉验收；两种视口均无页面级横向溢出，浏览器控制台无 warning/error。

真实 E2E 子阶段完成（2026-07-26）：

- 新增 `npm run e2e:environment-create`；缺少 API Key 或 `BROSDK_E2E_ALLOW_MUTATION=1` 时安全跳过。
- PowerShell 包装脚本使用唯一的系统临时 Manager 数据目录，复用现有 SDK workDir，并在退出时清理本地测试数据。
- E2E 自动选择最新可用本地内核，以本机网络创建环境，确认 operation 只保存两个 ID 字段，并验证创建结果进入本地镜像。
- 真实 DLL 验收使用 `chrome-134-windows-x86_64` 通过；创建后立即删除，再执行 `env_page`，测试前后账号环境数均为 1，补偿清理成功。

## 13. 阶段 11：远端事实源与 MCP 双层路由

目标：消除“SQLite 是环境事实来源”的歧义，并完整接入 DLL 已提供的全局管理 MCP 与单环境 BrowserOS MCP。

产品原则：

- API Key 只用于 `sdk_get_user_sig(role=user)`，后续环境读取和写入由初始化后的 DLL 使用 userSig 访问 SDK 服务端；服务端是环境配置唯一事实来源。
- SQLite 环境表只允许保存服务端响应的脱敏、可删除缓存。缓存不能产生本地专属环境名称、标签或覆盖服务端字段。
- `sdk_env_page` 必须分页拉取完整集合。只有所有页都成功时才原子替换缓存；任一页失败时保留上一份缓存并明确标记 stale。
- generation、reqId、CDP 和 ready/stopped 是当前设备的浏览器进程事实，作为本地运行态叠加在远端缓存上，不视为远端环境配置。
- DLL 全局 MCP `/sdk/v1/mcp` 同时用于管理读取和 `env.*` 浏览器操作；兼容单环境 endpoint 不再用于本项目的新调用。
- 全局 MCP 的 create/update/destroy/open/close 等写工具不直接暴露给 Dashboard，继续复用 Manager operation、状态校验、审批和补偿逻辑。

任务：

- 把默认环境分页从固定前 20 条改为完整分页读取，解析服务端 `data.list` 与 `data.total`，设置页数和条数上限。
- 增加环境缓存元数据：来源、fresh/stale/empty 状态、最后成功时间、最后尝试时间、脱敏错误和缓存条数。
- 成功同步时事务替换远端缓存并删除服务端已不存在的记录；失败时不写入半套结果。
- 清除并停止使用 `local_label`/`tags_json` 等本地环境覆盖字段；Dashboard 只显示服务端名称和 envId。
- 启动时在 API Key 可用且缓存过期时自动刷新；网络失败允许只读显示 stale 缓存，但 mutation 仍由 SDK 服务端最终校验。
- 为 MCP client 增加全局 endpoint、`tools/list` 发现和严格 session lifecycle 复用。
- Manager 分别维护全局只读工具策略与单环境工具策略，并核对 DLL 实际 advertised tools；响应继续脱敏，URL 继续降为 origin。
- Dashboard MCP 页面增加“全局/单环境”作用域、动态工具状态与安全参数输入，不直接访问 DLL 端口。

已核对契约：

- DLL 源码的全局 MCP 已包含 `sdk.health`、`sdk.info`、`env.list/resolve/get/create/update/destroy`、`browser.open/close/cleanup/status/install`、`task.list/get` 和 `mcp.endpoint`。
- DLL 单环境 MCP 已包含 `browser_state`、`tabs`、`snapshot`、`diff`、`read`、`grep`、`screenshot`、`pdf`、`wait` 以及导航/交互类工具。
- 因此缺口不在 DLL，而在本项目当时只接入兼容单环境 endpoint 且只允许 `browser_state(get)`、`tabs(list/current)`；阶段 25 已迁移到全局 `env.*` catalog。
- `brostu` 与当前 DLL 一样通过 init `port` 启用内嵌服务，并直接使用 `PageEnv/GetEnvInfo/CreateEnv/UpdateEnv/BrowserInfo`，没有额外的本地环境事实库要求。

验收：

- 超过一页的服务端环境能完整进入缓存；同步后已删除的远端环境不会残留。
- 中途分页失败不会覆盖上一份完整缓存，Dashboard 明确显示 stale 而不是伪装为最新。
- SQLite 不再保存或展示本地环境名称/标签覆盖，诊断包说明环境数据为服务端缓存。
- 全局 MCP 能完成 initialize、tools/list、只读 `sdk.health`/`env.list` 和 session DELETE。
- ready 环境能发现并调用允许的单环境只读工具；非 ready 环境和未批准的 mutation 被 Manager 拒绝。
- `npm run check`、`npm test`、`npm run build`、真实 SDK/MCP E2E 和 1440x900/390x844 浏览器验收通过。

## 14. 阶段 12：首次初始化与环境工作台

目标：让未配置客户端从 API Key 激活开始形成完整闭环，并把 Dashboard 重构为面向多环境日常操作的工作台。

产品原则：

- 首次启动只要求 API Key。客户端调用 `sdk_get_user_sig` 时固定 `role=user`；`userSig` 只存在于隔离 Host/DLL 生命周期中，不保存、不展示。
- API Key 使用平台安全存储：Windows 为 DPAPI，macOS/Linux 为系统 keyring。SQLite、前端 snapshot、operation、事件、诊断包和日志都不得包含密钥、摘要或尾号。
- 环境变量 `BROSDK_API_KEY` 只作为自动化测试和受管部署覆盖项；普通桌面用户使用安全存储。
- 初始化必须完成“验证 API Key -> 启动隔离 Host -> DLL 初始化 -> 全量同步环境”后才能进入主工作区。失败保持在初始化页，并返回脱敏、可重试的错误。
- 更换或移除 API Key 前先停止 Host；账号边界变化时清空远端环境缓存、环境详情和运行态，防止显示上一账号的数据。
- `/api/v2/browser/*` 是 API Key 认证的环境管理接口；`/api/v2/sdk/*` 是 DLL 持有 userSig 后使用的内部接口。Dashboard 不直接调用 `sdk/*` HTTP 接口。
- 服务端环境是指纹、代理和内核配置的事实来源。本地可保存脱敏详情缓存以支持离线只读，但不得覆盖服务端配置。

已核对能力矩阵：

| 用户任务 | browser API | DLL C ABI | Dashboard 目标 |
| --- | --- | --- | --- |
| 首次认证 | `getUserSig` | `sdk_get_user_sig`、`sdk_init` | API Key 初始化页、安全存储、重试/更换 |
| 环境列表 | `page` | `sdk_env_page` | 全量同步、搜索、状态筛选、缓存新鲜度 |
| 环境创建 | `create` | `sdk_env_create` | 只选择代理和内核版本 |
| 环境详情/指纹 | `info` | `sdk_env_getinfo` | 按单环境读取并结构化展示，不再以本地 JSON 档案为主 |
| 环境元数据/指纹更新 | `updateEnv`、`update` | `sdk_env_update` | 后续专家操作；普通工作流不暴露完整 DTO |
| 环境删除 | `destroy` | `sdk_env_destroy` | 停止态二次确认、同步缓存 |
| 浏览器启停 | - | `sdk_browser_open`、`sdk_browser_close` | 行级与详情区操作、callback ready 语义 |
| 运行状态 | - | `sdk_browser_info` | 对账、CDP/进程状态 |
| 指纹验证 | - | `sdk_browser_env_check` | ready 环境打开内置检查页 |
| 页面取证 | - | `sdk_browser_snapshot` | 诊断入口，不混入普通启动流程 |
| 本地缓存清理 | - | `sdk_browser_cleanup` | 停止态环境清理与内核下载缓存清理 |
| 网络诊断 | - | `sdk_network_diagnostics`、`sdk_system_proxy_diagnostics` | 代理页与环境故障诊断 |
| Cookie/Storage | DLL 内部走 `sdk/getCookie|getStorage|upCookie|upStorage` | callback、`sdk_env_get_cookies` | 默认由 DLL 生命周期管理，不在首轮暴露原始敏感数据 |
| MCP | - | DLL 内置全局/单环境 MCP | Manager 只读白名单与显式 envId 路由 |

子阶段：

1. 安全初始化：新增 API Key 配置、验证、删除命令；Host 仅通过子进程环境接收密钥；无凭据时不自动启动 Host。
2. 环境工作台：统一初始化状态、列表、选中环境详情、启停、删除、同步和对账；浏览器预览态只用于 UI QA，并明确不可执行本机动作。
3. 远端指纹查看器：按选中环境读取 `sdk_env_getinfo`，展示平台、内核、UA、语言/时区、屏幕、Canvas/WebGL/WebRTC 等脱敏结构；ready 时提供检查页入口。
4. 运维动作：补充单环境详情刷新、本地环境缓存清理和页面快照诊断；高风险更新与 Cookie/Storage 原始数据继续留在受控后续阶段。
5. E2E：使用临时注入的 API Key 验证首次初始化、重启后安全存储读取、环境同步、详情/指纹、启动 ready、检查页、停止与缓存隔离。

验收：

- 全新数据目录启动时只显示初始化页；API Key 不合法时不能进入主界面，合法时完成同步并进入环境页。
- 重启客户端无需再次输入 API Key，磁盘扫描和 SQLite 检查找不到明文密钥；snapshot 只返回 `present/source`。
- 移除或更换 API Key 后 Host 已停止，上一账号的环境列表、详情、CDP 和 operation 运行态不再显示。
- 用户能从环境列表启动/停止环境、查看服务端指纹摘要，并在 ready 后打开 DLL 指纹检查页。
- `npm run check`、`npm test`、`npm run build` 和 Rust workspace 测试通过。
- 真实 E2E 通过；1440x900 与 390x844 首次初始化、环境和指纹页面无重叠、无横向溢出、无应用控制台错误。

## 15. 阶段 13：多环境工作流

目标：让用户能高效管理多组远端指纹环境，同时保持 SDK 服务端事实源、单环境 generation 和敏感数据边界。

产品原则：

- 多选只作用于当前服务端环境镜像，不产生本地环境副本。筛选变化或同步后，已不存在的选择必须自动清除。
- DLL 虽支持一次传入多个 `envs`，现有 operation/generation 是单环境状态机；Manager 批量动作必须编排独立子 operation，不能用一个 operation 复用多个环境的 callback。
- 每批最多 20 个环境，执行前完整校验 envId、当前状态和重复项。启动只接受 stopped/failed，停止只接受 ready/starting，避免一半执行后才发现参数错误。
- `doc.json` 与服务端 `FingerEnvUpdateReq` 只把 `envName`、`serial`、`proxy`、`bridgeProxy`、`customerId` 定义为元数据更新。普通编辑器首轮只开放名称和序号；不把 customerId 当分组，不暴露代理明文或完整 DTO。
- 服务端仍是名称、序号和指纹配置的唯一事实来源。更新成功后必须重新全量同步并按需刷新单环境详情；SQLite 只缓存脱敏结果。
- 指纹对比最多同时展示 4 个环境，只比较已脱敏的远端摘要；空字段显示未知，不推断“绝对唯一”或“安全”。

子阶段：

1. 批量生命周期：环境表增加多选、全选当前结果、选择计数和批量启动/停止；Manager 编排独立子 operation 并返回 accepted/failed 汇总。
2. 远端元数据：补齐 `sdk_env_update` FFI/Host/Manager/Tauri 链路，在 stopped 环境详情中编辑 1-32 字符名称和最长 64 字节序号。
3. 指纹对比：从服务端详情缓存选择最多 4 个环境，按浏览器/系统、代理、设备和指纹表面逐行对比，并标记相同/不同/未知。
4. E2E：创建两个临时环境，更新元数据，批量启动并等待各自 callback ready，验证指纹详情/对比数据，再批量停止、本地清理、服务端删除和全量对账。

明确不做：

- 不增加本地环境标签、分组或名称覆盖；服务端当前没有对应事实字段。
- 不在普通工作流开放 customerId、完整 Finger DTO、Cookie/Storage、代理/桥代理明文更新。
- 不提供无二次确认的批量删除，也不让 Dashboard 直接调用 DLL 原生 batch 或 MCP mutation。

验收：

- 批量动作中每个环境都有独立 operation、generation、callback 和最终状态；单项失败不会伪装成整批成功。
- 同步、筛选、详情侧栏和多选控件不会互相覆盖；桌面与 390px 移动宽度无横向页面溢出。
- 元数据更新 operation request 只含 envId、envName、serial，不含代理、customerId、API Key、userSig 或原始服务端响应。
- 指纹对比不读取本地 profile，不显示 Cookie、Storage、DEK、路径、完整代理密码或未知原始字段。
- `npm run check`、`npm test`、`npm run build`、真实双环境 E2E 和应用内浏览器 QA 通过。

## 16. 阶段 14：Dashboard envId 身份与 E2E

目标：把 `envId` 固化为 Dashboard 所有环境关联和操作的唯一主键，并建立可重复执行的完整浏览器 E2E，确保同名多指纹环境不会被错误选择或串联。

产品原则：

- `envId` 是 SDK 服务端分配的不可变唯一标识；环境名称是可编辑展示字段，允许重复，不能用于 React key、选择状态、详情绑定或 mutation 参数。
- Manager SQLite 的 `environments.env_id`、`runtime_snapshots.env_id` 和 `environment_details.env_id` 继续以主键约束；Dashboard snapshot 必须拒绝空或重复 envId，并拒绝重复或悬空的详情绑定。
- 表格、批量操作、指纹详情/对比、MCP 环境选择和代理绑定都以 envId 作为 value；名称相同时，界面和可访问名称必须同时展示 envId。
- 浏览器预览只提供确定性的脱敏测试数据，不执行 Tauri/DLL mutation；真实生命周期继续由隔离 Manager runner 验收。

子阶段：

1. 身份契约：新增 Dashboard snapshot 身份校验和统一环境标签，覆盖重复名称、重复/空 envId、重复/悬空详情绑定。
2. 交互收敛：为环境表、指纹列表/对比、MCP 和代理绑定补齐 envId 可见文本、可访问名称和稳定测试标识。
3. Dashboard E2E：新增 Playwright 配置与同名环境预览场景，在桌面和 390px 移动视口覆盖 envId 搜索、独立多选、详情切换、指纹对比及 MCP 环境选项。
4. 真实复验：双环境 SDK runner 显式断言并报告两个临时 envId 唯一，重新完成创建、批量启停、详情刷新、清理、删除和全量对账。

验收：

- 两个相同名称、不同 envId 的环境能独立选择，批量计数、详情和指纹列不会串联；所有 mutation 参数仍只发送目标 envId。
- snapshot 出现空/重复 envId、重复详情绑定或绑定不存在的环境时，Dashboard 明确失败，不以名称或数组位置兜底。
- `npm run e2e:dashboard` 在系统 Chrome 上自动启动独立 Vite 服务，桌面和 390px 项目全部通过，不依赖已打开的开发服务器。
- 应用内浏览器完成页面身份、非空、framework overlay、console、交互和响应式检查；截图能力不可用时必须如实记录。
- `npm run check`、`npm test`、`npm run build`、真实双环境 E2E、敏感信息扫描和残留进程检查全部通过。

明确不做：

- 不把环境名称强制唯一，不引入本地别名或名称到 envId 的反向猜测。
- 不在浏览器预览或 Playwright 中调用真实 SDK mutation，不把 API Key、userSig、envId 测试账号数据或 DLL 原始响应写入测试产物。

## 17. 当前状态

阶段 0-20 已完成。Dashboard 已形成以 envId 为唯一主键的多环境工作台、具备会话历史和手动/自动执行语义的受控 AI、动态 DLL MCP 能力，以及经过真实安装 E2E 的 Windows 交付链路；阶段 25 已把 MCP 路由升级为全局 `env.* + arguments.envId`。环境配置仍以 SDK 服务端为事实来源，本地只保存可删除的脱敏缓存和运行态。接口覆盖不再用单一“完成”描述，详见 [interface-coverage.md](interface-coverage.md)。

阶段 14 envId 身份子阶段完成（2026-07-26）：

- Dashboard snapshot 入口新增 fail-closed 身份守卫：环境 envId 必须非空且唯一，详情绑定 envId 必须非空、唯一并引用当前环境镜像；环境名称明确允许重复。
- 统一环境展示/可访问标签为“名称 + envId”。环境表、批量选择、启停按钮、指纹详情/对比、MCP 和代理绑定继续只以 envId 作为 key、value 和操作参数。
- 同名预览场景提供两个不同 envId 的 ready 环境；环境行、指纹选择器和对比列带稳定 envId 属性，不以数组位置或名称定位。
- 新增 5 项身份与同名交互测试；Dashboard 组件测试由 28 项增至 33 项，TypeScript 检查与 production build 通过。组件测试还修正了 MCP 有 ready 环境时仍显示无效空选项的问题。

阶段 14 Dashboard E2E 子阶段完成（2026-07-26）：

- 新增 `npm run e2e:dashboard`，使用项目内 Playwright、系统 Chrome 和严格独占的 `127.0.0.1:1430` Vite 服务；失败截图与 trace 只写入已忽略的 `target/playwright`。
- 3 条身份流程分别覆盖环境表 envId 搜索/独立多选/详情、指纹两列绑定、MCP 与代理绑定下拉；在 1440x900 和 390x844 两个项目执行，共 6 项测试。
- 每条测试都校验页面 title/H1、无 Vite overlay、无页面级横向溢出和 console/page error；预览态 SDK mutation 保持禁用。
- 应用内浏览器复核同名环境多选计数、envId 精确搜索、两列指纹对比和 MCP 两个 envId 选项，console 无 warning/error。当前浏览器后端没有 screenshot 方法，视觉与移动视口证据由 Playwright 项目提供。
- 浏览器 Playwright 明确只覆盖无后端预览，不作为 SDK mutation 证据；Windows 真实 Dashboard 启停由 `npm run e2e:dashboard:desktop` 驱动 Tauri 环境表按钮并等待 ready/stopped 状态反转。

阶段 14 Dashboard 功能 E2E 回归完成（2026-07-26）：

- 环境列表在浏览器预览中明确显示“浏览器预览 · 只读”，启动/停止等真实 mutation 继续禁用，避免把没有 Tauri bridge 的预览当作实际客户端。
- `npm run e2e:dashboard:desktop` 已通过真实 Tauri UI 自动化，直接定位带环境标识的启动/停止控件，完成 start -> ready -> stop -> stopped；报告不输出 envId、名称、CDP 或页面内容。
- Dashboard Playwright 回归共 8 项，组件 33 项，Rust workspace 81 项；`npm run check`、Clippy 和 production build 通过。

阶段 14 指纹详情展示回归完成（2026-07-26）：

- 指纹详情改为固定白名单摘要，不再自动追加“其它”字段；嵌套对象和 JSON 编码对象只显示“已配置”，内部键值不会进入正文或 tooltip。
- Canvas、WebGL、WebRTC、AudioContext、字体指纹、Client Rects、语音和媒体设备按服务端 DTO 枚举显示可读模式；浏览器、语言、时区、屏幕、CPU、内存和 WebGL 厂商/渲染器继续显示实际摘要。
- MAC、WebRTC IP、字体列表、端口、设备名、硬件内部开关和未知扩展字段默认隐藏；跨环境对比复用同一安全格式化器。

阶段 14 真实 SDK 与最终验收完成（2026-07-26）：

- 双环境 runner 新增 envId 非空与唯一性断言，成功报告只增加 `uniqueEnvironmentIds=true`，不输出两个真实 envId。
- 真实 DLL 链路再次创建两个临时环境，完成独立元数据更新、批量启停、callback ready、远端指纹详情、本地清理和服务端删除；账号环境数前后均为 1，清理 2/2。
- Dashboard 33 项组件测试、Playwright 6 项、Rust workspace 81 项测试、TypeScript、Rustfmt、Clippy 和 production build 全部通过；退出后无临时 Manager 数据目录、测试端口或 `sdk-host` 残留。

阶段 13 批量生命周期子阶段完成（2026-07-26）：

- Domain/Manager/Tauri 新增批量 start/stop 编排，限制 1-20 个唯一 envId，并在任何 SDK 调用前校验所有环境状态。
- 批量请求按顺序复用单环境 operation；每个环境继续独立推进 generation、accepted、callback ready/stopped 和错误状态。
- 环境表增加复选框、当前结果前 20 个全选、选择计数、可启动/停止计数与清除动作；stopping/unknown 等过渡态不会显示成可启动。
- Dashboard 22 项、Rust workspace 71 项测试、Clippy、production build 通过；应用内浏览器确认选择/清除、预览禁用态和 console 无错误。

阶段 13 远端元数据子阶段完成（2026-07-26）：

- FFI/Runtime Host/Manager/Tauri 补齐 `sdk_env_update`；Manager 只接受 stopped 环境和 `envId/envName/serial`，名称按最多 32 个 Unicode 字符、序号按最多 64 个 UTF-8 字节校验。
- 更新成功必须同时满足 DLL 调用成功、后端 `code=200`、响应 `data.envName/data.serial` 与请求完全一致；operation request 不含 proxy、customerId、API Key、userSig 或原始响应。
- 更新后重新拉取完整环境分页并刷新单环境详情。真实部署的旧版 `getEnvInfo` 会返回空序号，因此绑定摘要采用详情非空值优先，并以服务端分页或严格校验过的更新响应补全空值；SQLite 仍只是可删除缓存。
- 环境详情增加停止态内联编辑器，运行态禁用；组件测试覆盖提交、状态限制、Unicode 字符和 UTF-8 字节边界。
- Dashboard 25 项、Manager library 48 项和创建 E2E binary 3 项测试、Clippy、production build 通过。真实临时环境完成创建、名称/序号更新、镜像核对、本地清理、服务端删除和最终对账，账号环境数恢复到 1；桌面与 390x844 DOM 完整且控制台无 warning/error。

阶段 13 指纹对比子阶段完成（2026-07-26）：

- 指纹页增加详情/对比分段模式；对比模式默认带入当前环境，以复选框选择最多 4 个服务端环境，环境同步后自动剔除已不存在的选择。
- 对比只读取 `environmentBindings` 的固定白名单字段，覆盖环境概要、浏览器与系统、设备和指纹表面；不展示动态原始字段。任一环境缺值时标为未知，全部有值时才判断相同或不同。
- “刷新所选”并发提交每个环境的独立详情 operation，全部返回后只刷新一次 Dashboard snapshot；浏览器预览继续禁用 SDK mutation。
- Dashboard 28 项组件测试与 production build 通过，覆盖两环境比较、相同/不同/未知、4 环境上限和所选环境刷新。应用内浏览器在桌面与 390x844 验证两环境对比表完整、控制台无 warning/error；移动端表格使用局部横向滚动容器，当前浏览器后端未提供截图/几何接口。

阶段 13 双环境最终 E2E 完成（2026-07-26）：

- 新增独立 `npm run e2e:multi-environment` runner；隐藏读取 API Key，使用唯一系统临时 Manager 数据目录，并由 wrapper 显式开启 mutation 门禁。
- 真实账号创建两个本机网络环境并分别更新名称/序号；批量启动和停止均返回两个不同 operation ID、匹配两个环境且全部 accepted，证明每个环境保持独立 operation/generation。
- 两个环境均等待到 callback ready，再分别刷新 `sdk_env_getinfo` 并确认两份非空脱敏指纹缓存；随后全部停止、本地数据清理、服务端删除并执行最终 `env_page` 对账。
- runner 失败报告只输出阶段和清理数量；失败路径会先 reconcile，必要时停止环境，再补偿本地清理与服务端删除，不打印 envId、名称、序号、页面内容、API Key、userSig 或 DLL 原始响应。
- 真实结果为测试前后环境数 1、临时环境 2、清理 2/2，全部生命周期与指纹布尔项为 true；退出后无残留 `sdk-host` 或临时测试目录。Dashboard 28 项、Rust workspace 80 项测试、Clippy 和 production build 全部通过。

阶段 12 安全初始化子阶段完成（2026-07-26）：

- 无凭据时 Manager 不再启动 Host，Dashboard 首屏只显示 API Key 初始化；成功配置后直接进入环境工作台。
- API Key 候选通过子进程环境注入隔离 Host，完成 `getUserSig(role=user) -> init -> 完整 env_page` 后才使用平台安全存储持久化；Manager IPC、SQLite 和 snapshot 不含密钥。
- 环境变量仍作为测试/受管部署覆盖项；设置页只读显示该来源。安全存储来源支持更换和移除，账号边界变化会清除环境、详情、运行态、operation、event、Agent 记录和旧环境绑定。
- 新增 `npm run e2e:credential`，使用唯一临时数据目录验证 DPAPI 密文、Manager 重建恢复和移除隔离；真实结果为 1 个环境、`encryptedAtRest/restartLoaded/accountStateCleared=true`。
- `npm run check`、workspace Rust 测试、Dashboard 13 个组件测试和 production build 通过；1440x900 与 CDP 390x844 视觉/几何检查无溢出，控制台无 warning/error。

阶段 12 环境/远端指纹工作台子阶段完成（2026-07-26）：

- Manager 新增带 `envId` 的单环境详情 operation，调用 `sdk_env_getinfo` 后校验业务码；原批量兼容路径也补齐业务码校验，避免错误响应被当作详情缓存。
- `environment_details` 只写入递归脱敏后的 `finger`、结构化掩码代理、`browser` 内核和非敏感元数据。真实服务端响应中的 Cookie、Storage、上传路径、DEK、token、secret 和代理密码均被排除。
- 环境详情侧栏展示实际内核、系统、代理、语言、时区、屏幕、序列号和 CDP；远端指纹页按环境组织浏览器/系统、设备、Canvas/WebGL/WebRTC 等字段，未知非敏感字段进入“其它”，不再提供不参与环境创建/启动的本地 JSON 编辑主流程。
- 浏览器 demo 新增 `?preview=workspace&page=<page>` 稳定 QA 入口，本机操作保持禁用。移动底部 9 个入口改为等分栅格，390px 下全部可达。
- 真实凭据 E2E 在唯一临时数据目录完成聚焦详情读取，`focusedDetailLoaded=true`；DPAPI、重启恢复和账号清理继续通过。Dashboard 16 个测试、workspace 65 个 Rust 测试、Clippy、production build、1440x900 与 390x844 视觉/几何和控制台检查通过。

阶段 12 运维动作子阶段完成（2026-07-26）：

- Tauri/Manager 新增环境删除、本地数据清理和页面诊断命令；所有 operation 显式绑定 `envId`。启动/停止、刷新、检查、诊断、清理和删除集中到环境详情区。
- `sdk_browser_cleanup({envs:[envId]})` 只清理 stopped 环境的本地 user-data-dir，响应在 Manager 压缩为 deleted/notFound/failed/deferred，删除本地路径和环境 ID；`sdk_env_destroy` 单独负责删除服务端环境。
- 页面诊断只对 ready 环境调用 `sdk_browser_snapshot`，固定 `includeHtml=false`、`includeScreenshot=false`、`emitEvents=false`、`maxPages=32`。Manager 只返回页数、失败数、状态和 origin，不返回 HTML、截图、标题、URL 路径/query、snapshot/target/session ID。
- 清理和删除均有独立行内确认；运行态禁止这两个操作，停止态禁止页面诊断和指纹检查。Dashboard 19 个组件测试与 workspace 67 个 Rust 测试全部通过。
- 真实临时环境 E2E 完成 create -> local cleanup -> destroy -> env_page 对账，`localDataCleanupSucceeded/cleanupSucceeded/destroyReconciled=true`，测试前后环境数均为 1。完整 ready 页面诊断调用并入最终生命周期 E2E。

阶段 12 最终生命周期 E2E 完成（2026-07-26）：

- runner 使用隐藏 API Key、唯一临时 Manager 数据目录和自动分配的 DLL MCP 端口；账号只有一个环境时无需手工提供 envId。
- 真实环境完成 `browser_open -> callback ready -> CDP evaluate -> env_check 新标签 -> 安全 snapshot -> MCP tabs/read -> browser_close -> stopped`。
- 检查页原始 target/session/CDP 响应在 Manager 内压缩为 `{opened,newTab,source}`；页面诊断只包含白名单字段，真实结果页数为 3。
- 报告为 `status=passed`、`fingerprintCheckOpened/pageDiagnosticVerified/environmentStopped=true`、MCP 广告 18 个/放行 7 个，不含环境 ID、页面 URL、正文、API Key 或 userSig。
- Dashboard 19 项、Rust workspace 69 项测试、Clippy、production build 和应用内浏览器 DOM/console QA 全部通过。

阶段 11 远端缓存子阶段完成（2026-07-26）：

- `sdk_env_page` 默认请求 200 条并按 `data.total` 拉取完整分页，按 envId 去重，限制最多 500 页/100000 条；异常总数、重复页和提前空页均 fail closed。
- schema 升级到 v5，新增 `environment_cache_status`；全页成功后单事务替换缓存并删除远端缺失项，失败保留旧缓存并记录脱敏 stale 原因。
- `EnvironmentRecord` 和 Dashboard 删除本地名称/标签覆盖；迁移保留旧列以兼容数据库，但启动即清空且后续不再读取。
- API Key 可用时首次 snapshot 自动刷新；Dashboard 环境工具栏显示服务端、缓存或待同步状态，诊断包与 Manager smoke 同步输出缓存元数据。
- `npm run check`、`npm test`、`npm run build` 通过；真实 `getUserSig(role=user) -> init -> env_page` Manager smoke 使用独立临时数据库通过，返回 fresh/1 个服务端环境且测试目录已清理；1440x900 与 390x844 环境页无重叠、无页面级横向溢出且控制台无应用错误。

阶段 11 Manager MCP 子阶段完成（2026-07-26）：

- MCP client 复用严格 Streamable HTTP session 生命周期，`tools/list` 同时解析说明和 read-only/destructive annotations；阶段 25 后只连接全局 `/sdk/v1/mcp`。
- Manager API 使用显式 `global`/`environment` scope；全局允许 `sdk.health/info`、`env.list/resolve/get`、`browser.status`、`task.list/get`、`mcp.endpoint`，环境级允许 7 个带参数上限的只读工具。
- 单环境发现/调用要求缓存中的环境为 ready；全局 mutation、页面导航/交互/脚本/文件工具、全页截图和越界参数均在建立 MCP session 前拒绝。每次发现和调用都有独立 operation，响应继续脱敏并把 URL 降为 origin。
- domain、MCP client、Manager 和 Tauri command 已接通动态发现；定向测试通过。真实隔离 smoke 发现 DLL 广告 16 个全局工具、Manager 放行 9 个，协议为 `2025-11-25`，并成功调用 `sdk.health`、`env.list`、`mcp.endpoint`；临时数据库与端口均已清理。

阶段 11 Dashboard 与单环境 E2E 子阶段完成（2026-07-26）：

- MCP 页面从固定 `tabs/browser_state` 表单升级为“全局/单环境”分段作用域；全局读取可直接使用，单环境只列出 ready 环境。
- 工具发现展示 DLL advertised tools 与 Manager 白名单交集，mutation 明确标为策略保护；工具选择只生成健康、分页、环境、任务、页面、搜索和截图所需的最小参数，不提供任意 JSON 输入。
- 新增 5 个 MCP 组件测试，覆盖作用域切换、动态发现交集、全局 envId 参数、环境 grep 参数和非 ready 禁用；Dashboard 当前共 10 个组件测试。
- 真实环境 E2E 使用唯一账号环境完成 callback ready、CDP evaluate、单环境工具发现、`tabs(list)`、`read(page)` 和 SDK close；DLL 广告 18 个环境工具、Manager 放行 7 个，临时数据库与端口已清理。
- Browser 插件完成页面身份、DOM、作用域切换和 console 检查；其截图 API 不可用，截图与几何测量回退到本机 Chrome Playwright。1440x900 与 390x844 均无框架 overlay、应用 warning/error、页面级或 MCP 控件横向溢出，移动工具行稳定为 48px。

阶段 7 实现结果：

- Tauri Windows bundle 已启用 NSIS/MSI 配置，便携包使用相同的 Dashboard、`sdk-host.exe` 和 `brosdk.dll` 资源布局。
- 运行时支持便携目录、安装目录、Tauri `resources`/`resources/bin` 和目标三元组 sidecar 名称发现。
- 发布脚本兼容 Windows PowerShell，生成 `RELEASE-MANIFEST.json` 和 ZIP；阶段 18 在此基础上完成安装器流水线和真实安装验收。
- WebView2 使用 `embedBootstrapper`；正式签名证书仍由发布环境注入。

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
- `npm run e2e:environment` 在设置 `BROSDK_EMBEDDED_PORT` 时会实际调用全局 `/sdk/v1/mcp` 的 `env.tabs(list)`，不再只检查 TCP 监听。
- 2026-07-25 使用真实测试配置完成 DeepSeek smoke 和环境 E2E；报告确认 `readySource=sdk_callback`、`runtimeEvaluateVerified=true`、`embeddedMcpToolVerified=true`、`manualCloseVerified=true`。
- Dashboard AI/MCP 页面通过 1440x900 与 390x844 Playwright 检查：导航与 Chat/Agent 模式切换有效、预览态写按钮保持禁用、控制台无应用错误、页面无横向溢出。

## 18. 阶段 15：操作中心与故障恢复

目标：让多环境 operation 的展示、取消和重试与 Manager 实际执行语义一致，避免 UI 状态与 DLL 调用脱节。

任务：

- 操作中心按精确 `envId` 筛选，环境名称重复时仍显示“名称 · envId”。
- 增加当前结果、进行中和失败数量摘要，并在详情中保留 operation id、kind、envId、generation、reqId 和错误码。
- 取消只允许 queued operation；running operation 不提供取消入口，Manager 与 SQLite 状态机双重拒绝。
- 重试只开放 `environment.sync`、`runtime.reconcile`、`environment.start`、`environment.stop` 和 `kernel.install`，其它失败操作不显示不可执行的重试。
- 浏览器 E2E 覆盖同名环境筛选、动作保护和移动布局；真实 Tauri E2E 在环境启停后验证操作中心能看到同一 envId。

验收：

- running operation 不能被标记为 cancelled。
- 环境筛选不依赖名称，操作表、详情和自动化定位都使用 envId。
- 浏览器预览不执行 mutation，取消/重试按钮保持禁用。
- `npm run check`、`npm test`、`npm run build`、`npm run e2e:dashboard` 和 `npm run e2e:dashboard:desktop` 通过。

阶段 15 实现结果（2026-07-26）：

- `OperationsPage` 从单体 `App.tsx` 拆分，新增环境筛选、环境身份列、状态摘要和稳定 operation/envId 测试属性。
- Manager 新增 `OPERATION_NOT_CANCELLABLE`，只允许 queued operation 取消；SQLite 状态机移除 `running -> cancelled`。
- Dashboard 只对 Manager 已支持的失败/取消类型显示重试，指纹刷新、MCP、诊断等无重放契约的 operation 不再显示误导入口。
- Browser 插件验证页面身份、筛选、详情和 console；Playwright 在 1440x900 与 390x844 完成 10 项测试。真实 Tauri E2E 完成 start -> ready -> stop -> stopped，并报告 `operationIdentityObserved=true`。最终 Dashboard 36 项、Rust workspace 82 项测试，`npm run check`、Clippy 和 production build 全部通过。

## 19. 阶段 16：AI 配置与环境上下文

目标：让 AI Chat/Agent 可以在 Dashboard 内完成 Provider 配置，并明确查看当前可提供给 AI 的环境运行信息。

任务：

- AI API Key 使用平台安全存储，不写入 SQLite、operation、事件、日志或诊断包。
- OpenAI-compatible Base URL 和模型写入 Manager settings；`BROSDK_AI_*` 环境变量继续作为受管部署覆盖项。
- AI 页面提供环境上下文查看器，展示环境名称、envId、状态、CDP、最近事件、generation 和当前 operation。
- Manager AI context 增加 CDP 可用性与去除 userinfo/path/query/fragment 的 origin，不把完整 DevTools 控制路径发送给外部模型。
- AI 页面和设置页显示配置来源、密钥状态、Base URL 与模型，并提供保存、更换和移除安全存储密钥的入口。

验收：

- Dashboard 可以配置、重启恢复和移除 AI API Key，磁盘中不存在明文字节。
- Chat/Agent 使用当前有效 Provider 配置，环境变量覆盖时 UI 明确显示受管状态。
- 用户可在 AI 页面查看实际暴露的本地 CDP 地址；pipe-only 环境明确显示 DLL 内部 CDP/MCP 控制通道且不伪造 TCP 地址。模型上下文只包含安全 origin、`cdpAvailable` 和控制通道类型。
- 组件测试、Manager 测试、浏览器桌面/移动 E2E、真实 Tauri UI、`npm run check`、`npm test` 和 production build 通过。

阶段 16 实现结果（2026-07-26）：

- AI Chat/Agent 从环境变量或 Manager settings 解析 OpenAI-compatible Base URL 和模型；API Key 支持受管环境变量或平台安全存储，Dashboard 可保存、更换和清除安全存储密钥，密钥不会回显。
- schema 升级到 v6，`settings` 增加 `ai_base_url/ai_model`。AI API Key 使用独立 secret reference；测试确认 SQLite、事件和受保护文件均不含明文字节。
- AI 页面新增 Provider 状态和设置入口、精确 envId 环境选择器及运行上下文，显示状态、generation、reqId、operation、最近事件、CDP 地址和实际控制通道；设置页提供 Base URL、模型和 API Key 管理。
- Manager 只向模型发送选中环境。外部 CDP 只保留去除 userinfo/path/query/fragment 的 origin；pipe-only ready 环境标记为 `sdk-browser-command`，不把 `ready` 或 `-` 误判为地址。
- `sdk_browser_info` 与 callback 对账可为已 ready 环境补充后到的 `remoteDebuggingPort`；当前实测环境使用 DLL 内部 CDP pipe，因此 Dashboard 显示“未暴露 TCP 地址 / DLL 内部 CDP / MCP”。
- 真实 Tauri E2E 修正了“启动中按钮可停止”被误判为 ready 的测试缺陷，最终完成 start -> 明确运行中 -> AI 环境信息 -> Provider 设置 -> stop -> 操作中心。Dashboard 43 项、Rust workspace 86 项、Playwright 12 项测试，以及 `npm run check`、Clippy、production build 和真实桌面 E2E 全部通过。

## 20. 阶段 17：CDP 运行态多源回填

目标：把 DLL callback、`sdk_env_getinfo` 和 `sdk_browser_info` 中的 CDP 信息统一合并到精确 `envId` 的本机运行态，同时保持端口 0 的诚实 fallback。

任务：

- 建立统一 CDP endpoint 解析器，支持 DevTools URL、host:port、数值端口、数字字符串、字段命名变体和 JSON 编码子对象。
- 严格限制可识别字段，不读取普通 `port`，避免把代理端口、`fpBlockPort` 或端口扫描白名单当作浏览器调试端口。
- `browser-open-success` 直接写入非零 endpoint；callback 缺少地址时先查询一次 `sdk_env_getinfo`，再轮询 `sdk_browser_info`。
- `manager_refresh_environment_detail(s)` 在保存脱敏详情摘要的同时，允许为 ready 环境回填 CDP；回填不得改变 generation、reqId、operation 或最近生命周期事件。
- 桌面 E2E 单独报告 `cdpEndpointObserved`，区分真实 endpoint 与“DLL 内部 CDP / MCP”fallback。

验收：

- callback/getInfo/BrowserInfo 任一路返回非零端口时，Dashboard 显示 `127.0.0.1:<port>` 或原始 DevTools URL。
- 端口 0、缺失字段和非 CDP 端口配置不会生成伪地址。
- 回填事件不持久化完整 endpoint，只记录来源和可用性。
- workspace、Dashboard、Playwright 和真实 Tauri E2E 全部通过，测试结束后环境恢复 stopped。

阶段 17 实现结果（2026-07-26）：

- Manager 统一解析三路 CDP 数据，Store 新增 ready-only 事务回填并同步 runtime snapshot；sdk-host 测试确认回调脱敏不会移除 `remoteDebuggingPort`。
- `open-success` 后只有在 callback 未提供 endpoint 时才触发补查；`sdk_env_getinfo` 命中后立即结束，未命中再轮询 `BrowserInfo`。
- 直接 C API 实测仓库 DLL 2.0.0.8：success callback 与 BrowserInfo 返回 `remoteDebuggingPort=0`，运行中 getEnvInfo 没有 CDP/调试地址字段，因此桌面报告 `cdpEndpointObserved=false` 并保持内部控制通道显示。非零值、数字字符串、嵌套 JSON 和误判边界由单元测试覆盖。
- Browser 插件完成 AI 环境切换和无控制台错误验证；截图 API 在当前插件运行时不可用，视觉与响应式验证由 Playwright 接管。最终 Dashboard 43 项、Rust workspace 89 项、Playwright 12 项，`npm run check`、Clippy、production build 和真实桌面 E2E 全部通过。

## 21. 阶段 18：Windows 安装交付闭环

目标：让普通用户可以从一个可重复构建、可校验、经过真实安装测试的 Windows 安装包开始使用 Dashboard。

任务：

- 默认发布生成 NSIS 安装包和便携 ZIP；MSI 作为独立可选企业产物，不阻塞普通用户交付。
- 自动准备 Tauri 固定版本 NSIS/WiX 工具，下载必须可重试、可续传并通过固定哈希验证。
- 将安装器、便携 ZIP、版本、大小、SHA-256 和签名状态统一写入发布清单。
- 安装器 E2E 必须覆盖临时静默安装、打包资源、首次初始化、完整 Dashboard 生命周期和静默卸载。
- 静默卸载不得弹出数据删除确认；交互式卸载按系统语言询问，并允许保留用户数据。

验收：

- `npm run release:windows`、`npm run release:verify` 和 `npm run release:test:installer` 通过。
- 使用隐藏测试凭据运行 `npm run release:test:installer:full`，安装后的 release 完成 init -> start -> ready -> AI/operation -> stop -> stopped。
- 测试结束后无 BroSDK Dashboard 卸载注册、安装目录、临时数据目录或 `sdk-host` 残留。
- 内部未签名构建明确报告 `NotSigned`；正式发布可启用 `-RequireSignature` fail closed。

阶段 18 实现结果（2026-07-26）：

- 默认流水线真实生成约 13.1 MB NSIS 和 15.4 MB 便携 ZIP，可选流水线额外生成 `zh-CN/en-US` 双语 MSI，统一输出到 `dist/release`；产物清单、ZIP 内容、版本、大小和 SHA-256 校验通过。
- Tauri NSIS 3.11 与辅助插件使用官方地址和固定哈希准备到用户级缓存；不再依赖 `PATH` 或项目 `target` 中的临时工具。WiX 3.14.1 由可选 MSI 命令按同一策略准备。
- NSIS 首次启动烟雾测试通过；使用安全提示输入真实测试凭据后，已安装 release 完成初始化、环境 ready、AI 环境上下文、Provider 设置、停止和操作中心验收，随后静默卸载成功。
- 两个 MSI 均通过无产品注册的 administrative extraction，并包含 Dashboard、`sdk-host.exe` 和 `brosdk.dll`。
- 当前 DLL 仍返回 `remoteDebuggingPort=0`，安装版 E2E 正确报告 `cdpEndpointObserved=false` 并保留 DLL 内部 CDP/MCP 显示，没有伪造 TCP 地址。

## 22. 阶段 19：AI 会话与 Agent 执行可靠性

目标：把 AI 从一次性请求面板补齐为可追溯会话，并确保用户批准的环境动作使用明确 envId 和 Manager 最新状态，而不是模型猜测的前置条件。

任务：

- 会话与关联环境分离；支持本地历史、新建、切换、清空、删除和页面重载恢复。
- Chat/Agent 携带有界历史，Manager 拒绝非法角色、空消息和超限输入。
- 用户文本明确包含一个已同步 envId 时优先绑定该环境；多 envId 单计划 fail closed。
- Manager 在计划返回 UI 前写入真实 `expectedState` 和 UUID 幂等键，批准时再次校验状态。
- 显示 Tauri 返回的具体错误，不用“Agent 执行失败”覆盖真实原因。
- 对照服务端 browser API、DLL C API、全局/单环境 MCP 建立产品覆盖矩阵并纠正 capability 误报。

验收：

- 精确复现 `启动环境 2044366881367789568`：即使旧关联环境为另一个 ready 环境，计划仍绑定目标 envId 和其当前 stopped 状态。
- 批准后必须创建 `environment.start` operation；DLL accepted 后继续等待 callback/browser info 才能报告 ready。
- 第二轮请求带上第一轮 user/assistant 历史；重载后历史存在，清空和删除立即生效。
- 未绑定的 cookie/security callbacks 与 `sdk_token_update` 不再出现在 capability。
- 全量 Dashboard、Rust、Playwright、production build 和真实桌面 Agent 生命周期通过。

实现结果（2026-07-26）：

- AI 页面已将会话历史与关联环境拆开；会话在本机支持新建、切换、清空、删除和重载恢复，Chat/Agent 第二轮请求携带有界历史。
- Manager 已覆盖精确指令 `启动环境 2044366881367789568`：文本 envId 覆盖旧选择，计划状态从最新镜像写为 `stopped`，幂等键由 Manager 生成；多个已知 envId 的单动作请求 fail closed。
- Windows Tauri Agent E2E 真实点击生成计划和批准按钮，观察到 operation、目标 ready、`browser-open-success` 和最终 stopped；当前 DLL 继续报告端口 0，未伪造 CDP TCP 地址。
- Dashboard 46 项、Rust workspace 92 项、Playwright 12 项和 production build 通过；测试结束后无桌面测试进程或 `sdk-host` 残留。

## 23. 阶段 20：多环境 Agent 与完整单环境 MCP

目标：让 Agent 和 MCP 控制台按精确 envId 操作任意 ready 环境，单环境工具能力跟随 DLL 运行时目录，并提供用户显式选择的免逐次批准模式。

任务：

- MCP client 增加统一 `Option<envId>` 入口；该阶段先使用查询参数兼容路由，阶段 25 已升级为全局 endpoint 的 `env.* + arguments.envId`。
- 全局 mutation 继续走 Manager operation；单环境允许当次 DLL `tools/list` 广告的全部工具，参数实施总大小、深度和字符串长度门禁。
- MCP 页面常用读取工具继续使用结构化控件，其余工具使用高级 JSON 参数区；目录和数量不得写死。
- Agent 新增 `mcp.call`，Manager 继续重写 envId、expectedState 和幂等键，并在执行时验证环境 ready 与工具仍被广告。
- AI 会话新增“每次批准/自动执行”分段选择，默认每次批准；自动模式在计划返回后立即执行，但不跳过 Manager 状态和 reservation 校验。
- 扩展双环境真实 E2E，自动使用当前用户安全存储中的加密 SDK/AI 凭据，覆盖 Agent 生命周期、MCP 调用和补偿清理。

验收：

- `mcp-client` 单元测试确认无 envId 为全局 URL，有 envId 时查询参数正确编码，并完成完整 Streamable HTTP session。
- 真实 ready 环境的 `allowedTools` 与 `advertisedTools` 数量一致，当前 DLL 每环境实测至少 18 个；未广告工具仍由 client fail closed。
- Agent 在错误关联环境下仍根据文本精确 envId 启停目标；手动与自动模式分别覆盖，失败计划不可复用同一批准按钮。
- Agent 自己规划并执行 `mcp.call -> env.tabs(list)`，operation 绑定正确 envId。
- 两个临时环境最终 stopped、本地清理、服务端删除，测试前后账号环境总数一致。
- Dashboard 组件、桌面/移动 Playwright、Rust、production build 和敏感信息扫描通过。

实现结果（2026-07-26）：

- MCP transport 在该阶段通过查询别名完成 discovery 和 call；阶段 25 已改为只连接全局 endpoint。全局保持 9 个读取策略，ready 单环境目录完全跟随 `tools/list`，当前发现 18/18。
- MCP 控制台显示动态目录，已知读取工具用表单，其余工具用 64 KiB JSON 参数区；Agent 支持 `mcp.call`，旧 `mcp.read` 保留严格读取兼容。
- 每个 AI 会话独立持久化执行方式，自动模式直接执行新计划；首次尝试后计划按钮锁定，防止状态不确定时重放幂等键。
- 真实 E2E 创建两个临时环境，Agent 手动/自动启停、显式 envId 覆盖错误上下文、Agent MCP tabs 调用、指纹详情和 2/2 清理均通过，环境数 1 -> 1。
- Dashboard 49 项组件测试、Playwright 14 项、Rust workspace 93 项、check 与 production build 通过；桌面和 390x844 截图无重叠或横向溢出。

## 24. 阶段 21：Windows Runtime Host 后台化

目标：让 Dashboard 启动的 `sdk-host` 作为受监督后台子进程运行，不显示独立终端窗口，同时保留 IPC、诊断输出和退出监督能力。

实现结果（2026-07-27）：

- `sdk-client` 使用统一的后台进程工厂创建长期 `serve` 和一次性 `capabilities/smoke` 子进程，避免启动路径遗漏 Windows 隐藏标志。
- Windows 为所有子进程设置 `CREATE_NO_WINDOW`；不改变 `sdk-host` 的 CLI 输出协议，stdout/stderr 仍可由自动化和错误处理捕获。
- Windows 行为测试在按生产方式创建的子进程内调用 `GetConsoleWindow()`，结果为 0，并同时验证 stdout 管道可读。
- 真实 runtime-host smoke 覆盖启动、health、capabilities、优雅停止和 supervisor 强制退出；Rust workspace 94 项与 Clippy 全部通过，退出后无 `sdk-host` 残留。

## 25. 阶段 22：Windows 桌面生命周期

目标：让 Windows 桌面程序直接启动时不显示终端，关闭主窗口后驻留托盘，并提供可验证的恢复与退出路径。

实现结果（2026-07-27）：

- Tauri 主程序和 `sdk-host` 都声明 Windows GUI subsystem；受管 host 仍保留 `CREATE_NO_WINDOW` 作为父进程侧防护。
- 主窗口关闭请求改为隐藏；托盘单击/双击恢复窗口，右键菜单提供“打开主界面”和“退出”。退出最多等待 5 秒完成 runtime 优雅停止，避免异常 host 永久卡住菜单动作。
- 新增 `npm run e2e:tray`，通过 Windows UI Automation 和真实托盘交互验证关闭隐藏、进程保留、托盘恢复与菜单退出，测试使用唯一临时数据目录并清理残留。
- 便携与安装验证读取 PE header，Dashboard 和 host 必须为 `Subsystem=2`；旧控制台子系统产物会直接拒绝。
- debug 与便携 release 托盘 E2E、NSIS 首次启动/静默卸载、release 清单验证通过；阶段增量后 Rust workspace 为 96 项，Clippy 通过，无 Dashboard、host 或临时目录残留。

## 26. 阶段 23：环境启动回调进度

目标：在环境启动期间显示 DLL callback 返回的真实进度，同时继续隐藏内部 payload 和敏感字段。

实现结果（2026-07-27）：

- Store 只处理与 operation 方向一致的 `browser-open`/`browser-close` 事件；非终态回调更新 operation message、环境 last event、reqId 和时间，不提前完成 operation 或改变 generation。
- 进度兼容 callback 嵌套的 `percent`/`progress` 数值与数字字符串，只接受 0-100；状态只读取 `statusName/stateName/statusText`，拒绝控制字符并限制为 64 字符。
- 环境表在 `starting` 状态从精简事件文本读取百分比，显示固定宽度进度条和数值；完整 callback payload 不进入可见 UI。桌面和 390x844 视口没有重叠或横向溢出。
- 真实生命周期 runner 在 ready 后回查 Manager 事件流，必须发现目标 envId 的 `browser-open` 中间回调和合法进度；报告只输出 `startProgressCallbackObserved` 布尔值。
- 真实 E2E 得到 `startProgressCallbackObserved=true`，并完成 callback ready、CDP evaluate、18 个 MCP 工具、指纹检查和停止；Dashboard 51 项、Playwright 16 项、Rust workspace 99 项、check、Clippy、runtime smoke 和 release 构建通过。

## 27. 阶段 24：客户端重启状态恢复与单实例

目标：客户端异常退出后不沿用过期的 starting/ready 状态，并确保同一个 Dashboard 只初始化一份 SDK runtime。

实现结果（2026-07-27）：

- Manager 新会话会原子终止上次遗留的 queued/running operation，错误码为 `CLIENT_RESTARTED`；可能仍运行的环境先变为 unknown，再以新 Runtime Host 的 `sdk_browser_info` 为事实恢复 ready 或 stopped。
- BrowserInfo 返回的是当前 DLL 跟踪的运行列表，因此 envId 存在且调试端口为 0 时也恢复 ready；CDP 继续显示为内部通道，不伪造 TCP 地址。每次客户端启动都执行状态对账，`startupPolicy` 只决定是否恢复动作，不再阻止只读对账。
- Tauri single-instance 插件作为首个插件注册，锁定应用标识 `com.brosdk.dashboard`。第二次启动不会创建第二个 Manager/sdk-host，而是显示、取消最小化并聚焦已有窗口。
- DLL 的后端 appId 锁仍是跨客户端的第二层保护：同一 Dashboard 的重复启动由桌面单实例提前拦截，其它产品若占用同 appId，则由隔离 sdk-host 承担初始化冲突，桌面 UI 不随 DLL 退出。
- Store 重启场景测试、Rust workspace 101 项测试和目标 Clippy 通过；真实 Windows 托盘 E2E 报告 `secondInstanceRedirected=true`，并继续完成隐藏、托盘恢复和菜单退出。

## 28. 阶段 25：新版全局多环境 MCP

目标：跟随新版 DLL 的推荐接入方式，让 Dashboard 和 Agent 只建立全局 MCP 连接，并按每次调用的 envId 控制不同环境。

实现结果（2026-07-27）：

- `mcp-client` 不再为环境作用域构造 `?envId=` URL；发现和调用都连接 `/sdk/v1/mcp`，旧 `tabs` 等输入规范为 `env.tabs`，Manager 选择的 envId 覆盖写入 arguments，阻止跨环境参数伪造。
- 环境目录从全局 `tools/list` 动态过滤。由于环境管理工具也使用 `env.` 前缀，策略显式排除 `env.list/resolve/get/create/update/destroy`，避免把创建、更新、删除误归入 ready 环境的页面工具白名单。
- Dashboard 显示真实 `env.*` 工具名，结构化表单继续按基础工具语义工作；AI 提示要求使用 `env.tabs/env.snapshot/env.act`，旧计划里的无前缀名称仍可兼容规范化。
- transport、Manager 和 MCP 页面测试通过；Dashboard 51 项、Rust workspace 101 项、Playwright 桌面/移动 16 项、check 与 Clippy 全部通过。真实 DLL 生命周期 E2E 发现并放行 18/18 个浏览器工具，经全局 endpoint 完成 `env.tabs(list)`、`env.read(page)`、指纹检查和停止，环境最终为 stopped。

## 29. 阶段 26：GitHub 首页与发布产物收口

目标：让首次访问仓库的开发者能从根 README 完成理解、开发、测试和 Windows 交付，并保证发布包包含当前受版本管理的 SDK 运行时。

实现结果（2026-07-27）：

- 根 README 覆盖产品定位、首次 API Key 初始化、envId 数据模型、隔离 Host、桌面单实例/托盘、全局 MCP、AI Agent、安全边界和文档索引。
- 开发与发布命令已集中说明；`target/` 定义为 Cargo/Tauri 原始构建区，`apps/dashboard/dist/` 定义为 Vite 中间产物，`dist/release/` 定义为经过发布脚本整理和验证的最终交付区。
- 更新后的 `libs/windows_x64/brosdk.dll` 和 `brosdk.h` 纳入版本库；本地服务端接口快照 `doc.json`/`docs.json` 显式忽略，不会随 GitHub 上传。
- 默认 Windows x64 NSIS 与便携 ZIP 使用当前代码和 DLL 重建，并通过发布清单、SHA-256、ZIP 布局、PE GUI subsystem 和签名状态校验；静默安装/首次初始化/卸载及便携版单实例/托盘 E2E 通过。
