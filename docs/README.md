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
5. 最后读 [testing-handoff.md](testing-handoff.md)，按约定设置测试密钥并跑 E2E。

## 当前实现状态

截至 2026-07-25，阶段 0 项目骨架、阶段 1 DLL smoke 和阶段 2 Runtime Host 已实现：

```text
brosdk-dashboard/
  apps/dashboard/            React + TypeScript + Vite Dashboard
  apps/desktop/src-tauri/    Tauri 2 桌面外壳
  crates/domain/             领域与传输模型
  crates/manager/            Manager 入口
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

## 当前实施目标

下一步按 [roadmap.md](roadmap.md) 推进阶段 3 Manager Domain，再逐步完成最小环境生命周期闭环：

1. 启动桌面窗口。
2. 从环境变量读取 API Key，换取 userSig。
3. 初始化 `brosdk.dll`。
4. 拉取环境列表。
5. 启动一个指定环境并等待真实 ready。
6. 展示 CDP/运行状态。
7. 停止环境。
8. 处理手动关闭浏览器后的状态回写。

完成这个闭环后，再移植 `brosdk-v3/apps/dashboard` 的完整菜单和 AI Agent 侧边栏。
