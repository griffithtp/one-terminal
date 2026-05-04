import { useCallback, useEffect, useState } from "react";
import type { EngineBinding } from "../types";

/**
 * Dev-only per-launch engine override, persisted in `localStorage` so it
 * survives a page reload. The launcher passes this value to the Rust
 * `cda_launch_app` command; release builds of CDA ignore it by `cfg`.
 *
 * Stored shape: `{"family": "...", "version": "..."}` — same as the Rust
 * `ot_core::engine::EngineBinding`.
 */
const STORAGE_KEY = "ot:engine-override";

function read(): EngineBinding | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<EngineBinding>;
    if (typeof parsed.family === "string" && typeof parsed.version === "string") {
      return { family: parsed.family as EngineBinding["family"], version: parsed.version };
    }
    return null;
  } catch {
    return null;
  }
}

export function useEngineOverride(): {
  override: EngineBinding | null;
  setOverride: (binding: EngineBinding | null) => void;
} {
  const [override, setOverrideState] = useState<EngineBinding | null>(() => read());

  // Sync with changes made in another tab / devtools console.
  useEffect(() => {
    function onStorage(e: StorageEvent) {
      if (e.key === STORAGE_KEY) setOverrideState(read());
    }
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  const setOverride = useCallback((binding: EngineBinding | null) => {
    if (binding) {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(binding));
    } else {
      localStorage.removeItem(STORAGE_KEY);
    }
    setOverrideState(binding);
  }, []);

  return { override, setOverride };
}
