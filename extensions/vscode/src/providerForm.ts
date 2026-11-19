import * as vscode from 'vscode';
import { randomUUID } from 'node:crypto';
import type { ModelCatalog, Provider } from './storage';
import { escapeHtml as escape, svg } from './webview';

export interface ProviderInput { name: string; baseUrl: string; model: string; key: string; nativeFast: boolean; catalog?: ModelCatalog }

export function providerFormHtml(existing: Provider | undefined, hasKey: boolean, nonce: string): string {
  const title = existing ? '编辑服务商' : '添加服务商';
  return `<!doctype html><html lang="zh-CN"><head><meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}'">
<meta name="viewport" content="width=device-width, initial-scale=1">
<style nonce="${nonce}">
*{box-sizing:border-box}body{color:var(--vscode-foreground);background:var(--vscode-editor-background);font-family:var(--vscode-font-family);font-size:14px;line-height:1.6;margin:0;padding:40px 28px}main{max-width:640px;margin:auto}h1{font-size:26px;font-weight:600;line-height:1.3;margin:0 0 10px}header p{color:var(--vscode-descriptionForeground);margin:0 0 32px}form{margin:0}.fields{display:grid;grid-template-columns:1fr 1fr;gap:22px 24px}.field{min-width:0}.wide{grid-column:1/-1}label{display:block;margin-bottom:7px;font-weight:500}.hint{display:block;color:var(--vscode-descriptionForeground);font-size:12px;margin:6px 0 0}input:not([type=checkbox]){width:100%;height:40px;padding:8px 11px;border:1px solid var(--vscode-input-border,var(--vscode-widget-border));border-radius:4px;background:var(--vscode-input-background);color:var(--vscode-input-foreground);font:inherit}input::placeholder{color:var(--vscode-input-placeholderForeground)}input:focus-visible,button:focus-visible{outline:2px solid var(--vscode-focusBorder);outline-offset:2px}input{caret-color:var(--vscode-input-foreground)}::selection{background:var(--vscode-editor-selectionBackground)}.secret{display:flex;position:relative}.secret input{padding-right:48px}.reveal{position:absolute;right:3px;top:3px;width:36px;height:34px;padding:8px;border:0;background:transparent;color:var(--vscode-descriptionForeground);cursor:pointer;border-radius:3px}svg{display:block;width:20px;height:20px}.reveal:hover{color:var(--vscode-foreground);background:var(--vscode-toolbar-hoverBackground)}.fast{display:flex;justify-content:space-between;align-items:center;gap:24px;margin-top:28px;padding:22px 0;border-top:1px solid var(--vscode-widget-border);border-bottom:1px solid var(--vscode-widget-border)}.fast label{font-size:15px;margin:0}.fast p{margin:4px 0 0;max-width:490px}.toggle{appearance:none;flex:0 0 36px;width:36px;height:21px;margin:0;border:1px solid var(--vscode-input-border,var(--vscode-descriptionForeground));border-radius:11px;background:var(--vscode-input-background);cursor:pointer;position:relative}.toggle:before{content:'';position:absolute;width:13px;height:13px;left:3px;top:3px;border-radius:50%;background:var(--vscode-descriptionForeground)}.toggle:checked{background:var(--vscode-button-background);border-color:var(--vscode-button-background)}.toggle:checked:before{left:18px;background:var(--vscode-button-foreground)}.error{color:var(--vscode-errorForeground);margin:18px 0 0;white-space:pre-wrap;overflow-wrap:anywhere}.error:empty{display:none}footer{display:flex;align-items:center;gap:10px;margin-top:28px}footer .hint{margin:0 auto 0 0}footer button{font:inherit;padding:8px 17px;border:1px solid var(--vscode-contrastBorder,transparent);border-radius:4px;cursor:pointer}#save{color:var(--vscode-button-foreground);background:var(--vscode-button-background)}#save:hover{background:var(--vscode-button-hoverBackground)}#cancel{color:var(--vscode-button-secondaryForeground);background:var(--vscode-button-secondaryBackground)}#cancel:hover{background:var(--vscode-button-secondaryHoverBackground)}button:disabled{opacity:.55;cursor:wait}@media(max-width:520px){body{padding:24px 20px}.fields{grid-template-columns:1fr;gap:18px}header p{margin-bottom:26px}footer{flex-wrap:wrap}footer .hint{flex-basis:100%;margin-bottom:8px}h1{font-size:23px}}@media(forced-colors:active){.toggle:checked{background:Highlight}.toggle:checked:before{background:HighlightText}}
.models{display:flex;gap:8px;align-items:center}.models button,.models select{font:inherit;border-radius:4px;border:1px solid var(--vscode-input-border,var(--vscode-widget-border));padding:8px}.models button{white-space:nowrap;cursor:pointer;background:var(--vscode-button-secondaryBackground);color:var(--vscode-button-secondaryForeground)}.models select{width:100%;min-width:0;background:var(--vscode-dropdown-background);color:var(--vscode-dropdown-foreground)}#modelStatus:empty{display:none}
.picker-heading{display:flex;align-items:center;gap:10px;margin:20px 0 8px}.picker-heading strong{flex:1}.choices{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));max-height:245px;overflow:auto;margin-top:8px;border:1px solid var(--vscode-widget-border);border-radius:4px}.choices label{display:flex;align-items:center;gap:7px;padding:8px;margin:0;overflow-wrap:anywhere;border-bottom:1px solid var(--vscode-widget-border);font-weight:400}.choices input{flex:none}.choices small{color:var(--vscode-descriptionForeground)}.choices p{padding:0 10px}.models select:focus-visible{outline:2px solid var(--vscode-focusBorder)}@media(max-width:520px){.choices{grid-template-columns:1fr}}
</style></head><body><main><header><h1>${title}</h1><p>连接你的 API 服务商，继续使用原来的聊天。</p></header>
<form id="provider"><div class="fields">
<div class="field wide"><label for="name">服务商名称</label><input id="name" name="name" required maxlength="200" value="${escape(existing?.name ?? '')}" placeholder="例如：win99" autofocus></div>
<div class="field wide"><label for="baseUrl">API 地址</label><input id="baseUrl" name="baseUrl" type="url" required value="${escape(existing?.baseUrl ?? '')}" placeholder="https://api.example.com/v1" spellcheck="false" aria-describedby="urlHint"><p class="hint" id="urlHint">需支持 Responses API，地址末尾不含 /responses。</p></div>
<div class="field wide"><label for="key">API 密钥</label><div class="secret"><input id="key" name="key" type="password" ${hasKey ? '' : 'required'} maxlength="16384" autocomplete="off" placeholder="${hasKey ? '已保存密钥，留空则不修改' : '输入 API 密钥'}" spellcheck="false"><button class="reveal" type="button" id="reveal" aria-label="显示密钥" aria-pressed="false">${svg('eye')}</button></div></div>
<div class="field wide"><label id="modelLabel" for="model">默认模型</label><div class="models"><select id="modelList" aria-label="默认模型" hidden></select><input id="model" name="model" required maxlength="200" value="${escape(existing?.model ?? '')}" placeholder="输入默认模型 ID" spellcheck="false"><button type="button" id="fetchModels">获取模型列表</button></div>
<p id="modelStatus" class="hint" role="status"></p><p id="updated" class="hint"></p>
<section id="picker" hidden><div class="picker-heading models"><strong>在 Codex 中显示的模型</strong><span id="count"></span><button id="toggleAll" type="button">全选</button></div>
<input id="search" type="search" aria-label="搜索模型 ID" placeholder="搜索模型 ID"><div class="choices" id="choices"></div>
<p class="hint">切换并重载后，Codex 只显示勾选的模型。默认模型需先更换才能取消勾选。</p></section></div>
</div><div class="fast"><div><label for="nativeFast">Fast 模式</label><p class="hint" id="fastHint">启用后可在 Codex 中选择 Standard / Fast。<br>实际速度和费用以服务商为准。</p></div><input class="toggle" type="checkbox" role="switch" id="nativeFast" name="nativeFast" ${existing?.nativeFast ? 'checked' : ''} aria-describedby="fastHint"></div>
<p id="error" class="error" role="alert"></p><footer><span class="hint">保存后，切换并重载即可生效。</span><button type="button" id="cancel">取消</button><button id="save" type="submit">保存服务商</button></footer></form></main>
<script nonce="${nonce}">
const api=acquireVsCodeApi(),form=document.getElementById('provider'),save=document.getElementById('save');
const fetchButton=document.getElementById('fetchModels'),list=document.getElementById('modelList'),statusText=document.getElementById('modelStatus'),model=form.elements.model;
let requestId=0;
const byId=id=>document.getElementById(id);
let catalog=${JSON.stringify(existing?.catalog ?? null).replace(/</g, '\\u003c')};
let excluded=new Set(catalog?.excluded), names=[];
const choices=byId('choices'), toggleAll=byId('toggleAll'), search=byId('search');
function updateCount(){
  const current=model.value.trim();
  const n=names.reduce((sum,id)=>sum+Number(id===current||!excluded.has(id)),0);
  byId('count').textContent=n+' / '+names.length;
  toggleAll.textContent=n===names.length?'取消全选':'全选';
  toggleAll.disabled=names.length===1&&names[0]===model.value.trim();
}
function drawChoices(){
  const fragment=document.createDocumentFragment(),query=search.value.trim().toLowerCase();
  for(const id of names){
    if(!id.toLowerCase().includes(query))continue;
    const row=document.createElement('label'),box=document.createElement('input');
    box.type='checkbox';box.value=id;box.disabled=id===model.value.trim();box.checked=box.disabled||!excluded.has(id);
    row.append(box,document.createTextNode(id));
    if(box.disabled){const tag=document.createElement('small');tag.textContent='默认';row.append(tag);}
    fragment.append(row);
  }
  if(!fragment.childElementCount){const p=document.createElement('p');p.textContent='没有匹配的模型';fragment.append(p);}
  choices.replaceChildren(fragment);
  updateCount();
}
function drawCatalog(manual=false){
  const current=model.value.trim();
  byId('picker').hidden=!catalog;
  list.hidden=!catalog;
  model.hidden=!!catalog&&!!current&&!manual;
  byId('modelLabel').htmlFor=model.hidden?'modelList':'model';
  byId('updated').textContent=catalog?'上次更新 '+new Date(catalog.updatedAt).toLocaleString():'';
  if(!catalog)return;
  names=Array.from(new Set([...catalog.available,...(current?[current]:[])]));
  if(!manual)excluded.delete(current);
  list.replaceChildren(new Option('手动输入模型 ID',''));
  for(const id of names)list.add(new Option(id,id));
  list.value=manual?'':current;
  drawChoices();
}
search.oninput=drawChoices;
choices.onchange=({target})=>{
  if(target.checked)excluded.delete(target.value);else excluded.add(target.value);
  updateCount();
};
toggleAll.onclick=()=>{
  const select=names.some(id=>id!==model.value.trim()&&excluded.has(id));
  for(const id of names){if(select)excluded.delete(id);else if(id!==model.value.trim())excluded.add(id);}
  drawChoices();
};
list.onchange=()=>{
  if(list.value){model.value=list.value;drawCatalog();}
  else{model.hidden=false;byId('modelLabel').htmlFor='model';model.focus();model.select();}
};
model.oninput=()=>drawCatalog(true);
drawCatalog();
function invalidateRequest(){
  requestId++;
  statusText.textContent='';
  fetchButton.disabled=false;
}
form.elements.baseUrl.oninput=()=>{invalidateRequest();catalog=null;excluded.clear();search.value='';drawCatalog();};
form.elements.key.oninput=invalidateRequest;
fetchButton.onclick=()=>{
  invalidateRequest();
  fetchButton.disabled=true;
  statusText.textContent='正在获取模型列表…';
  api.postMessage({action:'fetchModels',requestId,baseUrl:form.elements.baseUrl.value,key:form.elements.key.value});
};
window.addEventListener('message',({data})=>{
  if(data?.action!=='modelResult'||data.requestId!==requestId)return;
  fetchButton.disabled=false;
  if(data.message){statusText.textContent=data.message;return;}
  catalog={available:data.models,updatedAt:Date.now()};
  statusText.textContent='';
  drawCatalog();
});
document.getElementById('cancel').onclick=()=>api.postMessage({action:'cancel'});
document.getElementById('reveal').onclick=()=>{const key=document.getElementById('key'),button=document.getElementById('reveal'),visible=key.type==='password';key.type=visible?'text':'password';button.setAttribute('aria-pressed',String(visible));button.setAttribute('aria-label',visible?'隐藏密钥':'显示密钥');};
form.onsubmit=event=>{event.preventDefault();if(save.disabled)return;save.disabled=true;save.textContent='正在保存…';document.getElementById('error').textContent='';excluded.delete(model.value.trim());api.postMessage({action:'save',name:form.elements.name.value,baseUrl:form.elements.baseUrl.value,model:model.value,key:form.elements.key.value,nativeFast:form.elements.nativeFast.checked,catalog:catalog?{...catalog,excluded:[...excluded]}:undefined});};
window.addEventListener('message',event=>{if(event.data?.action==='error'){document.getElementById('error').textContent=event.data.message;save.disabled=false;save.textContent='保存服务商';}});
</script></body></html>`;
}

