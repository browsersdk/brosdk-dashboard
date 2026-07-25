# BroSDK Dashboard 跨平台客户端架构

## 1. 目标

`brosdk-dashboard` 是新的本地桌面客户端项目。它吸收当前 `brosdk-v3` Windows WebView 客户端的产品形态和 Dashboard 功能，但把实现边界重新整理为更容易跨平台、测试和发布的结构。

首发平台是 Windows x64。运行时能力来自：

```text
libs/windows_x64/brosdk.dll
libs/windows_x64/brosdk.h
```

后续平台不改变 Dashboard、领域模型和管理 API，只补平台动态库、IPC、进程、密钥库和打包 adapter。

## 2. 技术选型

建议采用：

- 桌面外壳：Tauri 2 / Rust。
- Dashboard：React + TypeScript + Vite，移植 `brosdk-v3/apps/dashboard` 的信息架构和视觉基线。
- 本地 Manager：Rust async service，负责状态机、SQLite、operation 队列、设置、日志和事件。
- SDK runtime host：独立 Rust 子进程，加载 `brosdk.dll`，通过 named pipe/UDS 和 Manager 通讯。
- 本地持久化：SQLite WAL。
- 密钥保护：Windows DPAPI/Credential Manager；macOS Keychain；Linux Secret Service。

选择独立 runtime host 的原因是 `brosdk.dll` 的 v2 代码在某些路径上可能调用 `std::exit`，例如同 `appId` 实例锁冲突。让 DLL 运行在隔离进程中，可以避免 SDK 直接结束桌面窗口，并把崩溃、退出码、日志和恢复策略收敛到 Manager。

## 3. 进程拓扑

```mermaid
flowchart LR
    SHELL["Desktop Shell\nTauri / WebView2 / WKWebView"]
    UI["Dashboard\nReact + TypeScript"]
    MANAGER["Local Manager\nRust service"]
    DB[("SQLite\nlocal state")]
    HOST["SDK Runtime Host\nloads brosdk.dll"]
    DLL["brosdk.dll"]
    BROWSER["YunBrowser / Chromium envs"]
    PIPE["named pipe / UDS"]
    API["Optional loopback API\nfor dev/tests/MCP"]
    DLLMCP["DLL embedded MCP\nvia sdk_init port"]

    SHELL --> UI
    UI -->|"Tauri invoke/events"| MANAGER
    UI -.->|"optional same-origin HTTP"| API
    MANAGER --> DB
    MANAGER --> PIPE --> HOST --> DLL --> BROWSER
    MANAGER --> API
    HOST -. optional .-> DLLMCP
```

## 4. 责任边界

| 组件 | 负责 | 不负责 |
| --- | --- | --- |
| Desktop Shell | 窗口、托盘、菜单、单实例、更新、系统对话框 | 环境状态机、DLL 调用、数据库业务 |
| Dashboard | 展示、表单、筛选、批量操作、AI 助手界面 | 直接调用 DLL、直接访问 CDP、保存密钥 |
| Local Manager | 单写者、SQLite、operation、设置、事件、权限、诊断 | 浏览器进程细节、SDK 内部协议 |
| SDK Runtime Host | 加载 DLL、注册回调、执行 SDK C API、转发事件 | 持久化业务状态、UI 决策 |
| SDK Adapter | JSON 请求/响应、内存释放、错误码和事件归一化 | 产品状态推断、重试策略 |

## 5. 项目目录建议

```text
brosdk-dashboard/
  apps/
    desktop/                 Tauri app，窗口与托盘
    dashboard/               React Dashboard
  crates/
    manager/                 本地 Manager、operation 队列、SQLite
    sdk-host/                独立进程，加载 brosdk 动态库
    sdk-client/              Manager 侧 IPC client
    sdk-ffi/                 libloading 绑定和 C ABI 类型
    domain/                  领域模型、API schema、事件类型
    platform/                文件选择器、进程锁、密钥库、路径
    local-api/               可选 loopback API，用于开发/E2E/外部接入
  libs/
    windows_x64/
      brosdk.dll
      brosdk.h
  docs/
```

## 6. 数据模型

首版保留本地事实来源，远端 SDK 环境作为可同步资源：

- `EnvironmentRecord`：本地显示名、分组、标签、远端 `envId`、绑定代理/指纹摘要、最近运行状态。
- `RuntimeInstance`：易失态，包含 `envId`、generation、SDK reqId、PID/CDP/ready/exit fact。
- `ProxyProfile`：本地代理配置，密码进入系统密钥库。
- `FingerprintProfile`：先显示 SDK 返回的指纹摘要和预览；完整本地编辑器后续移植。
- `KernelRecord`：来自 SDK 内核清单和本地目录扫描的合并视图。
- `Operation`：所有启动、停止、创建、更新、删除、安装、诊断都必须进入 operation。
- `Settings`：数据目录、工作目录、扩展目录、SDK API URL、启动策略、日志级别。

## 7. API 与事件形态

Dashboard 首选 Tauri command/event：

```text
manager.snapshot()
manager.listEnvironments(filter)
manager.createEnvironment(input)
manager.startEnvironment(envId, options)
manager.stopEnvironment(envId)
manager.listOperations(filter)
manager.updateSettings(input)
```

