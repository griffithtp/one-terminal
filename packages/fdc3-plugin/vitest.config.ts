import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["test/**/*.test.ts"],
    globalSetup: "./test/harness.ts",
    testTimeout: 15_000,
    hookTimeout: 15_000,
    // One Desktop Agent is shared by all tests, so run them one at a time.
    fileParallelism: false,
    sequence: { concurrent: false },
  },
});
