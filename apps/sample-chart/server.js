/**
 * Sample-Chart dev server — port 3011
 *
 * Serves:
 *   /            → index.html
 *   /fdc3-plugin.js → ../../packages/fdc3-plugin/fdc3-plugin.js  (from monorepo)
 */
import fs   from 'fs';
import http from 'http';
import path from 'path';
import { fileURLToPath } from 'url';

const PORT   = 3011;
const __dir  = path.dirname(fileURLToPath(import.meta.url));
const PLUGIN = path.resolve(__dir, '../../packages/fdc3-plugin/fdc3-plugin.js');

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js':   'application/javascript; charset=utf-8',
  '.css':  'text/css; charset=utf-8',
};

http.createServer((req, res) => {
  res.setHeader('Access-Control-Allow-Origin', '*');

  let filePath;
  if (req.url === '/fdc3-plugin.js') {
    filePath = PLUGIN;
  } else {
    // Strip the query string, decode, then confine the resolved path to __dir
    // so crafted requests (e.g. `/../../etc/passwd`) cannot escape the web root.
    let reqPath;
    try {
      reqPath = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
    } catch {
      reqPath = '';
    }
    const resolved = path.join(__dir, reqPath === '/' || reqPath === '' ? 'index.html' : reqPath);
    if (resolved !== __dir && !resolved.startsWith(__dir + path.sep)) {
      res.writeHead(403, { 'Content-Type': 'text/plain' });
      res.end('403 Forbidden');
      return;
    }
    filePath = resolved;
  }

  fs.readFile(filePath, (err, data) => {
    if (err) {
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('404 Not Found');
      return;
    }
    const ext  = path.extname(filePath);
    const mime = MIME[ext] ?? 'application/octet-stream';
    const cache = ext === '.html'
      ? 'no-cache, no-store, must-revalidate'
      : 'public, max-age=31536000, immutable';
    res.writeHead(200, { 'Content-Type': mime, 'Cache-Control': cache });
    res.end(data);
  });
}).listen(PORT, () => {
  console.log(`[sample-chart] http://localhost:${PORT}`);
});
