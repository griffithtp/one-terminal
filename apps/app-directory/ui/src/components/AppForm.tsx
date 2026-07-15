import React, { useEffect, useState } from "react";
import type { AppD, AppType, EngineBinding, EngineCatalog, EngineFamily, OsKey } from "../types";

function detectCurrentOs(): OsKey {
  const ua = navigator.userAgent;
  if (/Mac|iPhone|iPad/i.test(ua)) return "macos";
  if (/Windows/i.test(ua)) return "windows";
  return "linux";
}

const OS_LABELS: Record<OsKey, string> = {
  windows: "Windows",
  macos: "macOS",
  linux: "Linux",
};

// ── Form state types ───────────────────────────────────────────────────────────

interface IntentListener {
  intentName: string;
  displayName: string;
  contexts: string; // comma-separated FDC3 context types
  resultType: string;
}

interface IntentRaise {
  intentName: string;
  contexts: string; // comma-separated FDC3 context types
}

interface FormState {
  // Required
  appId: string;
  name: string;
  type: AppType;
  url: string;
  // Optional metadata
  title: string;
  description: string;
  version: string;
  tooltip: string;
  lang: string;
  publisher: string;
  contactEmail: string;
  supportEmail: string;
  moreInfo: string;
  categories: string; // comma-separated
  // Interop
  intentListeners: IntentListener[];
  intentRaises: IntentRaise[];
  channelBroadcasts: string; // comma-separated
  channelListensFor: string; // comma-separated
  // Engine bindings — list of supported engines per OS. Users are offered
  // this list at tab-open time to pick which engine to launch with.
  engineBindings: Partial<Record<OsKey, EngineBinding[]>>;
}

// ── Conversions ────────────────────────────────────────────────────────────────

function appDToForm(app: AppD): FormState {
  return {
    appId: app.appId,
    name: app.name,
    type: app.type,
    url: app.details.url,
    title: app.title ?? "",
    description: app.description ?? "",
    version: app.version ?? "",
    tooltip: app.tooltip ?? "",
    lang: app.lang ?? "",
    publisher: app.publisher ?? "",
    contactEmail: app.contactEmail ?? "",
    supportEmail: app.supportEmail ?? "",
    moreInfo: app.moreInfo ?? "",
    categories: app.categories?.join(", ") ?? "",
    intentListeners: Object.entries(app.interop?.intents?.listensFor ?? {}).map(
      ([intentName, def]) => ({
        intentName,
        displayName: def.displayName ?? "",
        contexts: def.contexts.join(", "),
        resultType: def.resultType ?? "",
      })
    ),
    intentRaises: Object.entries(app.interop?.intents?.raises ?? {}).map(([intentName, ctxs]) => ({
      intentName,
      contexts: ctxs.join(", "),
    })),
    channelBroadcasts: app.interop?.userChannels?.broadcasts.join(", ") ?? "",
    channelListensFor: app.interop?.userChannels?.listensFor.join(", ") ?? "",
    engineBindings: Object.fromEntries(
      Object.entries(app.engineBindings ?? {})
        .filter(([, list]) => Array.isArray(list))
        .map(([os, list]) => [os, (list as EngineBinding[]).map((b) => ({ ...b }))])
    ) as Partial<Record<OsKey, EngineBinding[]>>,
  };
}

const BLANK_FORM: FormState = {
  appId: "",
  name: "",
  type: "web",
  url: "",
  title: "",
  description: "",
  version: "",
  tooltip: "",
  lang: "en-US",
  publisher: "",
  contactEmail: "",
  supportEmail: "",
  moreInfo: "",
  categories: "",
  intentListeners: [],
  intentRaises: [],
  channelBroadcasts: "",
  channelListensFor: "",
  engineBindings: {},
};

function splitCsv(s: string): string[] {
  return s
    .split(",")
    .map((t) => t.trim())
    .filter(Boolean);
}

