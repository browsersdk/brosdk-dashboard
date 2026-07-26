# BroSDK Dashboard 新客户端规划入口

本文档目录用于在新会话中接力实施 `D:\go\src\browsersdk\brosdk-dashboard`。

当前目标是基于 `brosdk-v3` 已完成的 Windows WebView 客户端经验，规划并实现一个新的跨平台本地客户端。首个可执行版本优先 Windows x64，使用本项目已有的动态库：

- `libs/windows_x64/brosdk.dll`
- `libs/windows_x64/brosdk.h`

动态库源码参考：

- `D:\go\src\browsersdk\orbitbridge\projects\brosdk`

动态库使用文档参考：

- `D:\go\src\browsersdk\orbitbridge-docs\brosdk`

## 新会话读取顺序

1. 先读本文件，确认项目目标和当前边界。
2. 再读 [architecture.md](architecture.md)，确认客户端架构、进程边界和技术栈。
3. 再读 [dll-integration.md](dll-integration.md)，确认 `brosdk.dll` 的调用方式、回调、状态和风险。
4. 再读 [roadmap.md](roadmap.md)，按阶段实施。
5. 读 [manager-domain.md](manager-domain.md)，确认 SQLite、operation、generation 和事件规则。
6. 最后读 [testing-handoff.md](testing-handoff.md)，按约定设置测试密钥并跑 E2E。

## 当前实现状态

截至 2026-07-26，阶段 0-17 已完成：

```text
brosdk-dashboard/
  apps/dashboard/            React + TypeScript + Vite Dashboard
  apps/desktop/src-tauri/    Tauri 2 桌面外壳
  crates/domain/             领域与传输模型
  crates/manager/            SQLite、operation、镜像、事件与 Manager API
  crates/runtime-ipc/        named pipe/UDS 与长度前缀 JSON 帧
  crates/sdk-ffi/            brosdk.dll C ABI 绑定
  crates/sdk-host/           DLL 隔离进程与 smoke CLI
  crates/sdk-client/         Manager 侧 host client
  crates/platform/           平台路径与可执行文件适配
  crates/local-api/          loopback API 预留
  libs/windows_x64/          Windows x64 DLL 与头文件
```

当前自动验证覆盖 workspace 编译、Rust 单元测试、Dashboard 构建、DLL 符号加载、capability 报告、runtime host 优雅停止和强制退出降级。联网的 `getUserSig -> init -> info -> env_page` 只在调用进程已设置 `BROSDK_API_KEY` 时运行；未设置时 smoke 会安全跳过联网阶段并执行 `sdk_shutdown`。

## 关键产品定位

新客户端是一个本机单用户、多指纹、多环境管理工具：

- 桌面应用直接打开 Dashboard，不要求用户手动打开浏览器网页。
- Dashboard 继续采用白色、简洁、扁平、高信息密度的管理界面。
- Windows 首版基于 `libs/windows_x64/brosdk.dll` 管理环境、内核、代理、Cookie/Storage 和运行状态。
- 跨平台能力通过平台 adapter 演进：Windows 先用 DLL，macOS/Linux 等对应动态库和进程/密钥库/IPC adapter 准备好后再接。
- AI Agent 已接入受控规划/审批链路，Provider 配置与环境上下文由 Manager 统一管理。

## 重要约束

- 不把 API Key 写入仓库、文档、日志或截图。测试时使用 `BROSDK_API_KEY` 环境变量。
- `sdk_browser_open` 返回受理不等于 ready。真实可用状态以异步回调中的 `browser-open-success` 和 `sdk_browser_info` 对账为准；CDP 地址由 callback、`sdk_env_getinfo` 和 `sdk_browser_info` 三路补充，端口 0 不能伪造成 TCP endpoint。
- `brosdk.dll` 在 `sdk_init` 后会根据后端返回的 `appId` 做进程锁检查；冲突场景可能触发退出。新客户端应让隔离的 runtime host 加载 DLL，避免 DLL 直接结束桌面 UI。
- Dashboard 不直接加载 DLL，不直接读写本地数据库，不直接访问 CDP。所有操作进入本地 Manager/operation 队列。
- 动态端口只用于必要的本地 HTTP/MCP 入口。客户端内部优先用 named pipe/UDS 或 Tauri command/event，减少端口占用和启动失败点。

阶段 6 已补齐指纹、代理、内核、操作和设置菜单。代理密码在 Windows 使用 DPAPI 保护；目录、导入/导出和诊断包使用系统文件选择器；内核没有可靠 catalog URL 时显示“未知”，不会误报可更新。

