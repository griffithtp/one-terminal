import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";
import type { AppRecord } from "../types";

export function useAppDirectory() {
  const [apps, setApps] = useState<AppRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const records = await invoke<AppRecord[]>("cda_list_app_directory");
      setApps(records);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      await invoke<number>("cda_refresh_app_directory");
      const records = await invoke<AppRecord[]>("cda_list_app_directory");
      setApps(records);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  return { apps, loading, error, load, refresh };
}
