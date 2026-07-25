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

截至 2026-07-25，阶段 0 项目骨架、阶段 1 DLL smoke、阶段 2 Runtime Host、阶段 3 Manager Domain、阶段 4 Dashboard MVP、阶段 5 环境 E2E、阶段 6 完整菜单、阶段 7 Windows 便携发布、阶段 8 跨平台 adapter 基础和阶段 9 AI/MCP 已实现。阶段 10 环境创建交互收敛正在实施：

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
- AI Agent 自动化作为后续阶段接入，先保证基础环境管理链路稳定。

## 重要约束

- 不把 API Key 写入仓库、文档、日志或截图。测试时使用 `BROSDK_API_KEY` 环境变量。
- `sdk_browser_open` 返回受理不等于 ready。真实可用状态以异步回调中的 `browser-open-success`、CDP ready 和 `sdk_browser_info` 对账为准。
- `brosdk.dll` 在 `sdk_init` 后会根据后端返回的 `appId` 做进程锁检查；冲突场景可能触发退出。新客户端应让隔离的 runtime host 加载 DLL，避免 DLL 直接结束桌面 UI。
- Dashboard 不直接加载 DLL，不直接读写本地数据库，不直接访问 CDP。所有操作进入本地 Manager/operation 队列。
- 动态端口只用于必要的本地 HTTP/MCP 入口。客户端内部优先用 named pipe/UDS 或 Tauri command/event，减少端口占用和启动失败点。

阶段 6 已补齐指纹、代理、内核、操作和设置菜单。代理密码在 Windows 使用 DPAPI 保护；目录、导入/导出和诊断包使用系统文件选择器；内核没有可靠 catalog URL 时显示“未知”，不会误报可更新。

## 当前实施状态

[roadmap.md](roadmap.md) 中阶段 0-9 的仓库内规划已全部实施并通过当前平台验收。阶段 10 已新增并进入实施，目标是补齐 `sdk_env_create` 完整链路，同时把创建环境的普通用户界面收敛为只选择代理和内核版本。

阶段 10 的默认值边界：代理可不选；内核版本必须来自 Manager 已知 catalog/本地扫描；`customerId` 与环境名称由 Manager 自动生成；语言、时区、UA、Canvas、WebGL 等指纹细项交给后端按代理 IP 和 SDK 默认策略生成。代理密码只在 Manager 调用 DLL 前从系统密钥库恢复，不进入 operation、事件、snapshot、文档或日志。

阶段 5 已用真实账号完成 `getUserSig(role=user) -> init -> env_page -> browser_open -> browser-open-success -> Runtime.evaluate -> browser_close -> browser-close-success`。DLL 自带 MCP capability 已验证可用；只有设置 `BROSDK_EMBEDDED_PORT` 时才在 runtime host 初始化中启用端口。

阶段 7 已完成 Windows 便携发布验证：`npm run release:portable` 生成 ZIP，`npm run release:verify` 校验 `BroSDK Dashboard.exe`、`sdk-host.exe`、`brosdk/brosdk.dll` 和 `RELEASE-MANIFEST.json`。NSIS/MSI 的最终构建需要 Windows 构建机安装 NSIS/WiX 工具；正式签名不在仓库中保存证书。

阶段 8 已完成平台路径、UDS、系统 keyring 和 capability 边界；Windows、Linux x64 和 macOS x64 的核心平台 crates 均通过编译检查。仓库当前仍只携带 Windows x64 SDK 动态库，因此其他平台会明确显示 unavailable，直到对应库加入 `libs/<platform>_<arch>`。

阶段 9 已完成 DeepSeek/OpenAI 兼容 AI 与 DLL MCP 只读 adapter：使用 `BROSDK_AI_API_KEY`、`BROSDK_AI_BASE_URL`、`BROSDK_AI_MODEL`，默认模型为 `deepseek-v4-flash`。Chat 为只读，Agent 需要显式批准并通过 Manager 的 action、状态和持久化幂等校验；MCP 只允许 ready 环境执行 `browser_state(get)` 和 `tabs(list/current)`，所有调用都有 operation。

2026-07-25 最终验收已完成：DeepSeek smoke、`getUserSig(role=user) -> init -> env_page`、runtime/Manager smoke、环境 ready、页面级 CDP、DLL MCP `tabs(list)`、手动关闭对账、workspace 测试/Clippy、Dashboard production build，以及 1440x900 与 390x844 的 AI/MCP 页面交互和无横向溢出检查均通过。测试凭据未写入仓库。
