import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { registerCtxMenuItems } from "./contextMenuItems";
import { WidgetMenuItems, type WidgetMenuTarget } from "./WidgetMenuItems";

// Every `invoke` the menu makes, as [command, args].
let calls: Array<[string, unknown]>;

beforeEach(() => {
  calls = [];
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
  });
});

const tiled: WidgetMenuTarget = {
  label: "panel-1",
  appId: "ticker",
  title: "Ticker",
  stackPath: [0],
  nTabs: 3,
  floating: false,
};

function renderMenu(target: WidgetMenuTarget, submenu: "zoom" | "channel" | null = null) {
  const dismiss = vi.fn();
  render(
    <WidgetMenuItems target={target} dismiss={dismiss} submenu={submenu} onSubmenu={() => {}} />
  );
  return { dismiss };
}

describe("WidgetMenuItems", () => {
  it("offers Pop out and Close group for a tiled tab", () => {
    renderMenu(tiled);
    expect(screen.getByRole("menuitem", { name: "Pop out" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Close group (3 tabs)" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Dock back" })).not.toBeInTheDocument();
  });

  it("offers Dock back and Unglue, and no group actions, for a glued popout", () => {
    renderMenu({ ...tiled, stackPath: [], nTabs: 1, floating: true, group: "g1" });
    expect(screen.getByRole("menuitem", { name: "Dock back" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Unglue window" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /Close group/ })).not.toBeInTheDocument();
  });

  it("closes the tab through IPC with its webview label, then dismisses", async () => {
    const { dismiss } = renderMenu(tiled);
    await userEvent.click(screen.getByRole("menuitem", { name: "Close tab" }));
    expect(calls).toContainEqual(["close_tab", { label: "panel-1" }]);
    expect(dismiss).toHaveBeenCalledOnce();
  });

  it("marks the current zoom level and sets a new one", async () => {
    renderMenu({ ...tiled, zoomFactor: 1.25 }, "zoom");
    expect(screen.getByRole("menuitemradio", { name: /125%/ })).toHaveAttribute(
      "aria-checked",
      "true"
    );
    await userEvent.click(screen.getByRole("menuitemradio", { name: /150%/ }));
    expect(calls).toContainEqual(["wm_set_zoom", { label: "panel-1", zoomFactor: 1.5 }]);
  });

  it("renders registered custom items and passes them the tab context", async () => {
    const onClick = vi.fn();
    registerCtxMenuItems("ticker", [{ label: "Reload ticker", onClick }]);
    renderMenu(tiled);

    await userEvent.click(screen.getByRole("menuitem", { name: "Reload ticker" }));

    expect(onClick).toHaveBeenCalledWith(
      expect.objectContaining({ appId: "ticker", label: "panel-1", nTabs: 3, floating: false }),
      expect.any(Function)
    );
    registerCtxMenuItems("ticker", []);
  });
});
