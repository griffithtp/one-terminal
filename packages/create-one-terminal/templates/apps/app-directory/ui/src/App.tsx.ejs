import React, { useState, useEffect, useCallback } from "react";
import type { AppD } from "./types";
import AppList from "./components/AppList";
import AppForm from "./components/AppForm";

type View = "list" | "register" | "edit";

interface Toast {
  message: string;
  isError?: boolean;
}

export default function App() {
  const [view, setView] = useState<View>("list");
  const [editTarget, setEditTarget] = useState<AppD | null>(null);
  const [apps, setApps] = useState<AppD[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);

  const showToast = (message: string, isError = false) => {
    setToast({ message, isError });
    setTimeout(() => setToast(null), 3000);
  };

  const fetchApps = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetch("/v2/apps");
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = await res.json();
      setApps(data.applications ?? []);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to load applications");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchApps();
  }, [fetchApps]);

  const handleEdit = (app: AppD) => {
    setEditTarget(app);
    setView("edit");
  };

  const handleDelete = async (appId: string) => {
    try {
      const res = await fetch(`/v2/apps/${appId}`, { method: "DELETE" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      showToast("Application removed.");
      fetchApps();
    } catch (e: unknown) {
      showToast(e instanceof Error ? e.message : "Delete failed", true);
    }
  };

  const handleSave = (message: string) => {
    showToast(message);
    fetchApps();
    setView("list");
    setEditTarget(null);
  };

  const handleCancel = () => {
    setView("list");
    setEditTarget(null);
  };

  const gotoRegister = () => {
    setEditTarget(null);
    setView("register");
  };

  const gotoList = () => {
    setEditTarget(null);
    setView("list");
  };

  return (
    <>
      <header className="app-header">
        <h1>
          OneTerminal <span>· App Directory</span>
        </h1>
        <nav className="nav-tabs">
          <button className={view === "list" ? "active" : ""} onClick={gotoList}>
            App Catalogue
          </button>
          <button className={view === "register" ? "active" : ""} onClick={gotoRegister}>
            + Register App
          </button>
        </nav>
      </header>

      <main>
        {view === "list" && (
          <AppList
            apps={apps}
            loading={loading}
            error={error}
            onEdit={handleEdit}
            onDelete={handleDelete}
          />
        )}
        {(view === "register" || view === "edit") && (
          <AppForm existingApp={editTarget} onSave={handleSave} onCancel={handleCancel} />
        )}
      </main>

      {toast && <div className={`toast${toast.isError ? " is-error" : ""}`}>{toast.message}</div>}
    </>
  );
}
