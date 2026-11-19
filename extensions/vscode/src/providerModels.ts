import * as http from 'node:http';
import * as https from 'node:https';
import { validModelId } from './storage';

export async function fetchProviderModels(baseUrl: URL, key: string): Promise<string[]> {
  const endpoint = new URL(baseUrl);
  endpoint.pathname = endpoint.pathname.replace(/\/+$/, '') + '/models';
  const signal = AbortSignal.timeout(15_000);
  const get = endpoint.protocol === 'https:' ? https.get : http.get;
  let body: Buffer;
  let status: number | undefined;
  try {
    const response = await new Promise<http.IncomingMessage>((resolve, reject) => {
      get(endpoint, { headers: { Authorization: `Bearer ${key}`, Accept: 'application/json' }, signal }, resolve).once('error', reject);
    });
    status = response.statusCode;
    if (status !== 200) response.destroy();
    const chunks: Buffer[] = [];
    let size = 0;
    if (status === 200) {
      for await (const chunk of response) {
        size += chunk.length;
        if (size > 4 * 1024 * 1024) throw new RangeError();
        chunks.push(chunk);
      }
    }
    body = Buffer.concat(chunks);
  } catch (error) {
    if (signal.aborted) throw new Error('获取模型列表超时，请重试。');
    if (error instanceof RangeError) throw new Error('模型列表过大，请手动填写模型 ID。');
    throw new Error('无法获取模型列表，请检查服务器网络和 API 地址。');
  }
  if (status !== 200) throw new Error(`获取模型列表失败（HTTP ${status}）。请检查 API 地址、密钥和服务商权限。`);
  let data: unknown;
  try { data = JSON.parse(body.toString('utf8'))?.data; }
  catch { throw new Error('服务商返回的模型列表不是有效的 JSON。'); }
  if (!Array.isArray(data)) throw new Error('服务商返回的模型列表缺少 data 数组。');
  const ids = new Set<string>();
  for (const model of data) {
    const id = model?.id;
    if (validModelId(id)) ids.add(id);
  }
  if (!ids.size) throw new Error('服务商未返回可选模型，你仍可手动填写模型 ID。');
  if (ids.size > 10_000) throw new Error('服务商返回的模型超过一万个，请手动填写模型 ID。');
  return [...ids].sort();
}
