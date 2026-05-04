import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "url";
import { dirname, resolve } from "path";

const __dirname = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  root: resolve(__dirname, "ui"),
  plugins: [react()],
  build: {
    outDir: resolve(__dirname, "ui/dist"),
    emptyOutDir: true,
  },
  server: {
    port: 5174,
    proxy: {
      "/v2": "http://localhost:3005",
      "/health": "http://localhost:3005",
    },
  },
});
