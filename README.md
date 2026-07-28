<p align="center">
  <img src="apps/desktop/src-tauri/icons/128x128.png" width="96" height="96" alt="BroSDK Dashboard icon">
</p>

# BroSDK Dashboard

BroSDK Dashboard 是基于 BroSDK 的 Windows 多指纹浏览器桌面控制台，提供多环境生命周期、代理与内核管理、远端指纹查看、全局 MCP 自动化和受控 AI Agent。

当前版本面向 Windows x64。仓库已包含运行所需的 `libs/windows_x64/brosdk.dll` 和 C API 头文件 `brosdk.h`；服务端接口快照 `doc.json` / `docs.json` 仅作本地参考，不进入版本库。

后续产品方向会围绕跨境电商多店铺运营扩展：不是直接做传统大 ERP，而是在多环境指纹浏览器之上增加店铺环境绑定、订单发货、商品/SKU 同步、平台连接器和受控 Commerce Agent。详见 [跨境电商运营中台规划](docs/commerce-roadmap.md)。

## 核心能力

- 首次启动输入 API Key，按 `getUserSig(role=user) -> sdk_init` 完成初始化；API Key 使用 Windows DPAPI 保护，不写入 SQLite、日志或发布清单。
- 以服务端数据为事实来源，以 `envId` 为环境唯一主键；本地 SQLite 只保存可删除、脱敏且带新鲜度状态的缓存。
- 创建环境只要求选择代理和已安装内核版本，其余指纹参数使用服务端策略。
- 内核管理合并 API Key `/api/v2/browser/kernelList`、`sdk_init` 返回的 `kernelVersions`、后续 `sdk_info` catalog 和本地已安装 cores；未配置 SDK API URL 时默认使用 `https://api.brosdk.com`，刷新按钮会重新对账最新服务端清单。
- 支持环境创建、同步、启动、进度回调、停止、更新、删除、详情和关键指纹查看。
- 运行时由隔离的 `sdk-host.exe` 加载 DLL；Dashboard 和 Host 均为 Windows GUI 子系统，不显示终端窗口。
- 应用单实例运行；关闭主窗口后驻留系统托盘，重复启动会唤醒已有窗口，避免相同 appId 被重复初始化。
- 使用 DLL 全局 `/sdk/v1/mcp` 入口，未指定端口时自动选择本机环回端口；页面工具采用 `env.* + arguments.envId`，Manager 强制注入选中环境并实施工具白名单。
- AI 会话创建时固定为“全局”或“单环境”：全局会话绑定全局 MCP 目录，单环境会话绑定所选环境且创建后不可修改作用域。Agent 生命周期由 DLL 全局 `browser.open/browser.close` 执行，但仍经过 Manager 的审批、状态机、operation 与幂等校验；Chat 读取、Agent 规划和执行前强制调用全局 `browser.status` 获取当前运行列表，不使用上次客户端遗留的 ready/stopped 缓存。会话历史保存在本地，发送、回复和执行状态更新后自动跟随最新消息。

## 界面预览

### 运行总览

![BroSDK Dashboard 运行总览](docs/assets/dashboard-overview.png)

### 多环境工作台

![BroSDK Dashboard 多环境工作台](docs/assets/environment-workspace.png)

### AI Agent

![BroSDK Dashboard AI Agent 工作台](docs/assets/ai-agent-workspace.png)

截图来自内置只读预览数据，可通过 `npm run docs:screenshots` 在 1440x900 视口重新生成；脚本同时检查浏览器错误与横向溢出。

## 架构

```text
React Dashboard
      |
      | Tauri command / event
      v
Manager + SQLite cache + secure credentials
      |
      | named pipe, framed JSON
      v
sdk-host.exe
      |
      | BroSDK C ABI
      v
brosdk.dll ---- SDK server / browser runtime / global MCP
```

Dashboard 不直接加载 DLL、访问 CDP 或调用 `/api/v2/sdk/*`。环境管理来自 `/api/v2/browser/*` 及对应 DLL C API，运行状态以 DLL callback 和 `sdk_browser_info` 对账结果为准。

