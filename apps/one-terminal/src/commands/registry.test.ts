import { afterEach, describe, expect, it, vi } from "vitest";
import { bigramScore, registry, type Command } from "./registry";

// `registry` is a module singleton shared with the app; every test registers its own
// ids and removes them afterwards.
const registered: string[] = [];

function add(overrides: Partial<Command> & Pick<Command, "id">): Command {
  const cmd: Command = {
    label: overrides.id,
    keywords: [],
    group: "widgets",
    action: () => {},
    ...overrides,
  };
  registry.register(cmd);
  registered.push(cmd.id);
  return cmd;
}

afterEach(() => {
  for (const id of registered.splice(0)) registry.unregister(id);
});

describe("bigramScore", () => {
  it("counts shared two-character sequences", () => {
    expect(bigramScore("zoom", "zoom in")).toBe(3); // zo, oo, om
    expect(bigramScore("xyz", "zoom in")).toBe(0);
  });
});

describe("registry keybindings", () => {
  it("finds a command by its keybinding", () => {
    add({ id: "t.palette", keybinding: "CmdOrCtrl+Shift+P" });
    expect(registry.findByKeybinding("CmdOrCtrl+Shift+P")?.id).toBe("t.palette");
  });

  it("re-registering moves the keybinding instead of keeping both", () => {
    add({ id: "t.move", keybinding: "CmdOrCtrl+Shift+M" });
    add({ id: "t.move", keybinding: "CmdOrCtrl+Shift+N" });
    expect(registry.findByKeybinding("CmdOrCtrl+Shift+M")).toBeUndefined();
    expect(registry.findByKeybinding("CmdOrCtrl+Shift+N")?.id).toBe("t.move");
  });

  it("unregistering removes the command and its keybinding", () => {
    add({ id: "t.gone", keybinding: "CmdOrCtrl+Shift+G" });
    registry.unregister("t.gone");
    expect(registry.findByKeybinding("CmdOrCtrl+Shift+G")).toBeUndefined();
    expect(registry.getAll().some((c) => c.id === "t.gone")).toBe(false);
  });
});

describe("registry availability", () => {
  it("hides unavailable commands from search and the palette, and won't run them", async () => {
    const action = vi.fn();
    add({ id: "t.hidden", label: "Hidden thing", isAvailable: () => false, action });

    expect(registry.search("hidden").map((c) => c.id)).not.toContain("t.hidden");
    expect(registry.serializeForPalette().map((c) => c.id)).not.toContain("t.hidden");
    await registry.execute("t.hidden");
    expect(action).not.toHaveBeenCalled();
  });

  it("runs available commands", async () => {
    const action = vi.fn();
    add({ id: "t.run", action });
    await registry.execute("t.run");
    expect(action).toHaveBeenCalledOnce();
  });
});

describe("registry search", () => {
  it("ranks by fuzzy match on label and keywords", () => {
    add({ id: "t.zoom", label: "Zoom in", keywords: ["magnify"] });
    add({ id: "t.close", label: "Close tab", keywords: [] });

    expect(registry.search("zoom")[0]?.id).toBe("t.zoom");
    expect(registry.search("magnify")[0]?.id).toBe("t.zoom");
  });

  it("returns at most 10 results", () => {
    for (let i = 0; i < 12; i++) add({ id: `t.many.${i}`, label: `Many item ${i}` });
    expect(registry.search("many").length).toBe(10);
  });

  it("orders an empty query by group, navigation first", () => {
    add({ id: "t.instance", group: "widget-instances" });
    add({ id: "t.nav", group: "navigation" });
    const ids = registry.search("").map((c) => c.id);
    expect(ids.indexOf("t.nav")).toBeLessThan(ids.indexOf("t.instance"));
  });
});
