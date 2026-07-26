# 服务端、DLL 与 MCP 覆盖审计

审计日期：2026-07-26。审计依据：仓库 `doc.json`、`browser-open-server/modules/open/router/browser`、`libs/windows_x64/brosdk.h`、DLL 源码 `orbitbridge/projects/brosdk` 及参考客户端 `orbitbridge/projects/brostu`。

## 结论

当前项目已经形成可安装、可初始化、可管理多环境并可通过 AI 手动或自动执行受控操作的 Windows 主链路，但不能再描述为“所有服务端和 DLL 功能均已产品化”。环境数据继续以 SDK 服务端为事实来源；本地 SQLite 只是脱敏缓存与运行态，AI 会话则是单独的本机用户数据。

已完成的主链路：

- API Key 安全初始化、`getUserSig(role=user)`、DLL 初始化与完整环境分页同步。
- 环境创建、详情、元数据更新、删除、批量启停、callback/browser info 对账和本地数据清理。
- 指纹关键字段、代理、内核、操作记录、CDP/MCP 运行信息与诊断摘要。
- DLL 全局和单环境 MCP 的可选 envId 路由、动态发现、严格 session lifecycle、Manager 策略与响应脱敏。
- AI Provider、持久化会话历史、关联环境、受控计划、会话级手动/自动执行、幂等 reservation 和真实状态校验。

仍需产品化的主要能力：Cookie 读取/导入导出、userSig 在线刷新、扩展选择与启动参数、服务端专家指纹选项/全局指纹配置，以及面向高风险 MCP 工具的专用参数 UI。跨平台仍等待对应动态库。

## `/api/v2/browser/*`

这些接口使用 API Key。Dashboard 不重复实现一套直连 CRUD client：除 `getUserSig` 外，环境请求由 DLL 使用 userSig 访问 `/api/v2/sdk/*`，两条服务端链路复用 BrowserApi/DTO；因此“未直连”不代表环境来自本地数据库。

| 服务端接口 | 当前产品覆盖 | 状态 |
| --- | --- | --- |
| `getUserSig` | 首次初始化和换号，固定 `role=user` | 已产品化 |
| `create` | `sdk_env_create`，普通模式只提交代理和已安装内核 | 已产品化（DLL 链路） |
| `destroy` | `sdk_env_destroy`，stopped 门禁和二次确认 | 已产品化（DLL 链路） |
| `info` | `sdk_env_getinfo`，只缓存脱敏关键字段 | 已产品化（DLL 链路） |
| `page` | `sdk_env_page` 完整分页、总数校验、envId 去重和原子替换 | 已产品化（DLL 链路） |
| `updateEnv` | `sdk_env_update` 的名称/序号最小更新与服务端回显校验 | 已产品化（DLL 链路） |
| `update` | 完整指纹/代理更新 DTO 未开放给普通 Dashboard | 部分覆盖 |
| `archList`、`platformList`、`kernelIdList`、`kernelList` | 当前内核页以 DLL 本地安装/运行信息为主，没有直连专家 catalog | 未产品化 |
| `getUiFingerList` | 普通创建依赖服务端默认指纹，没有专家字段选择器 | 未产品化 |
| `getGlobalFinger`、`setGlobalFinger` | 没有全局指纹模板管理 UI | 未产品化 |

普通用户创建环境只选代理和内核仍是正确产品边界；专家指纹配置应作为独立高级模式，不能重新塞回默认创建表单。

## DLL C API

当前 Rust FFI、隔离 Host 和 Manager 已实际接通：

- 回调/生命周期：`sdk_register_result_cb`、`sdk_register_log_cb`、`sdk_init`、`sdk_info`、`sdk_shutdown`。
- 认证：`sdk_get_user_sig`。
- 环境：`sdk_env_create`、`sdk_env_update`、`sdk_env_page`、`sdk_env_getinfo`、`sdk_env_destroy`。
- 浏览器：`sdk_browser_install`、`sdk_browser_cleanup`、`sdk_browser_info`、`sdk_browser_open`、`sdk_browser_close`。
- 自动化/诊断：`sdk_browser_command`、`sdk_browser_env_check`、`sdk_browser_snapshot`、`sdk_network_diagnostics`、`sdk_system_proxy_diagnostics`。
- 内存/错误：`sdk_free` 以及可选错误名称/文本读取。

尚未接通：