## 开发环境

- Windows 10/11 x64
- Node.js 当前 LTS 与 npm
- Rust stable，目标工具链 `x86_64-pc-windows-msvc`
- Visual Studio Build Tools（MSVC 与 Windows SDK）
- Microsoft Edge WebView2 Runtime
- 可用的 BroSDK API Key 和 SDK 服务网络连接

安装依赖并启动真实桌面开发版：

```powershell
npm ci
npm run tauri:dev
```

首次打开时输入 API Key 完成初始化。`npm run dev` 只启动 Dashboard 前端，适合 UI 预览，不提供 DLL、托盘和本机 mutation 能力。

发布版首次启动只要求填写 API Key。SDK API URL 留空时采用 BroSDK 参考客户端一致的默认服务 `https://api.brosdk.com`；只有连接私有部署、测试服或内网 Swagger 服务时才需要在设置页覆盖。

## 构建与打包

前端生产构建：

```powershell
npm run build
```

Tauri 原始构建入口：

```powershell
npm run tauri:build
```

推荐使用仓库发布脚本生成 Windows x64 的 NSIS 安装包和便携 ZIP，并校验文件布局、版本、SHA-256、PE GUI subsystem 和签名状态：

```powershell
npm run release:windows
npm run release:verify
```

只生成便携包：

```powershell
npm run release:portable
npm run release:verify:portable
```

额外生成企业部署用 MSI：

```powershell
npm run release:windows:msi
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-windows-release.ps1 -RequireMsi
npm run release:test:msi
```

### `target` 与 `dist` 的区别

| 目录 | 用途 | 是否交付 |
| --- | --- | --- |
| `target/` | Cargo/Tauri 的编译缓存、目标文件、原始可执行文件和原始 bundle；体积大，可删除后重建 | 否 |
| `apps/dashboard/dist/` | Vite 生成的前端静态资源，作为 Tauri 构建输入 | 否 |
| `dist/release/` | 发布脚本从原始产物中整理出的安装包、便携目录、ZIP 和校验清单 | 是 |

之所以分开，是因为 `target/` 和 `apps/dashboard/dist/` 遵循各自工具链的构建约定，可能包含缓存与中间文件；`dist/release/` 只保留可发布给用户的经过校验的成品。三个目录均可由命令重新生成，不提交 Git。

默认发布布局：

```text
dist/release/
  BroSDK-Dashboard-<version>-windows-x64-setup.exe
  BroSDK-Dashboard-<version>-windows-x64-portable.zip
  WINDOWS-RELEASE-MANIFEST.json
  BroSDK-Dashboard-portable/
    BroSDK Dashboard.exe
    sdk-host.exe
    brosdk/brosdk.dll
    RELEASE-MANIFEST.json
```

更多签名、安装器、升级和回滚说明见 [Windows 发布文档](docs/windows-release.md)。

## 测试

本地静态检查和单元测试：

```powershell
npm run check
npm test
npm run e2e:dashboard
```

桌面与运行时测试：

```powershell
npm run sdk:capabilities
npm run sdk:runtime-smoke
npm run e2e:dashboard:desktop
npm run e2e:tray
```

真实账号测试只从当前进程环境变量或安全输入读取凭据，禁止把 API Key 写进命令脚本、仓库、日志和测试报告：

```powershell
$env:BROSDK_API_KEY = Read-Host "BroSDK API Key"
npm run e2e:environment
Remove-Item Env:BROSDK_API_KEY
```

快速验证本机 Host、Manager、服务端内核列表和 DLL MCP 的连通性：

```powershell
$env:BROSDK_API_KEY = Read-Host "BroSDK API Key"
npm run manager:smoke
Remove-Item Env:BROSDK_API_KEY
```

报告中的 `kernelRefresh.serverKernelListLoaded=true` 且 `kernelRefresh.count > 1` 表示内核页会显示服务端清单，而不只是本地已安装 core。

