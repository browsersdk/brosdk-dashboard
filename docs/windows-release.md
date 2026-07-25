# Windows 发布与回滚

## 产物

阶段 7 提供两类 Windows x64 产物：

- NSIS/MSI 安装包：`npm run release:windows`
- 便携 ZIP：`npm run release:portable`

发布脚本会先构建 `sdk-host.exe` sidecar，再构建 Tauri 桌面程序，并生成：

```text
dist/release/BroSDK-Dashboard-portable/
  BroSDK Dashboard.exe
  sdk-host.exe
  brosdk/brosdk.dll
  RELEASE-MANIFEST.json
```

`RELEASE-MANIFEST.json` 记录版本、目标三元组、文件大小和 SHA-256。构建产物不进入 Git。

构建便携包后运行 `npm run release:verify`，脚本会检查必需文件、清单路径、SHA-256 和文件大小。

## 构建机要求

便携包可在当前仓库脚本中直接构建。NSIS/MSI 安装包还要求 Windows 构建机提供 NSIS 和 WiX 工具；Tauri 会在 `useLocalToolsDir=true` 时缓存工具到 `target/.tauri/`。正式签名需要由发布环境注入证书 thumbprint、时间戳或签名命令，仓库不保存证书和私钥。

## WebView2

NSIS/MSI 使用 Tauri `embedBootstrapper` 模式。安装时先检查 WebView2 Runtime，缺失或过旧时静默运行 Microsoft bootstrapper。便携包不携带 WebView2；目标机器需要预装 Evergreen WebView2 Runtime。

## 数据目录

release 构建默认使用：

```text
%LOCALAPPDATA%/BroSDK Dashboard
```

NSIS 卸载结束时询问是否删除本地数据、日志和受保护凭据；默认可选择保留。MSI 卸载始终保留用户数据，企业部署可另行清理该目录。

## 签名

仓库不保存证书和私钥。正式发布前使用组织代码签名证书签署：

- `BroSDK Dashboard.exe`
- `sdk-host.exe`
- NSIS/MSI 安装包

证书 thumbprint、时间戳 URL 和 CI secret 由发布环境注入。未签名构建仅用于内部测试。

## 升级与回滚

- MSI 的 `upgradeCode` 固定为 `30a62539-a61f-5ee6-a81e-064d37fd1968`，版本升级不会被 Windows 识别为新产品。
- `allowDowngrades=false` 阻止安装器覆盖为更低版本。
- 回滚时先卸载当前版本并选择保留用户数据，再安装目标旧版本；SQLite schema 只做向前兼容迁移，因此回滚前应备份 `%LOCALAPPDATA%/BroSDK Dashboard`。
- 便携版回滚直接替换程序目录，不删除用户数据目录。