function formToAppD(form: FormState): AppD {
  const app: AppD = {
    appId: form.appId.trim(),
    name: form.name.trim(),
    type: form.type,
    details: { url: form.url.trim() },
  };

  // Optional scalar fields
  const scalars = [
    "title",
    "description",
    "version",
    "tooltip",
    "lang",
    "publisher",
    "contactEmail",
    "supportEmail",
    "moreInfo",
  ] as const;
  for (const key of scalars) {
    const v = form[key].trim();
    if (v) (app as unknown as Record<string, unknown>)[key] = v;
  }

  const cats = splitCsv(form.categories);
  if (cats.length) app.categories = cats;

  // Intent listeners → interop.intents.listensFor
  const listensFor: NonNullable<NonNullable<AppD["interop"]>["intents"]>["listensFor"] = {};
  for (const il of form.intentListeners) {
    const name = il.intentName.trim();
    if (!name) continue;
    listensFor[name] = {
      contexts: splitCsv(il.contexts),
      ...(il.displayName.trim() && { displayName: il.displayName.trim() }),
      ...(il.resultType.trim() && { resultType: il.resultType.trim() }),
    };
  }

  // Intent raises → interop.intents.raises
  const raises: NonNullable<NonNullable<AppD["interop"]>["intents"]>["raises"] = {};
  for (const ir of form.intentRaises) {
    const name = ir.intentName.trim();
    if (!name) continue;
    raises[name] = splitCsv(ir.contexts);
  }

  const broadcasts = splitCsv(form.channelBroadcasts);
  const chListensFor = splitCsv(form.channelListensFor);

  const hasIntents = Object.keys(listensFor).length > 0 || Object.keys(raises).length > 0;
  const hasChannels = broadcasts.length > 0 || chListensFor.length > 0;

  if (hasIntents || hasChannels) {
    app.interop = {};
    if (hasIntents) {
      app.interop.intents = {};
      if (Object.keys(listensFor).length) app.interop.intents.listensFor = listensFor;
      if (Object.keys(raises).length) app.interop.intents.raises = raises;
    }
    if (hasChannels) {
      app.interop.userChannels = { broadcasts, listensFor: chListensFor };
    }
  }

  const bindings = Object.entries(form.engineBindings).reduce<
    Partial<Record<OsKey, EngineBinding[]>>
  >((acc, [os, list]) => {
    if (!Array.isArray(list)) return acc;
    const clean = list
      .filter((b): b is EngineBinding => !!b && !!b.family && !!b.version)
      .map((b) => ({ family: b.family, version: b.version }));
    if (clean.length) acc[os as OsKey] = clean;
    return acc;
  }, {});
  if (Object.keys(bindings).length > 0) {
    app.engineBindings = bindings;
  }

  return app;
}

// ── Component ──────────────────────────────────────────────────────────────────

interface Props {
  existingApp: AppD | null;
  onSave: (message: string) => void;
  onCancel: () => void;
}

