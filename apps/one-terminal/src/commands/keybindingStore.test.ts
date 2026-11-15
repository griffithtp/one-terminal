import { beforeEach, describe, expect, it, vi } from "vitest";

const STORAGE_KEY = "one-terminal:keybindings";

// keybindingStore keeps a module-level snapshot of default keybindings, so each test
// gets fresh copies of it and of the registry it reads.
async function load() {
  vi.resetModules();
  const { registry } = await import("./registry");
  const store = await import("./keybindingStore");
  return { registry, ...store };
}

beforeEach(() => localStorage.clear());

describe("loadKeybindings / saveKeybindings", () => {
  it("round-trips overrides through localStorage", async () => {
    const { loadKeybindings, saveKeybindings } = await load();
    saveKeybindings({ "app.palette": "CmdOrCtrl+P" });
    expect(loadKeybindings()).toEqual({ "app.palette": "CmdOrCtrl+P" });
  });

  it("returns {} for missing or corrupt storage", async () => {
    const { loadKeybindings } = await load();
    expect(loadKeybindings()).toEqual({});
    localStorage.setItem(STORAGE_KEY, "{not json");
    expect(loadKeybindings()).toEqual({});
  });
});

describe("applyKeybindingOverrides", () => {
  it("replaces defaults with saved overrides and remembers the defaults", async () => {
    const { registry, applyKeybindingOverrides, getDefaultKeybinding } = await load();
    registry.register({
      id: "app.palette",
      label: "Command palette",
      keywords: [],
      group: "navigation",
      keybinding: "CmdOrCtrl+K",
      action: () => {},
    });
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ "app.palette": "CmdOrCtrl+Shift+K" }));

    applyKeybindingOverrides();

    expect(registry.findByKeybinding("CmdOrCtrl+Shift+K")?.id).toBe("app.palette");
    expect(registry.findByKeybinding("CmdOrCtrl+K")).toBeUndefined();
    expect(getDefaultKeybinding("app.palette")).toBe("CmdOrCtrl+K");
  });

  it('treats an empty-string override as "explicitly unbound"', async () => {
    const { registry, applyKeybindingOverrides } = await load();
    registry.register({
      id: "app.close",
      label: "Close",
      keywords: [],
      group: "widgets",
      keybinding: "CmdOrCtrl+W",
      action: () => {},
    });
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ "app.close": "" }));

    applyKeybindingOverrides();

    expect(registry.findByKeybinding("CmdOrCtrl+W")).toBeUndefined();
  });
});
