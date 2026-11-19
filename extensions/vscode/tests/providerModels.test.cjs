const test = require('node:test');
const assert = require('node:assert/strict');
const http = require('node:http');
const { fetchProviderModels } = require('../out/providerModels');

test('fetches models from the configured prefix and reports failures without exposing response bodies', async t => {
  const requests = [];
  const server = http.createServer((req, res) => {
    requests.push([req.method, req.url, req.headers.authorization]);
    switch (req.url) {
      case '/proxy/v1/models':
        res.end(JSON.stringify({ data: [{ id: 'z' }, { id: 'a' }, { id: 'z' }, null, { id: 'bad\n' }, { id: '界'.repeat(70) }] }));
        break;
      case '/redirect/models':
        res.writeHead(302, { Location: '/proxy/v1/models' }).end();
        break;
      case '/empty/models': res.end('{"data":[]}'); break;
      case '/broken/models': res.end('<html>private data</html>'); break;
      case '/missing/models': res.end('{}'); break;
      case '/large/models': res.end(Buffer.alloc(4 * 1024 * 1024 + 1)); break;
      default: res.writeHead(401).end('private data');
    }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => server.close(resolve)));
  const fetch = path => fetchProviderModels(new URL(`http://127.0.0.1:${server.address().port}/${path}`), 'synthetic');
  assert.deepEqual(await fetch('proxy/v1///'), ['a', 'z']);
  assert.deepEqual(requests[0], ['GET', '/proxy/v1/models', 'Bearer synthetic']);
  for (const [path, expected] of [['redirect', /302/], ['empty', /未返回可选/], ['broken', /JSON/], ['missing', /data 数组/], ['large', /过大/], ['denied', /401/]]) {
    await assert.rejects(fetch(path), error => expected.test(error.message) && !/synthetic|private data/.test(error.message));
  }
  assert.equal(requests.filter(([, url]) => url === '/proxy/v1/models').length, 1);
});
