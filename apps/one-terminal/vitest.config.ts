import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config";

// Unit and component tests for the Terminal UI. Runs in jsdom with Tauri IPC mocked
// (see src/test/setup.ts); nothing here needs the Rust side or a real webview.
export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: "jsdom",
      include: ["src/**/*.test.{ts,tsx}"],
      setupFiles: ["./src/test/setup.ts"],
      restoreMocks: true,
    },
  })
);
