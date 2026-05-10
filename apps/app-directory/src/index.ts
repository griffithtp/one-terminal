import express from "express";
import cors from "cors";
import { fileURLToPath } from "url";
import { dirname, join } from "path";
import router from "./router.js";

// ── Configuration ──────────────────────────────────────────────────────────────

const PORT = Number(process.env.PORT ?? 3005);

// Origins that may query this directory.
// In production, replace "*" with an explicit allowlist.
const CORS_ORIGIN = process.env.CORS_ORIGIN ?? "*";

const __dirname = dirname(fileURLToPath(import.meta.url));
// When running from dist/index.js, go up one level to the package root, then ui/dist
const UI_DIST = join(__dirname, "..", "ui", "dist");

// ── App ────────────────────────────────────────────────────────────────────────

const app = express();

app.use(express.json());

app.use(
  cors({
    origin: CORS_ORIGIN,
    methods: ["GET", "POST", "PUT", "DELETE", "OPTIONS"],
    allowedHeaders: ["Content-Type", "Accept"],
  })
);

// ── Health endpoint ────────────────────────────────────────────────────────────

app.get("/health", (_req, res) => {
  res.json({ status: "ok", version: "2.2" });
});

// ── FDC3 2.2 App Directory API ─────────────────────────────────────────────────

app.use("/v2", router);

// ── React SPA ──────────────────────────────────────────────────────────────────
// Serve the built Vite frontend. Any non-API path falls back to index.html so
// the React router can handle client-side navigation.

app.use(
  express.static(UI_DIST, {
    maxAge: "1y",
    immutable: true,
    setHeaders(res, filePath) {
      if (filePath.endsWith(".html")) {
        res.setHeader("Cache-Control", "no-cache, no-store, must-revalidate");
      }
    },
  })
);

app.get(/^(?!\/v2|\/health).*/, (_req, res) => {
  res.setHeader("Cache-Control", "no-cache, no-store, must-revalidate");
  res.sendFile(join(UI_DIST, "index.html"));
});

// ── Start ──────────────────────────────────────────────────────────────────────

app.listen(PORT, () => {
  console.log(`FDC3 2.2 App Directory listening on http://localhost:${PORT}`);
  console.log(`  GET    http://localhost:${PORT}/v2/apps`);
  console.log(`  GET    http://localhost:${PORT}/v2/apps/:appId`);
  console.log(`  POST   http://localhost:${PORT}/v2/apps`);
  console.log(`  PUT    http://localhost:${PORT}/v2/apps/:appId`);
  console.log(`  DELETE http://localhost:${PORT}/v2/apps/:appId`);
  console.log(`  UI     http://localhost:${PORT}/`);
});
