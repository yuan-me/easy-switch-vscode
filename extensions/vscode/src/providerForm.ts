import * as vscode from 'vscode';
import { randomUUID } from 'node:crypto';
import type { Provider } from './storage';
import { escapeHtml as escape, svg } from './webview';

export interface ProviderInput { name: string; baseUrl: string; model: string; key: string; nativeFast: boolean }

export function providerFormHtml(existing: Provider | undefined, hasKey: boolean, nonce: string): string {
  const title = existing ? '编辑服务商' : '添加服务商';
  return `<!doctype html><html lang="zh-CN"><head><meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}'">
<meta name="viewport" content="width=device-width, initial-scale=1">
<style nonce="${nonce}">
*{box-sizing:border-box}body{color:var(--vscode-foreground);background:var(--vscode-editor-background);font-family:var(--vscode-font-family);font-size:14px;line-height:1.6;margin:0;padding:40px 28px}main{max-width:640px;margin:auto}h1{font-size:26px;font-weight:600;line-height:1.3;margin:0 0 10px}header p{color:var(--vscode-descriptionForeground);margin:0 0 32px}form{margin:0}.fields{display:grid;grid-template-columns:1fr 1fr;gap:22px 24px}.field{min-width:0}.wide{grid-column:1/-1}label{display:block;margin-bottom:7px;font-weight:500}.hint{display:block;color:var(--vscode-descriptionForeground);font-size:12px;margin:6px 0 0}input:not([type=checkbox]){width:100%;height:40px;padding:8px 11px;border:1px solid var(--vscode-input-border,var(--vscode-widget-border));border-radius:4px;background:var(--vscode-input-background);color:var(--vscode-input-foreground);font:inherit}input::placeholder{color:var(--vscode-input-placeholderForeground)}input:focus-visible,button:focus-visible{outline:2px solid var(--vscode-focusBorder);outline-offset:2px}input{caret-color:var(--vscode-input-foreground)}::selection{background:var(--vscode-editor-selectionBackground)}.secret{display:flex;position:relative}.secret input{padding-right:48px}.reveal{position:absolute;right:3px;top:3px;width:36px;height:34px;padding:8px;border:0;background:transparent;color:var(--vscode-descriptionForeground);cursor:pointer;border-radius:3px}svg{display:block;width:20px;height:20px}.reveal:hover{color:var(--vscode-foreground);background:var(--vscode-toolbar-hoverBackground)}.fast{display:flex;justify-content:space-between;align-items:center;gap:24px;margin-top:28px;padding:22px 0;border-top:1px solid var(--vscode-widget-border);border-bottom:1px solid var(--vscode-widget-border)}.fast label{font-size:15px;margin:0}.fast p{margin:4px 0 0;max-width:490px}.toggle{appearance:none;flex:0 0 36px;width:36px;height:21px;margin:0;border:1px solid var(--vscode-input-border,var(--vscode-descriptionForeground));border-radius:11px;background:var(--vscode-input-background);cursor:pointer;position:relative}.toggle:before{content:'';position:absolute;width:13px;height:13px;left:3px;top:3px;border-radius:50%;background:var(--vscode-descriptionForeground)}.toggle:checked{background:var(--vscode-button-background);border-color:var(--vscode-button-background)}.toggle:checked:before{left:18px;background:var(--vscode-button-foreground)}.error{color:var(--vscode-errorForeground);margin:18px 0 0;white-space:pre-wrap;overflow-wrap:anywhere}.error:empty{display:none}footer{display:flex;align-items:center;gap:10px;margin-top:28px}footer .hint{margin:0 auto 0 0}footer button{font:inherit;padding:8px 17px;border:1px solid var(--vscode-contrastBorder,transparent);border-radius:4px;cursor:pointer}#save{color:var(--vscode-button-foreground);background:var(--vscode-button-background)}#save:hover{background:var(--vscode-button-hoverBackground)}#cancel{color:var(--vscode-button-secondaryForeground);background:var(--vscode-button-secondaryBackground)}#cancel:hover{background:var(--vscode-button-secondaryHoverBackground)}button:disabled{opacity:.55;cursor:wait}@media(max-width:520px){body{padding:24px 20px}.fields{grid-template-columns:1fr;gap:18px}header p{margin-bottom:26px}footer{flex-wrap:wrap}footer .hint{flex-basis:100%;margin-bottom:8px}h1{font-size:23px}}@media(forced-colors:active){.toggle:checked{background:Highlight}.toggle:checked:before{background:HighlightText}}
</style></head><body><main><header><h1>${title}</h1><p>连接你的 API 服务商，继续使用原来的聊天。</p></header>
<form id="provider"><div class="fields">
<div class="field"><label for="name">服务商名称</label><input id="name" name="name" required maxlength="200" value="${escape(existing?.name ?? '')}" placeholder="例如：win99" autofocus></div>
<div class="field"><label for="model">模型 ID</label><input id="model" name="model" required maxlength="200" value="${escape(existing?.model ?? '')}" placeholder="填写服务商提供的模型 ID" spellcheck="false"></div>
<div class="field wide"><label for="baseUrl">API 地址</label><input id="baseUrl" name="baseUrl" type="url" required value="${escape(existing?.baseUrl ?? '')}" placeholder="https://api.example.com/v1" spellcheck="false" aria-describedby="urlHint"><p class="hint" id="urlHint">需支持 Responses API，地址末尾不含 /responses。</p></div>
<div class="field wide"><label for="key">API 密钥</label><div class="secret"><input id="key" name="key" type="password" ${hasKey ? '' : 'required'} maxlength="16384" autocomplete="off" placeholder="${hasKey ? '已保存密钥，留空则不修改' : '输入 API 密钥'}" spellcheck="false"><button class="reveal" type="button" id="reveal" aria-label="显示密钥" aria-pressed="false">${svg('eye')}</button></div></div>
</div><div class="fast"><div><label for="nativeFast">Fast 模式</label><p class="hint" id="fastHint">启用后可在 Codex 中选择 Standard / Fast。<br>实际速度和费用以服务商为准。</p></div><input class="toggle" type="checkbox" role="switch" id="nativeFast" name="nativeFast" ${existing?.nativeFast ? 'checked' : ''} aria-describedby="fastHint"></div>
<p id="error" class="error" role="alert"></p><footer><span class="hint">保存后，切换并重载即可生效。</span><button type="button" id="cancel">取消</button><button id="save" type="submit">保存服务商</button></footer></form></main>
<script nonce="${nonce}">
const api=acquireVsCodeApi(),form=document.getElementById('provider'),save=document.getElementById('save');
document.getElementById('cancel').onclick=()=>api.postMessage({action:'cancel'});
document.getElementById('reveal').onclick=()=>{const key=document.getElementById('key'),button=document.getElementById('reveal'),visible=key.type==='password';key.type=visible?'text':'password';button.setAttribute('aria-pressed',String(visible));button.setAttribute('aria-label',visible?'隐藏密钥':'显示密钥');};
form.onsubmit=event=>{event.preventDefault();if(save.disabled)return;save.disabled=true;save.textContent='正在保存…';document.getElementById('error').textContent='';api.postMessage({action:'save',name:form.elements.name.value,baseUrl:form.elements.baseUrl.value,model:form.elements.model.value,key:form.elements.key.value,nativeFast:form.elements.nativeFast.checked});};
window.addEventListener('message',event=>{if(event.data?.action==='error'){document.getElementById('error').textContent=event.data.message;save.disabled=false;save.textContent='保存服务商';}});
</script></body></html>`;
}

