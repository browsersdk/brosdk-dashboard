# Changelog

本文件记录面向版本的产品变化。详细开发过程和逐阶段测试结果见 docs/history/implementation-history.md。

## Unreleased

### Changed

- 将 Dashboard 一级导航收敛为“工作台、环境、自动化、资源、系统”，高级 MCP、操作记录和设置通过系统分区的二级标签访问。

### Security

- 为 Tauri WebView 启用最小 CSP，并新增 `npm run security:tauri` 防止生产策略回退到 `null`、通配源或 `unsafe-eval`。
- AI 会话默认改为仅内存保存；只有用户显式开启“保存历史”后才写入 WebView 本地存储。

### Documentation

- 明确产品为 Windows 多环境指纹浏览器控制中心，Commerce 不进入 1.0 核心范围。
- 新增产品定位、当前状态和正式发布路线。
- 将阶段 0-34 与后续补丁的实施流水归档为历史记录。
- 明确正式发布前的安全、AI 风险、签名、更新、许可和运营效率门槛。

## 0.1.0 - Internal Preview

### Core

- API Key 首次初始化、Windows 安全存储、换号和本地账号状态隔离。
- 以 envId 为唯一身份的服务端环境分页同步、创建、详情、更新、删除和批量启停。
- DLL 独立 Runtime Host、named pipe IPC、callback 进度、BrowserInfo 对账和客户端重启恢复。
- 代理 profile、关键指纹查看与对比、内核 catalog/安装/更新和 operation 中心。

### Automation

- DLL 单一全局 MCP endpoint 与动态 tools/list。
- 全局 env.* 多环境调用和单环境会话 envId 强制绑定。
- OpenAI-compatible Chat/Agent、会话历史、手动批准和会话级自动执行。
- Agent 生命周期使用 DLL 全局 browser.status/open/close，并在决策前强制实时对账。

### Desktop And Release

- Tauri 2 Windows 桌面、单实例、关闭到托盘、后台 sdk-host 和 GUI subsystem。
- NSIS、便携 ZIP、可选 MSI、发布清单、SHA-256 和安装/卸载验证。
- Dashboard 组件测试、Rust workspace 测试、Playwright、真实 SDK/MCP/AI 和发布 E2E。

### Kernel Fixes

- 服务端 kernelList、sdk_init/sdk_info catalog 和本地 cores 合并。
- 当前 platform/arch 过滤，避免展示其它系统内核。
- DLL callback 安装进度、无回调超时和可重试失败。
- versionCode 与 sha256/md5 对账；已是最新版本时禁用重复安装。
