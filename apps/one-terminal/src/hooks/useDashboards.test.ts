import { renderHook, waitFor } from "@testing-library/react";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { beforeEach, describe, expect, it } from "vitest";
import { useDashboards } from "./useDashboards";

// Every `invoke` the hook makes, as [command, args].
let calls: Array<[string, unknown]>;

beforeEach(() => {
  calls = [];
  mockWindows("terminal-main");
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === "wm_list_dashboards") {
      return {
        active: "Trading",
        autoSave: false,
        dirty: true,
        dashboards: ["Trading"],
        closedDashboards: [],
        parkedCount: 0,
        lockedBy: {},
      };
    }
  });
});

const titleCalls = () => calls.filter(([cmd]) => cmd === "plugin:window|set_title");

describe("useDashboards window title", () => {
  it("sets the title from the active dashboard when asked to (the chrome)", async () => {
    renderHook(() => useDashboards({ syncWindowTitle: true }));
    await waitFor(() => expect(titleCalls()).toHaveLength(1));
    expect(titleCalls()[0][1]).toMatchObject({ value: "OneTerminal — Trading *" });
  });

  it("leaves the title alone by default (the overlay shares the window)", async () => {
    renderHook(() => useDashboards());
    await waitFor(() => expect(calls.some(([cmd]) => cmd === "wm_list_dashboards")).toBe(true));
    // Let the payload effect run.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(titleCalls()).toHaveLength(0);
  });
});