## 当前实施状态

[roadmap.md](roadmap.md) 中阶段 0-17 的仓库内规划已全部实施并通过当前平台验收。首次 API Key 激活、安全凭据持久化、环境工作台、多环境生命周期、远端指纹对比、Dashboard envId 身份、操作中心、AI Provider/环境上下文和 CDP 运行态回填已经形成完整桌面流程。环境配置继续以 SDK 服务端为唯一事实来源，SQLite 只保留可删除、带新鲜度状态的脱敏缓存；API Key 使用平台安全存储，userSig 只进入隔离 Host/DLL 生命周期。

`doc.json` 与服务端源码确认：`/api/v2/browser/*` 是 API Key 认证的环境管理契约，`/api/v2/sdk/*` 是 DLL 使用 userSig 的内部契约。Dashboard 不让用户配置 userSig，也不直接调用内部 SDK HTTP 接口。普通环境创建仍只有代理和内核版本；环境详情、指纹、代理和内核实际值从 `sdk_env_getinfo` 获取并以脱敏缓存支持离线只读。

阶段 12 首次初始化已完成：无凭据时不启动 Host，桌面首屏输入 API Key 后才执行 `getUserSig(role=user) -> init -> env_page`；成功后使用平台安全存储持久化。设置页可更换或移除安全存储凭据，换号会清空上一账号的环境、详情和运行态缓存。真实 E2E 已验证 DPAPI 文件不含明文、Manager 重建可恢复、移除后账号状态清空。

阶段 12 环境/远端指纹工作台已完成：环境详情通过 `manager_refresh_environment_detail(envId)` 单独调用 `sdk_env_getinfo`，校验业务 `code=200` 后只缓存指纹、掩码代理、内核与少量环境元数据；`cookie`、`storage`、上传路径、DEK、token 和凭据不会进入 SQLite/snapshot。环境详情侧栏直接展示服务端内核、代理、语言、时区和屏幕；“指纹”页改为跨环境结构化查看器，不再以本地 Profile JSON 编辑器作为主流程。浏览器 UI 验收可使用 `?preview=workspace&page=environments|fingerprints`，该模式不启用本机 mutation。

阶段 12 运维动作已接通：环境详情区统一提供启停、详情刷新、指纹检查、页面诊断、本地浏览数据清理和服务端删除。清理与删除只允许 stopped 环境并分别二次确认；页面诊断只允许 ready 环境，并强制关闭 HTML、截图和事件 chunk，只向 Dashboard 返回页数、失败数与去除 path/query/userinfo 的 origin。真实临时环境 E2E 已验证本地清理与服务端删除是两个独立动作，测试环境完成最终对账和清理。

阶段 12 最终生命周期验收已完成：`npm run e2e:environment` 使用隐藏 API Key、临时 Manager 数据目录和自动分配的 DLL MCP 端口，账号只有一个环境时自动选择。真实链路完成启动 callback ready、CDP evaluate、内置指纹检查页新标签、脱敏页面诊断、单环境 MCP `tabs/read` 和 SDK 停止；`sdk_browser_env_check` 的 target/session/CDP 原始结果在 Manager 内压缩为布尔摘要，不进入 Dashboard。Dashboard 19 项、Rust workspace 69 项测试及 Clippy、production build 全部通过。

阶段 13 多环境工作流已完成。环境表支持最多 20 个多选、全选当前结果和按状态拆分的启动/停止；Manager 为每个环境编排独立 operation/generation，不复用一个 batch callback。stopped 环境只能修改名称和序号，Manager 校验 Unicode/UTF-8 长度、`code=200` 和服务端回显后刷新列表与详情；旧版 `getEnvInfo` 返回空序号时，只能用 SDK 服务端分页或已确认更新响应补全缓存，不能从表单乐观覆盖。指纹页提供详情/对比模式，最多选择 4 个服务端环境，按固定脱敏字段显示相同、不同或未知。服务端没有环境分组/标签契约，因此不新增本地覆盖，也不把 customerId 误用为分组。

阶段 13 真实双环境验收也已完成：`npm run e2e:multi-environment` 使用隐藏 API Key 和唯一临时 Manager 数据目录，创建两个临时环境，分别更新元数据，再通过批量入口生成两组独立启停 operation；两个环境均到达 callback ready、刷新各自远端指纹详情、停止、清理本地数据并删除服务端记录。测试前后账号环境数均为 1，成功报告只包含数量和布尔值；异常路径会先对账运行态，再补偿停止、清理和删除。Dashboard 28 项、Rust workspace 80 项测试及 Clippy、production build 全部通过。