| C API | 影响 | 优先级 |
| --- | --- | --- |
| `sdk_env_get_cookies` | Dashboard 目前没有真实 Cookie 查看、导入和导出主流程 | P1 |
| `sdk_token_update` | API Key/userSig 更新当前通过重建 Host 完成，不能在线刷新 | P1 |
| `sdk_register_cookies_storage_cb` | 没有宿主级 Cookie/Storage 拦截与替换策略 | P1/P2 |
| `sdk_register_security_decision_cb` | 没有自定义 bridge security redirect 决策 | P2 |
| `sdk_init_async` | 同步 init 已被隔离在 Host，不阻塞桌面 UI；不是当前缺陷 | 不需要 |
| `sdk_init_cpp` | Rust 只使用稳定 C ABI，避免 C++ vtable ABI | 不需要 |
| `sdk_init_webapi` | 兼容接口；当前通过 `sdk_init.port` 启用 MCP/Web API | 不需要 |

Capability 只能报告实际绑定能力。阶段 19 已移除未绑定 cookie/security callbacks 和 `sdk_token_update` 的误报。

## DLL MCP

DLL 不缺少单环境 MCP。当前源码和真实运行时的单环境 catalog 包含 `browser_state`、`tabs`、`bookmarks`、`history`、`tab_groups`、`navigate`、`snapshot`、`diff`、`act`、`download`、`upload`、`read`、`grep`、`screenshot`、`pdf`、`wait`、`windows`、`evaluate` 等工具。具体数量以当次 `tools/list` 为准，当前实测为 18，不在 Dashboard 写死。

Manager 当前策略：

- 全局允许 9 个只读工具：`sdk.health`、`sdk.info`、`env.list`、`env.resolve`、`env.get`、`browser.status`、`task.list`、`task.get`、`mcp.endpoint`。
- 单环境请求必须提供一个已同步且 `ready` 的 envId。client 将 `None/Some(envId)` 统一映射到 `/sdk/v1/mcp` 与 `/sdk/v1/mcp?envId=...`，原有 global/env Rust 函数入口继续兼容调用方。
- 单环境允许 DLL 当次广告的全部工具；Manager 在调用前要求参数为 JSON object，并限制总大小 64 KiB、嵌套 16 层、单字符串 16 KiB。DLL schema 仍是字段级事实来源。
- Dashboard 为常用读取工具提供结构化表单，其余工具提供高级 JSON 参数入口。Agent 使用 `mcp.call` 调用任意单环境广告工具；`mcp.read` 保留旧的 7 工具严格参数策略作兼容。
- 环境 CRUD 和浏览器启停不从全局 MCP 直通，继续使用有状态校验和 operation 追踪的 Manager API。单环境导航、点击、输入、脚本、上传下载等只在用户进入 MCP 控制台或 Agent 执行模式后调用。

所有 MCP 调用仍执行动态 `tools/list` 校验、operation 追踪和响应脱敏。`browser_state` 依赖扩展桥时可能返回 `BRIDGE_NOT_READY`；真实 E2E 因此使用可由 CDP 后端完成的 `tabs(list)` 验证环境路由，并不把工具“已广告”误报成其所有后端能力都已就绪。

## AI 会话与执行

阶段 20 后：

- “会话”与“关联环境”是两个概念。会话保存用户/AI 消息；关联环境只决定当前请求附带哪个 envId 的脱敏运行摘要。
- 会话保存在 WebView 本地存储，最多 20 个会话、每个 80 条消息；请求只携带最近 40 条，Manager 再限制单条 16 KiB、总计 128 KiB。
- 支持新建、切换、清空和删除；Dashboard 不主动注入 API Key、userSig、完整 CDP URL 或 SDK 原始响应。用户手动输入的文本会原样进入本地会话，因此不应在对话框粘贴凭据。
- 用户文本中只出现一个已同步 envId 时，该 envId 优先于旧关联环境。Manager 在计划返回 UI 前写入真实 `expectedState` 和 UUID 幂等键。
- 每个会话保存独立执行方式，默认“每次批准”；用户可显式切换“自动执行”，计划生成后立即调用 Manager。两种方式执行前都再次比较当前状态；如果状态在计划后发生变化，会显示具体错误且不会调用 DLL。
- 执行一旦尝试，无论成功或失败都不会再次显示同一计划的批准按钮，避免复用状态不确定的幂等键。

会话历史目前不加密、不跨设备同步，也不进入 Manager SQLite。它属于可清除的本机 UI 数据，不是环境数据缓存；清空或删除会话会立即更新本地存储。后续若提供导出/云同步，必须单独做敏感内容和用户确认设计。

## 后续顺序

1. P1：Cookie 查看、导入、导出及敏感字段/文件边界。
2. P1：`sdk_token_update` 在线 userSig 刷新与失败回退到 Host 重建。
3. P1：本地扩展扫描、manifest 校验和按环境启动选择。
4. P2：专家指纹选项与全局指纹模板，不改变普通创建的两字段流程。
5. P2：为上传、脚本执行、下载/PDF 等 MCP 工具提供 schema 驱动的专用 UI、风险分级和 artifact 保留策略。
