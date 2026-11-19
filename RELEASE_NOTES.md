# Easy Switch 0.3.0

在 Linux Remote-SSH 窗口添加或编辑 API 服务商时，填好地址和密钥，点击“获取模型列表”。勾选希望在 Codex 中显示的模型，再选一个作为默认模型。保存后点击“切换并重载”即可生效。

- 模型列表支持搜索、全选和取消全选。每个服务商分别保存选择，刷新列表后会保留已有模型的勾选状态。要取消勾选默认模型，先换一个默认模型。
- 服务商未提供模型列表时，仍可手动填写模型 ID。更换 API 地址需重新输入密钥。
- 启用 Fast 后，已勾选的模型都可以选择 Fast，实际可用性取决于服务商。

在 VS Code 的 Remote-SSH 窗口运行“Extensions: Install from VSIX...”，选择本页的 `easy-switch-remote-0.3.0.vsix`，然后重载窗口。

已在隔离的 Linux x64 和 ARM64 环境验证 Codex 能读取勾选的模型列表。真实 Remote-SSH 会话中的供应商回复尚未验证。
