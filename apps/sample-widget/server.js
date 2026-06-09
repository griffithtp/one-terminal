/**
 * Sample widget dev server — port 3012.
 *
 * The widget is intentionally minimal: a single HTML page that demonstrates
 * receiving and broadcasting FDC3 context. Used as the only built-in sample
 * in Standalone-variant scaffolds.
 *
 * Routes:
 *   /                → index.html
 *   /fdc3-plugin.js  → ../../packages/fdc3-plugin/fdc3-plugin.js
 */
import fs from "fs";
import http from "http";
import path from "path";
import { fileURLToPath } from "url";

const PORT = 3012;
const __dir = path.dirname(fileURLToPath(import.meta.url));
const PLUGIN = path.resolve(__dir, "../../packages/fdc3-plugin/fdc3-plugin.js");

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "application/javascript; charset=utf-8",
};

http
  .createServer((req, res) => {
    res.setHeader("Access-Control-Allow-Origin", "*");

    const filePath =
      req.url === "/fdc3-plugin.js"
        ? PLUGIN
        : path.join(__dir, req.url === "/" ? "/index.html" : req.url);

    fs.readFile(filePath, (err, data) => {
      if (err) {
        res.writeHead(404, { "Content-Type": "text/plain" });
        res.end("404 Not Found");
        return;
      }
      const mime = MIME[path.extname(filePath)] ?? "application/octet-stream";
      res.writeHead(200, { "Content-Type": mime, "Cache-Control": "no-cache" });
      res.end(data);
    });
  })
  .listen(PORT, () => {
    console.log(`[sample-widget] http://localhost:${PORT}`);
  });
