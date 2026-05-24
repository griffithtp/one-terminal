/**
 * DashboardsSection
 *
 * Full dashboard CRUD inside the App Menu drawer. Reads/writes through the
 * shared `UseDashboardsResult` so the in-header pill switcher (and command
 * palette entries) stay in sync.
 *
 * UX choices:
 *   - Inline create row at the top (input + Create) — no dialog.
 *   - Inline rename via "Rename" button → name becomes input, Enter/blur to
 *     commit, Escape to cancel.
 *   - Reorder via ↑ / ↓ buttons (drag-to-reorder lives in the header pill
 *     strip).
 *   - Dirty marker mirrors the header pill's amber dot.
 *   - Auto-save toggle + Save/Discard buttons surface only when relevant.
 *
 * Switching a dashboard from the drawer goes through `switchTo`, which may
 * set `pendingSwitch` on the hook and surface the unsaved-changes confirm
 * dialog rendered by `DashboardSwitcher` in the header.
 */

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import type { UseDashboardsResult } from "../../hooks/useDashboards";
import "./DashboardsSection.css";

interface Props {
  ds: UseDashboardsResult;
}

export function DashboardsSection({ ds }: Props) {
  const {
    dashboards,
    autoSave,
    switchTo,
    create,
    save,
    discard,
    rename,
    remove,
    reorder,
    setAutoSave,
  } = ds;

  const [createName, setCreateName] = useState("");
  const [renamingName, setRenamingName] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");

  const createDisabled = createName.trim().length === 0;

  const handleCreate = useCallback(() => {
    const name = createName.trim();
    if (!name) return;
    setCreateName("");
    create(name)
      .then(() => switchTo(name))
      .catch(console.error);
  }, [createName, create, switchTo]);

  const handleCreateKey = useCallback(
    (e: KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter") handleCreate();
    },
    [handleCreate]
  );

  const startRename = useCallback((name: string) => {
    setRenamingName(name);
    setRenameDraft(name);
  }, []);

  const cancelRename = useCallback(() => {
    setRenamingName(null);
    setRenameDraft("");
  }, []);

  const commitRename = useCallback(() => {
    const next = renameDraft.trim();
    const prev = renamingName;
    setRenamingName(null);
    setRenameDraft("");
    if (!prev || !next || next === prev) return;
    rename(prev, next).catch(console.error);
  }, [renameDraft, renamingName, rename]);

  const handleRenameKey = useCallback(
    (e: KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter") commitRename();
      else if (e.key === "Escape") cancelRename();
    },
    [commitRename, cancelRename]
  );

  const moveBy = useCallback(
    (name: string, delta: number) => {
      const names = dashboards.map((d) => d.name);
      const from = names.indexOf(name);
      if (from === -1) return;
      const to = from + delta;
      if (to < 0 || to >= names.length) return;
      const next = [...names];
      [next[from], next[to]] = [next[to], next[from]];
      reorder(next).catch(console.error);
    },
    [dashboards, reorder]
  );

  const active = useMemo(() => dashboards.find((d) => d.active), [dashboards]);
  const showDirtyControls = active?.dirty === true && !autoSave;

  // Auto-focus the rename input when entering rename mode.
  const renameInputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (renamingName) renameInputRef.current?.select();
  }, [renamingName]);

  return (
    <div className="ot-dashboards">
      <header className="ot-dashboards__head">
        <h2 className="ot-dashboards__title">Dashboards</h2>
        <p className="ot-dashboards__subtitle">
          Switch between layouts, rename or reorder them, and control how changes are saved.
        </p>
      </header>

      <section className="ot-dashboards__row ot-dashboards__autosave">
        <label className="ot-dashboards__toggle">
          <input
            type="checkbox"
            checked={autoSave}
            onChange={(e) => setAutoSave(e.target.checked).catch(console.error)}
          />
          <span>Auto-save layout changes</span>
        </label>
        {showDirtyControls && (
          <span className="ot-dashboards__dirty-actions">
            <button
              type="button"
              className="ot-dashboards__btn ot-dashboards__btn--primary"
              onClick={() => save().catch(console.error)}
            >
              Save now
            </button>
            <button
              type="button"
              className="ot-dashboards__btn"
              onClick={() => discard().catch(console.error)}
            >
              Discard
            </button>
          </span>
        )}
      </section>

      <section className="ot-dashboards__row ot-dashboards__create">
        <input
          type="text"
          className="ot-dashboards__input"
          placeholder="New dashboard name…"
          value={createName}
          onChange={(e) => setCreateName(e.target.value)}
          onKeyDown={handleCreateKey}
        />
        <button
          type="button"
          className="ot-dashboards__btn ot-dashboards__btn--primary"
          disabled={createDisabled}
          onClick={handleCreate}
        >
          Create
        </button>
      </section>

      <ul className="ot-dashboards__list">
        {dashboards.length === 0 ? (
          <li className="ot-dashboards__empty">
            No dashboards yet. Create one above to get started.
          </li>
        ) : (
          dashboards.map((d, idx) => {
            const isRenaming = renamingName === d.name;
            const isFirst = idx === 0;
            const isLast = idx === dashboards.length - 1;

            return (
              <li
                key={d.name}
                className={`ot-dashboards__item${d.active ? " ot-dashboards__item--active" : ""}`}
              >
                <span className="ot-dashboards__item-head">
                  {d.dirty && (
                    <span className="ot-dashboards__item-dirty" title="Unsaved changes" />
                  )}
                  {isRenaming ? (
                    <input
                      ref={renameInputRef}
                      type="text"
                      className="ot-dashboards__input ot-dashboards__rename-input"
                      value={renameDraft}
                      onChange={(e) => setRenameDraft(e.target.value)}
                      onKeyDown={handleRenameKey}
                      onBlur={commitRename}
                    />
                  ) : (
                    <button
                      type="button"
                      className="ot-dashboards__name"
                      onClick={() => switchTo(d.name).catch(console.error)}
                      disabled={d.active}
                      title={d.active ? "Active dashboard" : `Switch to ${d.name}`}
                    >
                      {d.name}
                      {d.active && <span className="ot-dashboards__active-tag">Active</span>}
                    </button>
                  )}
                </span>

                <span className="ot-dashboards__item-actions">
                  {isRenaming ? (
                    <>
                      <button
                        type="button"
                        className="ot-dashboards__btn ot-dashboards__btn--primary"
                        onClick={commitRename}
                      >
                        Save
                      </button>
                      <button
                        type="button"
                        className="ot-dashboards__btn"
                        onClick={cancelRename}
                      >
                        Cancel
                      </button>
                    </>
                  ) : (
                    <>
                      <button
                        type="button"
                        className="ot-dashboards__icon-btn"
                        onClick={() => moveBy(d.name, -1)}
                        disabled={isFirst}
                        title="Move up"
                        aria-label="Move up"
                      >
                        ↑
                      </button>
                      <button
                        type="button"
                        className="ot-dashboards__icon-btn"
                        onClick={() => moveBy(d.name, +1)}
                        disabled={isLast}
                        title="Move down"
                        aria-label="Move down"
                      >
                        ↓
                      </button>
                      <button
                        type="button"
                        className="ot-dashboards__btn"
                        onClick={() => startRename(d.name)}
                      >
                        Rename
                      </button>
                      <button
                        type="button"
                        className="ot-dashboards__btn ot-dashboards__btn--danger"
                        onClick={() => remove(d.name).catch(console.error)}
                      >
                        Delete
                      </button>
                    </>
                  )}
                </span>
              </li>
            );
          })
        )}
      </ul>
    </div>
  );
}
