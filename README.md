# Easy Switch

在 VS Code 的 Linux Remote-SSH 窗口中切换 Codex 的 ChatGPT 和 API 服务商，保留原聊天。

![Easy Switch](extensions/vscode/resources/icon.png)

## 安装

1. 从 [GitHub Releases](https://github.com/yuan-me/easy-switch-vscode/releases/latest) 下载 `.vsix`。
2. 用 Remote-SSH 连接 Linux 服务器，在该窗口运行“Extensions: Install from VSIX...”。确认 Easy Switch 和官方 Codex 都安装在 SSH 服务器端。
3. 重载窗口，在 Easy Switch 侧边栏添加服务商。接口需要支持 Responses API。

安装包同时包含 Linux x64 和 ARM64 工具，服务器无需编译 Rust。当前支持的官方 Codex 版本和已完成的测试见 [兼容性说明](extensions/vscode/COMPATIBILITY.md)。完整的配置、管理员检查器和恢复步骤见 [使用说明](extensions/vscode/README.md)。

## 修复损坏聊天

0.1.7 提供“Easy Switch: 修复聊天记录”。先查看侧边栏的逐项说明，确认后加密备份并修复，重载后再切换服务商。已取消跳过损坏聊天的方案。只能处理有明确证据的截断模式；缺失图片和未知尾部无法还原，操作前会说明。详细范围见[使用说明](extensions/vscode/README.md)。

## 更新

启动后自动检查 GitHub 正式版，最多每天一次；窗口持续打开时也会检查。同一个新版本只自动提醒一次。点击“查看更新”打开发布页，下载安装包后，在 SSH 窗口中通过 VSIX 更新并重载。不会自动安装或中断聊天。

侧边栏右上角和命令面板都有“Easy Switch: 检查更新”。在设置中关闭 `easySwitch.autoCheckUpdates` 可停用自动检查，手动检查仍可用。检查从远端发起，需要服务器能访问 `api.github.com`；失败时请检查远端网络或代理。

更新检查不发送服务商配置、密钥或聊天。只有带正确名称 VSIX 附件的正式 Release 才会提示更新，草稿和预发布版不参与检查。未发布新版本或自动检查失败时，不弹出打扰提示。

## 开发与发布

这是从 [Easy Switch](https://github.com/yuan-me/easy-switch) 拆出的 VS Code 扩展仓库。`extensions/vscode` 是界面与安装包，`crates/vscode-helper` 是远端切换工具，`crates/core` 是共享 Rust 核心。

构建步骤见使用说明。GitHub Actions 会在 Linux x64 和 ARM64 上运行测试、编译工具，再运行扩展测试与包审计。推送与 `extensions/vscode/package.json` 版本一致的 `v版本号` 标签后，测试通过才会创建 Release，并附上 VSIX 和 SHA-256 审计报告。发布步骤见 [发布说明](RELEASING.md)。

当前启用的 API 密钥仍明文保存在服务器配置中，文件限制为当前用户访问。管理员可能读取它；请勿上传 Codex 配置、聊天目录或账号文件。部分跨服务商加密历史可能无法继续使用，遇到这种情况可保留聊天并切回原服务商。