阶段 14 Dashboard 身份验收已完成：`envId` 是环境、详情、指纹列、批量选择、代理绑定和 MCP 单环境选择的唯一关联键，环境名称允许重复；snapshot 对空/重复 envId、重复详情绑定和悬空详情绑定 fail closed。`npm run e2e:dashboard` 在 1440x900 与 390x844 下执行 8 项同名环境和只读预览 Playwright 流程；`npm run e2e:dashboard:desktop` 另外通过真实 Tauri 环境表按钮完成启动/ready/停止/stopped。真实双环境 runner 也显式断言两个临时 envId 非空且唯一。最终 Dashboard 33 项、Playwright 8 项、Rust workspace 81 项测试及 Clippy、production build 全部通过；真实 E2E 前后环境数均为 1，清理 2/2，退出后无临时目录或 `sdk-host` 残留。

阶段 15 操作中心与故障恢复已完成：operation 列表按精确 `envId` 过滤并同时显示服务端名称和标识，摘要区显示当前结果、进行中与失败数量；失败/取消操作只有在 Manager 已实现对应重试路径时才显示重试。用户取消只允许 queued operation，Manager 和 SQLite 状态机都拒绝把 running operation 标为 cancelled，避免 SDK/DLL 调用继续执行而界面误报已取消。浏览器预览与真实 Tauri E2E 已验证操作中心能追踪环境启停记录；Dashboard 36 项、Rust 82 项、Playwright 10 项测试，以及 check、Clippy 和 production build 全部通过。

阶段 16 AI 配置与环境上下文已完成：AI 页面和设置页均可进入 Provider 配置，Base URL/模型保存在 Manager settings，AI API Key 使用平台安全存储且不回显；环境变量仍可覆盖受管配置。Chat/Agent 选择精确 `envId` 作为上下文，Dashboard 本地显示运行状态、generation、reqId、operation、最近事件及 CDP 控制信息。若 DLL 暴露 `remoteDebuggingPort`，界面显示并允许复制实际地址；当前 pipe-only 环境明确显示“未暴露 TCP 地址 / DLL 内部 CDP / MCP”。模型只收到脱敏 origin 或 `sdk-browser-command` 控制通道，不接收完整 DevTools URL。最终 Dashboard 43 项、Rust workspace 86 项、Playwright 12 项、真实 Tauri 启停/AI/设置 E2E、check、Clippy 和 production build 全部通过。

阶段 17 CDP 运行态回填已完成：Manager 使用统一解析器读取 `browser-open-success`、`sdk_env_getinfo` 和 `sdk_browser_info` 中的 DevTools URL 或调试端口，兼容数值、数字字符串、下划线字段和 JSON 编码子对象；只识别 CDP 专用键，不会把代理端口或指纹端口扫描配置误判为 CDP。callback 已提供地址时直接落库；否则 success 后先查询一次 `sdk_env_getinfo`，再轮询本地 `sdk_browser_info`。详情刷新同样可为 ready 环境补充地址，事务只更新 CDP/runtime snapshot，不改变 generation、reqId、operation 或最近生命周期事件。当前仓库携带的 DLL 2.0.0.8 实测 success callback 与 BrowserInfo 的 `remoteDebuggingPort` 为 0，运行中 getEnvInfo 未返回 CDP 字段，因此真实桌面仍显示内部控制通道；非零路径由单元测试覆盖。最终 Dashboard 43 项、Rust workspace 89 项、Playwright 12 项、真实 Tauri E2E、check、Clippy 和 production build 全部通过。

阶段 10 的默认值边界：代理可不选；内核版本必须来自 Manager 本地已安装的当前平台 core；Manager 只向 `sdk_env_create` 发送服务端 `dto.FingerReqDto` 支持的顶层 `kernel`、`kernelVersion` 和可选 `proxy`。`customerId`、`envName` 以及语言、时区、UA、Canvas、WebGL 等字段均省略，由 userSig 上下文和服务端默认策略处理。代理密码只在 Manager 调用 DLL 前从系统密钥库恢复，不进入 operation、事件、snapshot、文档或日志。

