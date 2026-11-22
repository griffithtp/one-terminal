import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import type { Telemetry } from "./index";
import { TelemetryErrorBoundary } from "./react";

// Tell React this is a test environment that wraps updates in act().
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Broken(): never {
  throw new Error("render failed");
}

describe("TelemetryErrorBoundary", () => {
  it("reports render errors and shows the fallback", () => {
    const telemetry = { error: vi.fn(), log: vi.fn(), flush: vi.fn(), dispose: vi.fn() };
    const container = document.createElement("div");
    // React logs caught render errors; keep the test output clean.
    vi.spyOn(console, "error").mockImplementation(() => {});

    act(() => {
      createRoot(container).render(
        <TelemetryErrorBoundary
          telemetry={telemetry as unknown as Telemetry}
          fallback={<p>oops</p>}
        >
          <Broken />
        </TelemetryErrorBoundary>
      );
    });

    expect(container.textContent).toBe("oops");
    expect(telemetry.error).toHaveBeenCalledWith(
      expect.objectContaining({ message: "render failed" }),
      expect.objectContaining({ attributes: expect.any(Object) })
    );
  });
});