export default function AppForm({ existingApp, onSave, onCancel }: Props) {
  const isEdit = existingApp !== null;

  const [form, setForm] = useState<FormState>(() =>
    isEdit ? appDToForm(existingApp!) : { ...BLANK_FORM }
  );
  const [errors, setErrors] = useState<Partial<Record<keyof FormState, string>>>({});
  const [submitting, setSubmitting] = useState(false);
  const [apiError, setApiError] = useState<string | null>(null);

  const [catalog, setCatalog] = useState<EngineCatalog>({});
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [engineOsTab, setEngineOsTab] = useState<OsKey>(() => detectCurrentOs());

  useEffect(() => {
    let cancelled = false;
    fetch("/v2/engines")
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((data: { engines?: EngineCatalog }) => {
        if (!cancelled) setCatalog(data.engines ?? {});
      })
      .catch((e: unknown) => {
        if (!cancelled) setCatalogError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const osEntries = catalog[engineOsTab] ?? [];
  const osBindings: EngineBinding[] = form.engineBindings[engineOsTab] ?? [];

  function setOsBindings(next: EngineBinding[]) {
    setForm((f) => {
      const bindings = { ...f.engineBindings };
      if (next.length) bindings[engineOsTab] = next;
      else delete bindings[engineOsTab];
      return { ...f, engineBindings: bindings };
    });
  }

  function addEngineRow() {
    if (osEntries.length === 0) return;
    // Seed the new row with the first catalog entry that isn't already in the
    // list, falling back to the first entry if every one is already picked.
    const takenKeys = new Set(osBindings.map((b) => `${b.family}@${b.version}`));
    const firstFree =
      osEntries.find((e) => !takenKeys.has(`${e.family}@${e.version}`)) ?? osEntries[0];
    setOsBindings([...osBindings, { family: firstFree.family, version: firstFree.version }]);
  }

  function removeEngineRow(index: number) {
    setOsBindings(osBindings.filter((_, i) => i !== index));
  }

  function updateEngineRow(index: number, patch: Partial<EngineBinding>) {
    const next = osBindings.map((b, i) => (i === index ? { ...b, ...patch } : b));
    // Changing family resets version to a valid one for that family.
    if (patch.family) {
      const first = osEntries.find((e) => e.family === patch.family);
      next[index] = {
        family: patch.family,
        version: first?.version ?? osBindings[index].version,
      };
    }
    setOsBindings(next);
  }

  function familyOptionsForRow(): EngineFamily[] {
    return Array.from(new Set(osEntries.map((e) => e.family))) as EngineFamily[];
  }

  function versionOptionsForRow(family: EngineFamily | undefined) {
    if (!family) return [];
    return osEntries.filter((e) => e.family === family);
  }

  // ── Field helpers ────────────────────────────────────────────────────────────

  function set<K extends keyof FormState>(field: K, value: FormState[K]) {
    setForm((f) => ({ ...f, [field]: value }));
    if (errors[field])
      setErrors((e) => {
        const n = { ...e };
        delete n[field];
        return n;
      });
  }

  // ── Intent listeners ─────────────────────────────────────────────────────────

  const addListener = () =>
    set("intentListeners", [
      ...form.intentListeners,
      { intentName: "", displayName: "", contexts: "", resultType: "" },
    ]);

  const removeListener = (i: number) =>
    set(
      "intentListeners",
      form.intentListeners.filter((_, idx) => idx !== i)
    );

  const updateListener = (i: number, field: keyof IntentListener, value: string) =>
    set(
      "intentListeners",
      form.intentListeners.map((il, idx) => (idx === i ? { ...il, [field]: value } : il))
    );

  // ── Intent raises ────────────────────────────────────────────────────────────

  const addRaise = () =>
    set("intentRaises", [...form.intentRaises, { intentName: "", contexts: "" }]);

  const removeRaise = (i: number) =>
    set(
      "intentRaises",
      form.intentRaises.filter((_, idx) => idx !== i)
    );

  const updateRaise = (i: number, field: keyof IntentRaise, value: string) =>
    set(
      "intentRaises",
      form.intentRaises.map((ir, idx) => (idx === i ? { ...ir, [field]: value } : ir))
    );

  // ── Validation ───────────────────────────────────────────────────────────────

  function validate(): boolean {
    const e: Partial<Record<keyof FormState, string>> = {};

    if (!form.appId.trim()) {
      e.appId = "App ID is required";
    } else if (!/^[\w-]+$/.test(form.appId.trim())) {
      e.appId = "Only letters, numbers, hyphens and underscores are allowed";
    }

    if (!form.name.trim()) e.name = "Name is required";

    if (!form.url.trim()) {
      e.url = "Launch URL is required";
    } else {
      try {
        new URL(form.url.trim());
      } catch {
        e.url = "Enter a valid URL (e.g. http://localhost:3010)";
      }
    }

    setErrors(e);
    return Object.keys(e).length === 0;
  }

  // ── Submit ───────────────────────────────────────────────────────────────────

  const handleSubmit = async (ev: React.FormEvent) => {
    ev.preventDefault();
    if (!validate()) return;

    setSubmitting(true);
    setApiError(null);

    try {
      const body = formToAppD(form);
      const method = isEdit ? "PUT" : "POST";
      const url = isEdit ? `/v2/apps/${existingApp!.appId}` : "/v2/apps";

      const res = await fetch(url, {
        method,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });

      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        throw new Error((data as { message?: string }).message ?? `HTTP ${res.status}`);
      }

      onSave(
        isEdit ? `"${body.name}" updated successfully.` : `"${body.name}" registered successfully.`
      );
    } catch (e: unknown) {
      setApiError(e instanceof Error ? e.message : "Unknown error");
    } finally {
      setSubmitting(false);
    }
  };

  // ── Render ───────────────────────────────────────────────────────────────────

  return (
    <div className="app-form">
      <div className="form-header">
        <h2>{isEdit ? `Edit: ${existingApp!.name}` : "Register New Application"}</h2>
        <button className="btn btn-secondary" onClick={onCancel}>
          Cancel
        </button>
      </div>

      <form onSubmit={handleSubmit} noValidate>
        {/* ── Identity ──────────────────────────────────────────────────────── */}
        <div className="form-section">
          <div className="section-title">Identity — FDC3 Required Fields</div>
          <div className="form-grid-3">
            <div className="field">
              <label>
                App ID<span className="req">*</span>
              </label>
              <input
                type="text"
                value={form.appId}
                onChange={(e) => set("appId", e.target.value)}
                placeholder="e.g. my-app"
                disabled={isEdit}
                className={errors.appId ? "is-error" : ""}
              />
              {errors.appId && <span className="field-error">{errors.appId}</span>}
            </div>
            <div className="field">
              <label>
                Name<span className="req">*</span>
              </label>
              <input
                type="text"
                value={form.name}
                onChange={(e) => set("name", e.target.value)}
                placeholder="e.g. My App"
                className={errors.name ? "is-error" : ""}
              />
              {errors.name && <span className="field-error">{errors.name}</span>}
            </div>
            <div className="field">
              <label>
                Type<span className="req">*</span>
              </label>
              <select value={form.type} onChange={(e) => set("type", e.target.value as AppType)}>
                <option value="web">web</option>
                <option value="native">native</option>
                <option value="citrix">citrix</option>
                <option value="onlineNative">onlineNative</option>
                <option value="other">other</option>
              </select>
            </div>
          </div>
          <div className="form-grid-1" style={{ marginTop: 12 }}>
            <div className="field">
              <label>
                Launch URL<span className="req">*</span>
                <span className="hint">(details.url)</span>
              </label>
              <input
                type="url"
                value={form.url}
                onChange={(e) => set("url", e.target.value)}
                placeholder="http://localhost:3010"
                className={errors.url ? "is-error" : ""}
              />
              {errors.url && <span className="field-error">{errors.url}</span>}
            </div>
          </div>
        </div>

        {/* ── Metadata ──────────────────────────────────────────────────────── */}
        <div className="form-section">
          <div className="section-title">Metadata — Optional</div>
          <div className="form-grid">
            <div className="field">
              <label>Title</label>
              <input
                type="text"
                value={form.title}
                onChange={(e) => set("title", e.target.value)}
                placeholder="Human-readable display title"
              />
            </div>
            <div className="field">
              <label>Version</label>
              <input
                type="text"
                value={form.version}
                onChange={(e) => set("version", e.target.value)}
                placeholder="e.g. 1.0.0"
              />
            </div>
            <div className="field col-span-2">
              <label>Description</label>
              <textarea
                value={form.description}
                onChange={(e) => set("description", e.target.value)}
                rows={3}
                placeholder="Short description of the application…"
              />
            </div>
            <div className="field">
              <label>Tooltip</label>
              <input
                type="text"
                value={form.tooltip}
                onChange={(e) => set("tooltip", e.target.value)}
                placeholder="Short hover tooltip text"
              />
            </div>
            <div className="field">
              <label>Language</label>
              <input
                type="text"
                value={form.lang}
                onChange={(e) => set("lang", e.target.value)}
                placeholder="en-US"
              />
            </div>
            <div className="field">
              <label>Publisher</label>
              <input
                type="text"
                value={form.publisher}
                onChange={(e) => set("publisher", e.target.value)}
                placeholder="Organisation name"
              />
            </div>
            <div className="field">
              <label>More Info URL</label>
              <input
                type="text"
                value={form.moreInfo}
                onChange={(e) => set("moreInfo", e.target.value)}
                placeholder="https://docs.example.com/app"
              />
            </div>
            <div className="field">
              <label>Contact Email</label>
              <input
                type="email"
                value={form.contactEmail}
                onChange={(e) => set("contactEmail", e.target.value)}
                placeholder="contact@example.com"
              />
            </div>
            <div className="field">
              <label>Support Email</label>
              <input
                type="email"
                value={form.supportEmail}
                onChange={(e) => set("supportEmail", e.target.value)}
                placeholder="support@example.com"
              />
            </div>
            <div className="field col-span-2">
              <label>
                Categories
                <span className="hint">(comma-separated)</span>
              </label>
              <input
                type="text"
                value={form.categories}
                onChange={(e) => set("categories", e.target.value)}
                placeholder="Analytics, Charting, FX, Trading"
              />
            </div>
          </div>
        </div>

        {/* ── Intent Listeners ──────────────────────────────────────────────── */}
        <div className="form-section">
          <div className="section-title">
            Interop — Intent Listeners
            <span className="hint" style={{ marginLeft: 8 }}>
              interop.intents.listensFor
            </span>
          </div>

          {form.intentListeners.length > 0 && (
            <div className="dyn-header dyn-header-listener" style={{ marginBottom: 4 }}>
              <span>Intent Name</span>
              <span>Display Name</span>
              <span>
                Context Types <em style={{ fontStyle: "normal", opacity: 0.7 }}>(comma-sep)</em>
              </span>
              <span>Result Type</span>
              <span />
            </div>
          )}

          <div className="dyn-list">
            {form.intentListeners.map((il, i) => (
              <div key={i} className="dyn-row dyn-row-listener">
                <input
                  type="text"
                  value={il.intentName}
                  onChange={(e) => updateListener(i, "intentName", e.target.value)}
                  placeholder="ViewChart"
                />
                <input
                  type="text"
                  value={il.displayName}
                  onChange={(e) => updateListener(i, "displayName", e.target.value)}
                  placeholder="View Chart"
                />
                <input
                  type="text"
                  value={il.contexts}
                  onChange={(e) => updateListener(i, "contexts", e.target.value)}
                  placeholder="fdc3.instrument"
                />
                <input
                  type="text"
                  value={il.resultType}
                  onChange={(e) => updateListener(i, "resultType", e.target.value)}
                  placeholder="fdc3.chart"
                />
                <button type="button" className="btn-remove" onClick={() => removeListener(i)}>
                  ✕
                </button>
              </div>
            ))}
          </div>
          <button type="button" className="btn-add-row" onClick={addListener}>
            + Add Intent Listener
          </button>
        </div>

        {/* ── Intent Raises ─────────────────────────────────────────────────── */}
        <div className="form-section">
          <div className="section-title">
            Interop — Intent Raises
            <span className="hint" style={{ marginLeft: 8 }}>
              interop.intents.raises
            </span>
          </div>

          {form.intentRaises.length > 0 && (
            <div className="dyn-header dyn-header-raises" style={{ marginBottom: 4 }}>
              <span>Intent Name</span>
              <span>
                Context Types <em style={{ fontStyle: "normal", opacity: 0.7 }}>(comma-sep)</em>
              </span>
              <span />
            </div>
          )}

          <div className="dyn-list">
            {form.intentRaises.map((ir, i) => (
              <div key={i} className="dyn-row dyn-row-raises">
                <input
                  type="text"
                  value={ir.intentName}
                  onChange={(e) => updateRaise(i, "intentName", e.target.value)}
                  placeholder="ViewChart"
                />
                <input
                  type="text"
                  value={ir.contexts}
                  onChange={(e) => updateRaise(i, "contexts", e.target.value)}
                  placeholder="fdc3.instrument"
                />
                <button type="button" className="btn-remove" onClick={() => removeRaise(i)}>
                  ✕
                </button>
              </div>
            ))}
          </div>
          <button type="button" className="btn-add-row" onClick={addRaise}>
            + Add Intent Raises Entry
          </button>
        </div>

        {/* ── User Channels ─────────────────────────────────────────────────── */}
        <div className="form-section">
          <div className="section-title">
            Interop — User Channels
            <span className="hint" style={{ marginLeft: 8 }}>
              interop.userChannels
            </span>
          </div>
          <div className="form-grid">
            <div className="field">
              <label>
                Broadcasts
                <span className="hint">(context types, comma-sep)</span>
              </label>
              <input
                type="text"
                value={form.channelBroadcasts}
                onChange={(e) => set("channelBroadcasts", e.target.value)}
                placeholder="fdc3.instrument, fx.rate"
              />
            </div>
            <div className="field">
              <label>
                Listens For
                <span className="hint">(context types, comma-sep)</span>
              </label>
              <input
                type="text"
                value={form.channelListensFor}
                onChange={(e) => set("channelListensFor", e.target.value)}
                placeholder="fdc3.instrument, fx.rate"
              />
            </div>
          </div>
        </div>

        {/* ── Browser Engine ────────────────────────────────────────────────── */}
        <div className="form-section">
          <div className="section-title">
            Browser Engines
            <span className="hint" style={{ marginLeft: 8 }}>
              engineBindings — supported runtimes per OS; the user picks one when opening a tab.
              Empty means launcher default.
            </span>
          </div>

          <div className="nav-tabs" style={{ marginBottom: 12 }}>
            {(["windows", "macos"] as OsKey[]).map((os) => {
              const count = form.engineBindings[os]?.length ?? 0;
              return (
                <button
                  key={os}
                  type="button"
                  className={engineOsTab === os ? "active" : ""}
                  onClick={() => setEngineOsTab(os)}
                >
                  {OS_LABELS[os]}
                  {count > 0 && (
                    <span className="hint" style={{ marginLeft: 6 }}>
                      ({count})
                    </span>
                  )}
                </button>
              );
            })}
          </div>

          {catalogError && (
            <div className="api-error" style={{ marginBottom: 12 }}>
              Engine catalog failed to load: {catalogError}
            </div>
          )}

          {osBindings.length > 0 && (
            <div
              className="dyn-header"
              style={{
                display: "grid",
                gridTemplateColumns: "1fr 2fr auto",
                gap: 8,
                marginBottom: 4,
              }}
            >
              <span>Engine family</span>
              <span>Version</span>
              <span />
            </div>
          )}

          <div className="dyn-list">
            {osBindings.map((b, i) => (
              <div
                key={i}
                className="dyn-row"
                style={{
                  display: "grid",
                  gridTemplateColumns: "1fr 2fr auto",
                  gap: 8,
                }}
              >
                <select
                  value={b.family}
                  onChange={(e) => updateEngineRow(i, { family: e.target.value as EngineFamily })}
                  disabled={osEntries.length === 0}
                >
                  {familyOptionsForRow().map((family) => (
                    <option key={family} value={family}>
                      {family}
                    </option>
                  ))}
                </select>
                <select
                  value={b.version}
                  onChange={(e) => updateEngineRow(i, { version: e.target.value })}
                  disabled={osEntries.length === 0}
                >
                  {versionOptionsForRow(b.family).map((entry) => (
                    <option key={entry.version} value={entry.version}>
                      {entry.label}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  className="btn-remove"
                  onClick={() => removeEngineRow(i)}
                  title="Remove engine"
                >
                  ✕
                </button>
              </div>
            ))}
          </div>

          <button
            type="button"
            className="btn-add-row"
            onClick={addEngineRow}
            disabled={osEntries.length === 0}
          >
            + Add Engine for {OS_LABELS[engineOsTab]}
          </button>

          {osBindings.length === 0 && osEntries.length > 0 && (
            <p className="hint" style={{ marginTop: 8 }}>
              No engines configured — launcher default will be used on {OS_LABELS[engineOsTab]}.
            </p>
          )}
        </div>

        {/* ── Error + Actions ───────────────────────────────────────────────── */}
        {apiError && <div className="api-error">Error: {apiError}</div>}

        <div className="form-actions">
          <button type="button" className="btn btn-secondary" onClick={onCancel}>
            Cancel
          </button>
          <button type="submit" className="btn btn-primary" disabled={submitting}>
            {submitting ? "Saving…" : isEdit ? "Save Changes" : "Register Application"}
          </button>
        </div>
      </form>
    </div>
  );
}