export function editProviderForm(existing: Provider | undefined, hasKey: boolean, save: (value: ProviderInput) => Promise<void>, fetchModels: (baseUrl: string, key: string) => Promise<string[]>): Promise<boolean> {
  const panel = vscode.window.createWebviewPanel('easySwitch.provider', existing ? 'Easy Switch · 编辑服务商' : 'Easy Switch · 添加服务商', vscode.ViewColumn.Active, { enableScripts: true, retainContextWhenHidden: true, localResourceRoots: [] });
  panel.webview.html = providerFormHtml(existing, hasKey, randomUUID());
  return new Promise(resolve => {
    let done = false, saving = false;
    const finish = (saved: boolean) => { if (done) return; done = true; messages.dispose(); closed.dispose(); resolve(saved); panel.dispose(); };
    const messages = panel.webview.onDidReceiveMessage(async message => {
      if (saving) return;
      if (message?.action === 'cancel') { finish(false); return; }
      if (message?.action === 'fetchModels') {
        const { baseUrl, key, requestId } = message;
        if (typeof baseUrl !== 'string' || typeof key !== 'string' || !Number.isSafeInteger(requestId)) return;
        let result;
        try { result = { models: await fetchModels(baseUrl.trim(), key) }; }
        catch (error) { result = { message: error instanceof Error ? error.message : '获取模型列表失败，请重试。' }; }
        if (!done) await panel.webview.postMessage({ action: 'modelResult', requestId, ...result });
        return;
      }
      if (message?.action !== 'save') return;
      if (['name', 'baseUrl', 'model', 'key'].some(key => typeof message[key] !== 'string') || typeof message.nativeFast !== 'boolean') {
        await panel.webview.postMessage({ action: 'error', message: '表单内容无效，请重新填写。' }); return;
      }
      saving = true;
      try {
        await save({ name: message.name.trim(), baseUrl: message.baseUrl.trim().replace(/\/+$/, ''), model: message.model.trim(), key: message.key, nativeFast: message.nativeFast, ...(message.catalog === undefined ? {} : {catalog: message.catalog}) });
        finish(true);
      } catch (error) {
        const detail = error instanceof Error ? error.message : '保存失败，请重试。';
        await panel.webview.postMessage({ action: 'error', message: message.key ? detail.split(message.key).join('[已隐藏]') : detail });
      } finally { saving = false; }
    });
    const closed = panel.onDidDispose(() => finish(false));
  });
}
