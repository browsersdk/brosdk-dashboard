# 当前状态

更新日期：2026-07-29。

## 发布成熟度

当前版本为 0.1.0，定位为 Windows x64 内测版。仓库可以构建 NSIS、便携 ZIP 和可选 MSI，并具备真实 DLL、环境生命周期、托盘和安装器 E2E，但内部产物尚未完成正式代码签名，客户端也没有完整自动更新通道。

macOS/Linux 只有路径、IPC 和安全存储 adapter；仓库未携带对应动态库，也没有平台发布验收，因此不属于当前支持范围。

## 能力矩阵

| 领域 | 当前能力 | 状态 |
| --- | --- | --- |
| 初始化 | API Key、getUserSig(role=user)、SDK init、安全持久化与换号清理 | 可用 |
| 环境 | 服务端分页同步、创建、详情、元数据更新、删除、批量启停 | 可用 |
| 运行时 | callback 进度、ready/stopped、BrowserInfo 对账、重启恢复 | 可用 |
| 指纹 | 关键字段只读展示和最多四环境对比 | 可用 |
| 代理 | 本地 profile、系统安全存储、绑定环境和诊断 | 可用 |
| 内核 | 服务端 catalog、本地扫描、平台过滤、版本/digest 比较、安装进度和重试 | 可用 |
| MCP | DLL 全局 endpoint、动态 tools/list、envId 注入和响应脱敏 | 可用 |
| AI | OpenAI-compatible Chat/Agent、不可变会话作用域、审批/自动模式和步骤审计 | 内测 |
| 信息架构 | 五个一级入口、组内二级标签、开发诊断能力收敛到系统区域 | 可用 |
| Windows 桌面 | 单实例、托盘、后台 Host、安装包和便携包 | 可用 |
| 本地 HTTP API | 只有地址与 crate 占位，没有产品化服务 | 未实现 |
| Commerce | 只有方向文档，没有模型、页面或 connector | 未实现 |
| macOS/Linux | adapter 占位，缺少动态库和发布验证 | 不支持 |

更细的服务端、C API 和 MCP 覆盖见 [接口覆盖审计](interface-coverage.md)。

## 已知限制

正式公开发布前必须处理：

1. Tauri CSP 当前未启用，需要建立与本地资源、Tauri invoke 和必要网络请求匹配的最小策略。
2. AI 会话历史当前保存在未加密 WebView localStorage，需要迁入 Manager 受保护存储，或增加默认不持久化模式。
3. Agent 自动执行缺少独立的工具风险等级。页面脚本、上传、下载、点击和表单写入不能只依赖会话级开关。
4. 正式安装包未签名；版本仍为 0.1.0，缺少稳定升级通道和面向用户的迁移/回滚验证。
5. Cookie 导入导出、扩展选择和批量创建尚未产品化，是多账号迁移与日常运营的明显缺口。
6. 环境缺少本地分组、标签、收藏和保存筛选，大规模环境下定位效率不足。
7. 仓库包含 brosdk.dll 和 brosdk.h；公开分发前必须确认 DLL 再分发条款，并确定项目自身许可证。

## 数据与状态边界

- envId 是环境唯一主键，名称允许重复。
- SDK 服务端是环境配置事实来源；本地环境和详情表是可删除缓存。
- 当前设备运行状态以 callback、sdk_browser_info 和 DLL 全局 browser.status 为准。
- API Key、AI Key 和代理密码不得进入 SQLite、日志、截图、测试报告或 AI prompt。
- Manager schema 当前为 version 7。
- DLL MCP 工具数量和 schema 动态发现，不承诺固定为 18 个。

## 质量基线

当前仓库已经覆盖：

- Dashboard 组件、TypeScript 和 production build。
- Rust workspace 单元测试、Rustfmt 和 Clippy。
- 桌面与移动视口 Playwright。
- 真实 API Key 的初始化、环境生命周期、双环境、MCP 和内核 catalog smoke。
- 真实 AI Provider 的 Chat、Agent 启停、重启、导航和状态查询。
- Windows 托盘、单实例、后台 Host、安装、卸载和便携包验证。

这些测试证明主链路可运行，但不等同于正式发布门槛。每个发布候选还需要执行 [路线图](roadmap.md) 中的安全、升级、规模和兼容性门禁。

## 当前优先级

1. P0：安全与发布基线，包括 CSP、AI 数据和工具风险、签名、版本及更新回滚。
2. P0：环境生命周期长期稳定性、Host 故障恢复和大规模环境性能。
3. P1：分组/标签/保存筛选、批量创建、Cookie 和扩展等运营效率能力。
4. P1：继续术语收敛和规模体验优化，让普通用户不必理解 MCP、CDP 和 operation。
5. P2：1.0 后验证店铺工作区与单平台只读 connector。