环境 E2E 会启动真实浏览器并调用全局 MCP；测试结束必须将环境恢复为 stopped。创建/多环境测试会使用临时环境并执行补偿清理。完整命令和安全约束见 [测试交接文档](docs/testing-handoff.md)。

真实 AI 会话与自动 Agent 回归：

```powershell
$env:BROSDK_AI_API_KEY = Read-Host "AI API Key"
$env:BROSDK_E2E_ENV_ID = "<existing-env-id>"
npm run e2e:ai-assistant
npm run e2e:ai-assistant:desktop
Remove-Item Env:BROSDK_AI_API_KEY, Env:BROSDK_E2E_ENV_ID
```

第二条命令会启动真实 Tauri 窗口，在隔离数据目录中复用 DPAPI 加密凭据，自动创建全局 Agent 会话并验证停止环境的实时状态查询；测试不会把密钥明文写入仓库或报告。

## MCP 与 AI Agent

新版 DLL 只需要一个全局 MCP endpoint：

```text
http://127.0.0.1:<embedded-port>/sdk/v1/mcp
```

Manager 从运行时 `tools/list` 动态发现工具及 `inputSchema`。所有调用都连接同一个 DLL 全局 endpoint，但会话的模型工具目录按作用域生成：全局会话获得全局读取、带显式 envId 的 `browser.open/browser.close`，以及 DLL 广告的 `env.navigate/env.tabs/...` 多环境浏览器工具；单环境会话获得同一批页面工具但隐藏 envId。单环境调用由 Manager 覆盖写入绑定的 `arguments.envId`；`env.list/resolve/get/create/update/destroy` 不会混入页面工具白名单。

端口设置留空时，Manager 会在 SDK 初始化前自动选择一个可用的 `127.0.0.1` 端口并传给 DLL。设置具体端口只用于需要固定 endpoint 的集成场景，修改后需重启 Runtime Host。

Chat 与 Agent 都通过 OpenAI-compatible 原生 `tools/tool_calls` 接入。Chat 只允许当前会话目录中的读取工具；Agent 从 DLL 目录额外绑定 `browser.open/browser.close` 和页面工具，并把函数调用转换为可批准计划或最多 20 轮的自动工具循环。全局 Agent 可在请求中明确指定任一 envId；单环境 Agent 只能操作创建时绑定的 envId，提示词中的其它环境会被拒绝。自动执行卡片会展示每一步的工具名、envId、operation 和脱敏后的工具参数，便于区分一次导航和后续读取/确认等连续 `mcp.call`。执行前的 `browser.status`、生命周期调用和 DLL callback 都由 Manager 关联到同一 operation；AI 不直接持有 API Key、userSig、完整 CDP URL 或代理凭据。

## 数据与安全

- 默认用户数据目录：`%LOCALAPPDATA%\BroSDK Dashboard`
- API Key 和 AI Provider Key：Windows DPAPI 保护
- 环境列表与详情：服务端为事实来源，本地只保留脱敏缓存
- 运行状态：每次客户端启动通过新 Runtime Host 重新对账
- 桌面实例：应用标识 `com.brosdk.dashboard`，同一用户会话只保留一个实例
- 发布签名：仓库不保存证书；内部构建会明确报告 `NotSigned`

## 文档

- [项目规划与当前状态](docs/README.md)
- [架构与进程边界](docs/architecture.md)
- [DLL C API 接入](docs/dll-integration.md)
- [接口覆盖矩阵](docs/interface-coverage.md)
- [跨境电商运营中台规划](docs/commerce-roadmap.md)
- [Manager 领域模型](docs/manager-domain.md)
- [实施路线图](docs/roadmap.md)
- [Windows 发布与回滚](docs/windows-release.md)

## License

本仓库当前标记为 `UNLICENSED`，未授予开源使用许可。`brosdk.dll` 及其头文件的分发和使用应遵循 BroSDK 对应授权条款。
