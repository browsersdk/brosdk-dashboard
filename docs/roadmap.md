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
| 10. 环境创建交互收敛 | P0 | 已完成 | 创建环境只要求选择代理和内核版本，真实 DLL 创建/删除及镜像对账通过 |
| 11. 远端事实源与 MCP 双层路由 | P0 | 已完成 | 环境配置以 SDK 服务端为准，本地仅保留可丢弃缓存；DLL 全局与单环境 MCP 已接通并通过真实验收 |

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
- DLL 全局 MCP `/sdk/v1/mcp` 用于环境发现、详情和运行状态诊断；单环境 MCP `/sdk/v1/mcp/env/{envId}` 用于具体浏览器页面操作。
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
- 因此缺口不在 DLL，而在本项目原先只实现 `/sdk/v1/mcp/env/{envId}` 且只允许 `browser_state(get)`、`tabs(list/current)`。
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

## 15. 当前状态

阶段 0-11 的仓库内规划已完成。阶段 12 已完成契约审计并进入实施；按“安全初始化 -> 环境/远端指纹工作台 -> 运维动作 -> 真实 E2E”拆分，每一部分独立执行自动测试、更新文档并提交。

阶段 12 安全初始化子阶段完成（2026-07-26）：

- 无凭据时 Manager 不再启动 Host，Dashboard 首屏只显示 API Key 初始化；成功配置后直接进入环境工作台。
- API Key 候选通过子进程环境注入隔离 Host，完成 `getUserSig(role=user) -> init -> 完整 env_page` 后才使用平台安全存储持久化；Manager IPC、SQLite 和 snapshot 不含密钥。
- 环境变量仍作为测试/受管部署覆盖项；设置页只读显示该来源。安全存储来源支持更换和移除，账号边界变化会清除环境、详情、运行态、operation、event、Agent 记录和旧环境绑定。
- 新增 `npm run e2e:credential`，使用唯一临时数据目录验证 DPAPI 密文、Manager 重建恢复和移除隔离；真实结果为 1 个环境、`encryptedAtRest/restartLoaded/accountStateCleared=true`。
- `npm run check`、workspace Rust 测试、Dashboard 13 个组件测试和 production build 通过；1440x900 与 CDP 390x844 视觉/几何检查无溢出，控制台无 warning/error。

阶段 11 远端缓存子阶段完成（2026-07-26）：

- `sdk_env_page` 默认请求 200 条并按 `data.total` 拉取完整分页，按 envId 去重，限制最多 500 页/100000 条；异常总数、重复页和提前空页均 fail closed。
- schema 升级到 v5，新增 `environment_cache_status`；全页成功后单事务替换缓存并删除远端缺失项，失败保留旧缓存并记录脱敏 stale 原因。
- `EnvironmentRecord` 和 Dashboard 删除本地名称/标签覆盖；迁移保留旧列以兼容数据库，但启动即清空且后续不再读取。
- API Key 可用时首次 snapshot 自动刷新；Dashboard 环境工具栏显示服务端、缓存或待同步状态，诊断包与 Manager smoke 同步输出缓存元数据。
- `npm run check`、`npm test`、`npm run build` 通过；真实 `getUserSig(role=user) -> init -> env_page` Manager smoke 使用独立临时数据库通过，返回 fresh/1 个服务端环境且测试目录已清理；1440x900 与 390x844 环境页无重叠、无页面级横向溢出且控制台无应用错误。

阶段 11 Manager MCP 子阶段完成（2026-07-26）：

- MCP client 复用全局 `/sdk/v1/mcp` 与单环境 `/sdk/v1/mcp/env/{envId}` 的严格 Streamable HTTP session 生命周期，`tools/list` 同时解析说明和 read-only/destructive annotations。
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
