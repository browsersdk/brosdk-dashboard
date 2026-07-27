# Windows 发布与回滚

## 构建目录

构建和发布使用三个职责不同的目录：

| 目录 | 产生者 | 内容 | 用途 |
| --- | --- | --- | --- |
| `target/` | Cargo / Tauri | Rust 编译缓存、原始 exe、依赖与原始 NSIS/MSI bundle | 构建工作区，不直接交付 |
| `apps/dashboard/dist/` | Vite | Dashboard 前端静态资源 | Tauri 嵌入输入，不直接交付 |
| `dist/release/` | `scripts/build-windows-release.ps1` | 便携目录、ZIP、安装器和发布清单 | 最终交付目录 |

`target/` 与 `apps/dashboard/dist/` 服从工具链目录约定，会包含缓存和中间文件；发布脚本从中选择 Dashboard、`sdk-host.exe`、`brosdk.dll` 和安装器，重命名并校验后写入 `dist/release/`。三个目录都在 `.gitignore` 中，删除后可通过发布命令完整重建。Tauri 的 NSIS/WiX 下载缓存位于 `%LOCALAPPDATA%/tauri`，不因删除项目 `target/` 而重新下载。

## 产物与命令

Windows x64 默认交付 NSIS 安装包和便携 ZIP：

```powershell
npm run release:windows
npm run release:verify
```

若正在运行 `dist/release/BroSDK-Dashboard-portable`，Windows 会锁定其中的 DLL。不要强制结束用户进程，可改为在仓库 `dist/` 下的独立目录构建和验证：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-windows-release.ps1 -ReleaseDirectory dist/release-next
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-windows-release.ps1 -ReleaseRoot dist/release-next
```

产物统一写入 `dist/release`：

```text
dist/release/
  BroSDK-Dashboard-0.1.0-windows-x64-setup.exe
  BroSDK-Dashboard-0.1.0-windows-x64-portable.zip
  WINDOWS-RELEASE-MANIFEST.json
  BroSDK-Dashboard-portable/
    BroSDK Dashboard.exe
    sdk-host.exe
    brosdk/brosdk.dll
    RELEASE-MANIFEST.json
```

`WINDOWS-RELEASE-MANIFEST.json` 记录版本、目标三元组、产物类型、文件大小、SHA-256 和签名状态。`npm run release:verify` 同时检查：

- 便携目录的三个必需二进制及内部清单。
- Dashboard 和 `sdk-host.exe` 的 PE subsystem 都是 Windows GUI（值 2），直接启动不创建终端窗口。
- 便携 ZIP 可以打开且包含完整资源。
- NSIS 安装包版本与发布版本一致。
- 所有发布产物大小和 SHA-256 与总清单一致。
- 签名状态可读取；内部未签名构建不会因此失败。

只生成便携版：

```powershell
npm run release:portable
npm run release:verify:portable
```

企业部署需要 MSI 时显式运行：

```powershell
npm run release:windows:msi
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-windows-release.ps1 -RequireMsi
npm run release:test:msi
```

该命令生成 NSIS、MSI 和便携 ZIP 的完整组合。MSI 是可选产物，不阻塞默认 NSIS 发布；固定 `upgradeCode` 为 `30a62539-a61f-5ee6-a81e-064d37fd1968`。

`release:test:msi` 对每个语言 MSI 执行 Windows Installer administrative extraction，不注册或覆盖已安装产品，并检查 Dashboard、`sdk-host.exe` 和 `brosdk.dll`。

## 构建工具

`scripts/prepare-tauri-windows-tools.ps1` 自动准备 Tauri 固定版本的官方工具：

- NSIS 3.11 与 `nsis_tauri_utils` 0.5.3。
- MSI 模式额外准备 WiX 3.14.1。

下载使用重试、续传和固定哈希校验；缓存位于 `%LOCALAPPDATA%/tauri/NSIS` 和 `%LOCALAPPDATA%/tauri/WixTools314`，不进入仓库。缓存完整后可重复构建，不依赖 `PATH` 中偶然存在的 NSIS/WiX，也不会因清理项目 `target` 重新下载工具。

构建先用 `--no-bundle` 生成便携版，再单独生成安装器，避免将 Tauri 安装器 bundle 标记带入便携可执行文件。

## 安装器 E2E

桌面托盘生命周期可在 debug 或指定便携程序上独立验证：

```powershell
npm run e2e:tray
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-desktop-tray-e2e.ps1 -DesktopExecutable "dist\release\BroSDK-Dashboard-portable\BroSDK Dashboard.exe"
```

该测试关闭主窗口后确认进程继续运行，从托盘恢复窗口，再通过右键菜单退出。

首次启动、文件布局和卸载烟雾测试不需要测试凭据：

```powershell
npm run release:test:installer
```

该测试会：

- 拒绝覆盖机器上已有的 BroSDK Dashboard 安装。
- 静默安装到唯一系统临时目录。
- 检查桌面程序、`sdk-host.exe` 和 `brosdk.dll`。
- 启动安装后的 release，确认首次 API Key 初始化页可见。
- 静默卸载并确认程序文件和卸载注册信息已移除。

完整安装版 Dashboard E2E 使用安全输入提示读取 API Key：

```powershell
npm run release:test:installer:full
```

它在已安装 release 中完成 API Key 初始化、环境启动到 ready、AI 环境上下文、Provider 设置入口、环境停止和操作中心 envId 验证，然后静默卸载。测试凭据不会进入命令参数、仓库、发布清单或测试报告。

## WebView2

NSIS/MSI 使用 Tauri `embedBootstrapper` 模式。安装时检查 WebView2 Runtime，缺失或过旧时静默运行 Microsoft bootstrapper。便携包不携带 WebView2；目标机器需要预装 Evergreen WebView2 Runtime。

## 数据与卸载

release 默认数据目录：

```text
%LOCALAPPDATA%/BroSDK Dashboard
```

交互式 NSIS 卸载按系统语言使用中文或英文询问是否删除本地数据、日志和受保护凭据。静默卸载不弹框并保留用户数据，避免自动升级或企业卸载被阻塞。MSI 卸载保留用户数据；企业部署可另行清理该目录。

## 签名

仓库不保存证书和私钥。正式发布前使用组织代码签名证书签署：

- `BroSDK Dashboard.exe`
- `sdk-host.exe`
- NSIS/MSI 安装包

证书 thumbprint、时间戳 URL 和 CI secret 由发布环境注入。要求正式签名时运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-windows-release.ps1 -RequireSignature
```

未签名构建只用于内部测试。

## 升级与回滚

- `allowDowngrades=false` 阻止安装器直接覆盖为更低版本。
- MSI 版本升级复用固定 `upgradeCode`，由 Windows 识别为同一产品线。
- 回滚时先卸载当前版本并保留用户数据，再安装目标旧版本；SQLite schema 只做向前兼容迁移，因此回滚前应备份 `%LOCALAPPDATA%/BroSDK Dashboard`。
- 便携版回滚直接替换程序目录，不删除用户数据目录。
