# BroSDK Dashboard 文档中心

本目录描述当前产品、已经实现的能力、稳定架构边界和后续发布路线。文档默认面向公开仓库，不依赖维护者本机路径或未提交的接口快照。

## 建议阅读顺序

### 使用者与产品负责人

1. [产品定位](product.md)：目标用户、核心场景、产品边界和交互原则。
2. [当前状态](status.md)：当前版本能做什么、还不能做什么、数据归属和发布成熟度。
3. [正式发布路线](roadmap.md)：从当前内测版走到可靠多环境控制中心的里程碑和验收门槛。
4. [Windows 发布与回滚](windows-release.md)：安装包、便携包、签名、升级和回滚。

### 开发与维护

1. [架构与进程边界](architecture.md)
2. [Manager 领域模型](manager-domain.md)
3. [DLL C API 接入](dll-integration.md)
4. [服务端、DLL 与 MCP 覆盖审计](interface-coverage.md)
5. [测试与发布验证](testing-handoff.md)

### 后续方向

- [跨境电商方向验证](commerce-roadmap.md)：1.0 之后的可选垂直模块，不属于当前核心发布范围。
- [历史实施记录](history/implementation-history.md)：阶段 0-34 和后续补丁的详细开发、测试与验收记录。
- [变更记录](../CHANGELOG.md)：面向版本的能力摘要。

## 当前结论

BroSDK Dashboard 当前是 Windows x64 内测版多环境指纹浏览器控制中心。核心链路已经覆盖 API Key 初始化、环境生命周期、代理、内核、关键指纹、全局 MCP 和受控 AI Agent，但正式公开发布仍需要完成安全加固、版本与签名治理、更新回滚、AI 风险分级以及关键运营效率能力。

环境配置以 SDK 服务端为事实来源，envId 是唯一身份。本地 SQLite 只保存设置、operation、运行事实和可删除的脱敏缓存。未来允许增加分组、标签、收藏等本地工作区元数据，但必须与服务端环境字段分表保存，不能覆盖远端事实。

## 文档事实优先级

发生冲突时按以下顺序判断：

1. 当前源码、测试和发布脚本。
2. [当前状态](status.md) 与 [接口覆盖审计](interface-coverage.md)。
3. 稳定架构和领域文档。
4. 历史实施记录。

doc.json / docs.json 是本地接口参考快照，不进入 Git。公开文档只描述已经由源码、DLL 头文件或真实测试确认的契约，不要求读者拥有这些本地文件。
