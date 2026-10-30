import * as vscode from 'vscode';
import { randomUUID } from 'node:crypto';

export const stopWarning = '请先手动停止所有使用此目录的 Codex 后端，包括当前窗口、其他 VS Code 窗口和 CLI。仅停止生成或关闭侧边栏，不代表后端已经退出。';
const escape = (text: string) => text.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!));

export function confirmManualStop(title: string, detail: string, action: string): Promise<boolean> {
  const panel = vscode.window.createWebviewPanel('easySwitch.confirm', 'Easy Switch · 操作确认', vscode.ViewColumn.Active, {
    enableScripts: true, localResourceRoots: []
  });
  const nonce = randomUUID();
  panel.webview.html = `<!doctype html><html lang="zh-CN"><head><meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}'">
<meta name="viewport" content="width=device-width, initial-scale=1">
<style nonce="${nonce}">
body { max-width: 720px; margin: 32px auto; padding: 0 24px; line-height: 1.7; font-family: var(--vscode-font-family); }
h1 { font-size: 22px; } .warning { color: var(--vscode-errorForeground, #c72e2e); border-left: 4px solid currentColor; padding: 8px 16px; font-weight: bold; font-size: 17px; }
.detail { white-space: pre-wrap; overflow-wrap: anywhere; } button { padding: 9px 16px; margin: 12px 12px 0 0; cursor: pointer; border: 1px solid var(--vscode-contrastBorder, transparent); color: var(--vscode-button-foreground); background: var(--vscode-button-background); font: inherit; }
button:focus-visible { outline: 2px solid var(--vscode-focusBorder); outline-offset: 3px; } #cancel { color: var(--vscode-button-secondaryForeground); background: var(--vscode-button-secondaryBackground); }
</style></head><body><h1>${escape(title)}</h1>
<div class="warning" role="alert">⚠ 操作前必须手动停止 Codex<p>${escape(stopWarning)}</p></div>
<p>插件不会检查进程或文件占用，不会调用 sudo，也不会自动停止任何后端。仍有进程写入时，可能覆盖新消息或损坏聊天；备份和文件校验不能完全消除这一风险。</p>
<div class="detail">${escape(detail)}</div>
<button id="cancel" autofocus>取消</button><button id="continue">我已停止 Codex，${escape(action)}</button>
<script nonce="${nonce}">const api = acquireVsCodeApi(); for (const id of ['cancel', 'continue']) document.getElementById(id).addEventListener('click', () => { document.getElementById('continue').disabled = true; api.postMessage({action:id}); });</script>
</body></html>`;
  return new Promise(resolve => {
    let done = false;
    const finish = (accepted: boolean) => {
      if (done) return;
      done = true; messages.dispose(); closed.dispose(); resolve(accepted); panel.dispose();
    };
    const messages = panel.webview.onDidReceiveMessage(message => {
      if (message?.action === 'continue') finish(true);
      else if (message?.action === 'cancel') finish(false);
    });
    const closed = panel.onDidDispose(() => finish(false));
  });
}