为测试、外部自动化和未来 MCP 保留可选 loopback API：

```text
GET  /api/v1/overview
GET  /api/v1/environments
POST /api/v1/environments/{id}/start
POST /api/v1/environments/{id}/stop
GET  /api/v1/operations
GET  /api/v1/events
```

mutation 都返回 operation，不等待浏览器完全 ready。事件必须有递增 sequence，便于 Dashboard 刷新后恢复。

`brosdk.dll` 本身已经包含内嵌 MCP / HTTP 能力。新客户端把它作为 `sdk-host` 的 platform capability 暴露：当 Manager 明确配置端口时，由 `sdk_init` 的 `port` 字段启用 DLL 内嵌端点；Dashboard 不直接依赖该端点，仍通过 Manager 统一处理 envId 路由、operation 状态、安全策略和未来审批。

## 8. 运行状态语义

环境状态分为：

```text
stopped -> preparing -> starting -> ready -> stopping -> stopped
              |            |          |
              +----------> failed <---+
```

状态来源合并规则：

- `sdk_browser_open` 返回 reqId 只表示 accepted。
- `browser-open-success` 表示 SDK 认为 CDP ready，是进入产品 ready 的必要条件。
- `sdk_browser_info` 用于重连、手动关闭浏览器后的对账。
- 手动关闭浏览器必须触发状态从 `ready` 变为 `stopped` 或 `failed`，不能长期停留在运行中。
- 启动期间看到窗口闪退时，不靠固定短超时判断；以 SDK 回调、进程退出事实和 `sdk_browser_info` 对账。

## 9. 内部通讯

内部 Manager 与 SDK Runtime Host 优先使用：

- Windows：named pipe。
- macOS/Linux：Unix Domain Socket。

消息格式采用长度前缀 JSON：

```json
{
  "id": "uuid",
  "type": "sdk.browser.open",
  "payload": {
    "envs": [
      { "envId": "..." }
    ]
  }
}
```

长度字段是 4 字节大端无符号整数，单帧上限 32 MiB。两侧都由专用 reader task 连续读取完整帧，再把已解析消息送入业务 channel；不能把“读取长度 + 读取 body”的 future 直接放进会被取消的 `select` 分支，否则半帧取消会让后续消息错位。

Manager 侧 `RuntimeHost` actor 同时监督子进程退出、请求响应、超时和事件广播。正常 `Shutdown` 进入 `stopped`；进程被 kill、崩溃、锁冲突退出或 IPC 异常进入 `degraded`。Manager 不做无限自动重启，桌面 UI 始终留在父进程中。

回调事件统一从 host 推送给 Manager：

```json
{
  "type": "sdk.event",
  "code": 0,
  "name": "browser-open-success",
  "payload": {},
  "receivedAt": "2026-07-25T00:00:00.000Z"
}
```

DLL callback 函数只在回调有效期内复制 `data/len` 到无界队列，不解析 JSON、不访问数据库。`sdk-host` 主任务随后完成 JSON 解析、敏感字段脱敏、递增 sequence、envId 提取，以及异步 SDK reqId 到 operation id 的映射。

## 10. 安全边界

- API Key 只来自环境变量或系统密钥库，不写入文档、仓库、SQLite 明文字段或普通日志。
- userSig、代理密码、Cookie、CDK/DEK、Authorization、URL query 中的敏感值统一脱敏。
- Dashboard 不展示完整密钥，只显示来源、最后校验时间和尾号摘要。
- 可选 loopback API 默认只监听 `127.0.0.1`，mutation 检查 Origin。
- 诊断包默认不包含密钥、Cookie 明文、代理密码和完整启动 URL。

## 11. 与当前 windows-webview 的继承关系

可直接继承：

- Dashboard 信息架构：总览、环境、指纹、代理、内核、操作、设置、AI 助手。
- 白色扁平视觉风格和最小窗口约束。
- 操作队列、事件驱动、ready 不等于 accepted 的产品语义。
- 全局 MCP 使用 `envId` 参数路由的思路。
- 目录选择、扩展选择、离线内核包导入等交互。

需要重写或隔离：

- 当前 C++ WebView2 launcher 替换为 Tauri shell。
- 当前 Node local-manager 替换为 Rust Manager，或短期保留 HTTP API 兼容层供迁移。
- 当前 N-API binding 替换为 `sdk-host` 独立进程加载 `brosdk.dll`。
- 当前 v3 本地运行时能力与 v2 DLL 能力不完全一致，先按 `libs/windows_x64/brosdk.h` 暴露的能力落地。

## 12. 跨平台策略

Windows x64 是首个完成平台。macOS/Linux 不在没有动态库和平台验证时承诺功能完成：

- 动态库目录按 `libs/<platform>_<arch>` 组织。
- `sdk-ffi` 通过平台 resolver 选择库名和加载路径。
- `sdk-host` 的 IPC、进程监督、日志路径和崩溃恢复按平台 adapter 实现。
- Dashboard 和 Manager domain 不包含 `cfg(windows)` 业务分支。
- 若某个平台 SDK 不支持某能力，Dashboard 显示 capability，而不是隐藏失败。