export function editProviderForm(existing: Provider | undefined, hasKey: boolean, save: (value: ProviderInput) => Promise<void>): Promise<boolean> {
  const panel = vscode.window.createWebviewPanel('easySwitch.provider', existing ? 'Easy Switch · 编辑服务商' : 'Easy Switch · 添加服务商', vscode.ViewColumn.Active, { enableScripts: true, retainContextWhenHidden: true, localResourceRoots: [] });
  panel.webview.html = providerFormHtml(existing, hasKey, randomUUID());
  return new Promise(resolve => {
    let done = false, saving = false;
    const finish = (saved: boolean) => { if (done) return; done = true; messages.dispose(); closed.dispose(); resolve(saved); panel.dispose(); };
    const messages = panel.webview.onDidReceiveMessage(async message => {
      if (saving) return;
      if (message?.action === 'cancel') { finish(false); return; }
      if (message?.action !== 'save') return;
      if (['name', 'baseUrl', 'model', 'key'].some(key => typeof message[key] !== 'string') || typeof message.nativeFast !== 'boolean') {
        await panel.webview.postMessage({ action: 'error', message: '表单内容无效，请重新填写。' }); return;
      }
      saving = true;
      try {
        await save({ name: message.name.trim(), baseUrl: message.baseUrl.trim().replace(/\/+$/, ''), model: message.model.trim(), key: message.key, nativeFast: message.nativeFast });
        finish(true);
      } catch (error) {
        const detail = error instanceof Error ? error.message : '保存失败，请重试。';
        await panel.webview.postMessage({ action: 'error', message: message.key ? detail.split(message.key).join('[已隐藏]') : detail });
      } finally { saving = false; }
    });
    const closed = panel.onDidDispose(() => finish(false));
  });
}
