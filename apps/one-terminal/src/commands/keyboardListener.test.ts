import { describe, expect, it } from "vitest";
import { normaliseCombo } from "./keyboardListener";

const key = (
  k: string,
  mods: Partial<Record<"meta" | "ctrl" | "shift" | "alt", boolean>> = {}
) => ({
  key: k,
  metaKey: !!mods.meta,
  ctrlKey: !!mods.ctrl,
  shiftKey: !!mods.shift,
  altKey: !!mods.alt,
});

describe("normaliseCombo", () => {
  it("maps both ⌘ (macOS) and Ctrl (Windows) to CmdOrCtrl", () => {
    expect(normaliseCombo(key("k", { meta: true }))).toBe("CmdOrCtrl+K");
    expect(normaliseCombo(key("k", { ctrl: true }))).toBe("CmdOrCtrl+K");
    // Known quirk (docs/conventions/cross-platform.md): on macOS Ctrl+K also matches ⌘K.
    expect(normaliseCombo(key("k", { meta: true, ctrl: true }))).toBe("CmdOrCtrl+K");
  });

  it("upper-cases single printable characters", () => {
    expect(normaliseCombo(key("w", { meta: true }))).toBe("CmdOrCtrl+W");
    expect(normaliseCombo(key("0", { ctrl: true }))).toBe("CmdOrCtrl+0");
  });

  it("orders modifiers CmdOrCtrl, Shift, Alt", () => {
    expect(normaliseCombo(key("p", { alt: true, shift: true, ctrl: true }))).toBe(
      "CmdOrCtrl+Shift+Alt+P"
    );
  });

  it("names = and - as Equal and Minus (zoom shortcuts)", () => {
    expect(normaliseCombo(key("=", { meta: true }))).toBe("CmdOrCtrl+Equal");
    expect(normaliseCombo(key("-", { ctrl: true }))).toBe("CmdOrCtrl+Minus");
  });

  it("leaves named keys unchanged", () => {
    expect(normaliseCombo(key("Tab", { ctrl: true }))).toBe("CmdOrCtrl+Tab");
    expect(normaliseCombo(key("Escape"))).toBe("Escape");
    expect(normaliseCombo(key("ArrowDown", { shift: true }))).toBe("Shift+ArrowDown");
  });
});
