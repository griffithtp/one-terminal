import * as matchers from "@testing-library/jest-dom/matchers";
import { cleanup } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, expect } from "vitest";

// Register jest-dom matchers on this package's `expect`. (`@testing-library/jest-dom/vitest`
// would bind to whichever `vitest` it resolves from the repo root — fdc3-plugin's older
// copy — and the matchers would silently be missing here.)
expect.extend(matchers);

// Each test starts with an empty DOM, no IPC handler and empty storage.
afterEach(() => {
  cleanup();
  clearMocks();
  localStorage.clear();
});
