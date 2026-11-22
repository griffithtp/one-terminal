/**
 * <%= widgetTitle %> dev server — port <%= widgetPort %>.
 *
 * Serves index.html and proxies /fdc3-plugin.js and /ot-telemetry.js from the
 * monorepo so the widget can talk to the configured FDC3 agent and send its
 * errors to OneTerminal's logs.
 */
import fs from "fs";
import http from "http";
import path from "path";
import { fileURLToPath } from "url";

const PORT = <%= widgetPort %>;
const __dir = path.dirname(fileURLToPath(import.meta.url));
const PLUGIN = path.resolve(__dir, "../../packages/fdc3-plugin/fdc3-plugin.js");
const TELEMETRY = path.resolve(__dir, "../../packages/telemetry-client/ot-telemetry.js");

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "application/javascript; charset=utf-8",
};

http
  .createServer((req, res) => {
    res.setHeader("Access-Control-Allow-Origin", "*");

    // Strip the query string and decode, then confine the resolved path to
    // __dir so crafted requests (e.g. `/../../etc/passwd`) cannot escape it.
    let reqPath;
    try {
      reqPath = decodeURIComponent(new URL(req.url, "http://localhost").pathname);
    } catch {
      reqPath = "";
    }
    let filePath;
    if (reqPath === "/fdc3-plugin.js") {
      filePath = PLUGIN;
    } else if (reqPath === "/ot-telemetry.js") {
      filePath = TELEMETRY;
    } else {
      filePath = path.join(__dir, reqPath === "/" || reqPath === "" ? "index.html" : reqPath);
      if (filePath !== __dir && !filePath.startsWith(__dir + path.sep)) {
        res.writeHead(403, { "Content-Type": "text/plain" });
        res.end("403 Forbidden");
        return;
      }
    }
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
    console.log(`[<%= widgetName %>] http://localhost:${PORT}`);
  });