Manager/Runtime Host 创建链路、Dashboard 双字段交互和真实创建/删除验收均已完成：FFI 已加载 `sdk_env_create`/`sdk_env_destroy`，Runtime Host 继续统一脱敏，Manager 校验本地内核、后端业务 `code=200`、创建结果 `data.envId`，并把结果写入本地镜像和 operation。环境页的创建带只显示代理与内核版本，默认本机网络并预选最新本地内核；无可用内核时跳转内核页。`getUserSig` 请求固定使用 `role=user`。真实 DLL 验收使用 Chrome 134 和本机网络完成创建、镜像确认、删除及 `env_page` 对账，测试环境已清理。

阶段 11 的缓存边界：API Key 只用于换取 userSig，环境列表和详情仍由 DLL 向 SDK 服务端读取；SQLite 环境数据不是可独立编辑的本地事实，只是完整分页成功后的可丢弃缓存。Manager 首次 snapshot 在 API Key 可用时自动刷新，按 `page/pageSize` 拉取完整集合，全部成功才原子替换；失败保留旧值并标记 stale。旧 `local_label/tags_json` 会在迁移时清空，Dashboard 只显示服务端名称、envId 和缓存状态；本机 generation、reqId、CDP 和 ready 状态继续作为运行态叠加。DLL 源码审计确认全局 `/sdk/v1/mcp` 已能管理和定位环境，单环境 `/sdk/v1/mcp/env/{envId}` 已包含完整页面工具。

阶段 11 的 MCP 边界：Manager 现在按 `global`/`environment` 显式路由，复用严格的 `initialize -> initialized -> tools/list -> tools/call -> DELETE` 生命周期，并把工具发现写入 operation。全局仅放行 9 个健康、环境查询、浏览器状态和任务查询工具；单环境仅放行 `browser_state`、`tabs`、`snapshot`、`diff`、`read`、`grep`、`screenshot` 的受限只读参数。环境创建/更新/删除、浏览器启停、导航、脚本执行、上传下载等 mutation 不能从该通道直通。真实 DLL smoke 已发现 16 个全局工具并验证 `sdk.health`、`env.list`、`mcp.endpoint`，协议为 `2025-11-25`。

Dashboard MCP 页面使用“全局/单环境”作用域、ready 环境选择、动态工具状态和按工具生成的最小表单，不提供任意 JSON 透传。真实单环境 E2E 发现 DLL 广告 18 个工具、Manager 放行 7 个，并完成 `tabs(list)` 与 `read(page)`；1440x900 和 390x844 交互/视觉验收无应用控制台错误或 MCP 控件横向溢出。

阶段 5 已用真实账号完成 `getUserSig(role=user) -> init -> env_page -> browser_open -> browser-open-success -> Runtime.evaluate -> browser_close -> browser-close-success`。DLL 自带 MCP capability 已验证可用；只有设置 `BROSDK_EMBEDDED_PORT` 时才在 runtime host 初始化中启用端口。

阶段 7 已完成 Windows 便携发布验证：`npm run release:portable` 生成 ZIP，`npm run release:verify` 校验 `BroSDK Dashboard.exe`、`sdk-host.exe`、`brosdk/brosdk.dll` 和 `RELEASE-MANIFEST.json`。NSIS/MSI 的最终构建需要 Windows 构建机安装 NSIS/WiX 工具；正式签名不在仓库中保存证书。

阶段 8 已完成平台路径、UDS、系统 keyring 和 capability 边界；Windows、Linux x64 和 macOS x64 的核心平台 crates 均通过编译检查。仓库当前仍只携带 Windows x64 SDK 动态库，因此其他平台会明确显示 unavailable，直到对应库加入 `libs/<platform>_<arch>`。

阶段 9 已完成 DeepSeek/OpenAI 兼容 AI 与 DLL MCP 只读 adapter：使用 `BROSDK_AI_API_KEY`、`BROSDK_AI_BASE_URL`、`BROSDK_AI_MODEL`，默认模型为 `deepseek-v4-flash`。Chat 为只读，Agent 需要显式批准并通过 Manager 的 action、状态和持久化幂等校验；MCP 只允许 ready 环境执行 `browser_state(get)` 和 `tabs(list/current)`，所有调用都有 operation。

2026-07-25 最终验收已完成：DeepSeek smoke、`getUserSig(role=user) -> init -> env_page`、runtime/Manager smoke、环境 ready、页面级 CDP、DLL MCP `tabs(list)`、手动关闭对账、workspace 测试/Clippy、Dashboard production build，以及 1440x900 与 390x844 的 AI/MCP 页面交互和无横向溢出检查均通过。测试凭据未写入仓库。
