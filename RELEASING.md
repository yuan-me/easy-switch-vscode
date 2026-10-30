# 发布版本

使用 GitHub Releases 分发，不发布到 VS Code Marketplace。

## 发布前

确认源代码不含真实服务商配置、API 密钥、登录令牌、聊天、服务器诊断或代理配置。`extensions/vscode/scripts/validation` 只保存合成测试的证据；不要把真实运行数据放进去。

在 `extensions/vscode` 目录运行：

```sh
npm ci
npm version patch --no-git-tag-version
npm test
node --test scripts/package.test.mjs
```

检查 `package.json` 与 `package-lock.json` 的版本一致，并更新 `RELEASE_NOTES.md`。如果新增 Codex 支持版本，先按兼容性说明验证，不能只扩展版本白名单。

## 创建 Release

提交源码和发布说明，再推送与扩展版本一致的标签。例如扩展版本为 `0.1.5` 时：

```sh
git tag v0.1.5
git push origin main
git push origin v0.1.5
```

`VS Code Extension` 工作流会编译两个 Linux 架构、运行测试和打包审计。全部通过后，创建正式 Release，附上 `easy-switch-remote-0.1.5.vsix` 和对应的 `.vsix.json` 校验报告。工作流使用 GitHub 自带的临时令牌，无需在插件里保存 PAT。

若工作流失败，先修复构建或测试。没有完成附件上传的版本不会触发插件更新提醒。不要用桌面版安装程序替代 VSIX，也不要覆盖已发布版本的附件；修复应发布新版本。

发布后，用旧版插件执行一次“检查更新”，核对版本号与发布页；下载安装并重载后，再检查一次，应显示当前已是最新正式版。新版本还需要在真实 Remote-SSH 环境完成切换与原聊天继续发送验证。
