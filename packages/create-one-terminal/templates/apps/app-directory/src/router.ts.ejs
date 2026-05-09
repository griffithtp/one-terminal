import { Router } from "express";
import type { Request, Response } from "express";
import { getApps, setApps } from "./data.js";
import { getCatalog, isEngineBindingValid } from "./engines.js";
import type {
  AppsListResponse,
  AppD,
  EngineBinding,
  EngineCatalogResponse,
  ErrorResponse,
  OsKey,
} from "./types.js";

const router = Router();

// ── Helpers ────────────────────────────────────────────────────────────────────

/**
 * Parse a basic OData-style `$filter` string.
 *
 * Supported forms (case-insensitive field names):
 *   name eq 'ticker-plant'
 *   appId eq 'chart-viewer'
 *   categories/any(c: c eq 'Analytics')
 *
 * All other expressions fall through and return every app (no filtering).
 */
function applyFilter(apps: AppD[], filter: string): AppD[] {
  const lower = filter.trim().toLowerCase();

  // name eq '<value>'
  const nameEq = lower.match(/^name\s+eq\s+'([^']+)'$/);
  if (nameEq) {
    const v = nameEq[1];
    return apps.filter((a) => a.name.toLowerCase().includes(v));
  }

  // appId eq '<value>'
  const appIdEq = lower.match(/^appid\s+eq\s+'([^']+)'$/);
  if (appIdEq) {
    const v = appIdEq[1];
    return apps.filter((a) => a.appId.toLowerCase() === v);
  }

  // categories/any(c: c eq '<value>')
  const catAny = lower.match(/^categories\/any\(c:\s*c\s+eq\s+'([^']+)'\)$/);
  if (catAny) {
    const v = catAny[1];
    return apps.filter((a) => a.categories?.some((c) => c.toLowerCase() === v));
  }

  // intent listensFor: intents/listensFor/<IntentName>
  const intentListens = lower.match(/^intents\/listensfor\/(\w+)$/);
  if (intentListens) {
    const intent = intentListens[1];
    return apps.filter(
      (a) =>
        a.interop?.intents?.listensFor &&
        intent in
          Object.fromEntries(
            Object.entries(a.interop.intents.listensFor).map(([k, v]) => [k.toLowerCase(), v])
          )
    );
  }

  // Unrecognised filter — return all (graceful degradation)
  return apps;
}

/**
 * Validate `engineBindings` against the engine catalog. Each OS key maps to
 * a list of supported engines; every entry must resolve to a `(family, version)`
 * pair present in the catalog, and duplicates within a single OS list are
 * rejected. Returns an error message on failure, otherwise `null`.
 */
function validateEngineBindings(
  bindings: Partial<Record<OsKey, EngineBinding[]>> | undefined
): string | null {
  if (!bindings) return null;
  for (const [os, list] of Object.entries(bindings) as [OsKey, EngineBinding[] | undefined][]) {
    if (!list) continue;
    if (!Array.isArray(list)) {
      return `engineBindings.${os}: must be an array of { family, version } entries`;
    }
    const seen = new Set<string>();
    for (const binding of list) {
      if (!binding || typeof binding.family !== "string" || typeof binding.version !== "string") {
        return `engineBindings.${os}: family and version are required strings`;
      }
      if (!isEngineBindingValid(os, binding.family, binding.version)) {
        return `engineBindings.${os}: ${binding.family}@${binding.version} is not in the catalog`;
      }
      const key = `${binding.family}@${binding.version}`;
      if (seen.has(key)) {
        return `engineBindings.${os}: duplicate entry ${key}`;
      }
      seen.add(key);
    }
  }
  return null;
}

// ── Routes ─────────────────────────────────────────────────────────────────────

/**
 * GET /v2/engines
 * Returns the catalog of available browser engines keyed by OS.
 */
router.get("/engines", (_req: Request, res: Response) => {
  const body: EngineCatalogResponse = {
    engines: getCatalog(),
    message: "OK",
  };
  res.json(body);
});

/**
 * GET /v2/apps
 * Returns all application records, optionally filtered by `$filter`.
 */
router.get("/apps", (req: Request, res: Response) => {
  let results: AppD[] = [...getApps()];

  const filter = (req.query["$filter"] as string | undefined)?.trim();
  if (filter) {
    results = applyFilter(results, filter);
  }

  const body: AppsListResponse = {
    applications: results,
    message: "OK",
  };
  res.json(body);
});

/**
 * GET /v2/apps/:appId
 * Returns a single application by `appId` (exact, case-sensitive match).
 */
router.get("/apps/:appId", (req: Request, res: Response) => {
  const { appId } = req.params;
  const app = getApps().find((a) => a.appId === appId);

  if (!app) {
    const err: ErrorResponse = { message: "AppNotFound" };
    res.status(404).json(err);
    return;
  }

  res.json(app);
});

/**
 * POST /v2/apps
 * Register a new application. `appId` must be unique.
 *
 * Body: AppD (appId, name, type, details required)
 * Response: 201 + AppD on success, 409 if appId already exists, 400 on invalid body
 */
router.post("/apps", (req: Request, res: Response) => {
  const body = req.body as Partial<AppD>;

  if (!body.appId || !body.name || !body.type || !body.details?.url) {
    const err: ErrorResponse = {
      message: "Missing required fields: appId, name, type, details.url",
    };
    res.status(400).json(err);
    return;
  }

  const engineError = validateEngineBindings(body.engineBindings);
  if (engineError) {
    res.status(400).json({ message: engineError } as ErrorResponse);
    return;
  }

  const apps = getApps();
  if (apps.find((a) => a.appId === body.appId)) {
    const err: ErrorResponse = { message: "AppAlreadyExists" };
    res.status(409).json(err);
    return;
  }

  const newApp = body as AppD;
  setApps([...apps, newApp]);
  res.status(201).json(newApp);
});

/**
 * PUT /v2/apps/:appId
 * Update an existing application (full replacement). `appId` in the body must
 * match the URL parameter.
 *
 * Body: AppD
 * Response: 200 + updated AppD, 404 if not found, 400 on invalid body
 */
router.put("/apps/:appId", (req: Request, res: Response) => {
  const { appId } = req.params;
  const body = req.body as Partial<AppD>;

  if (!body.appId || !body.name || !body.type || !body.details?.url) {
    const err: ErrorResponse = {
      message: "Missing required fields: appId, name, type, details.url",
    };
    res.status(400).json(err);
    return;
  }

  const engineError = validateEngineBindings(body.engineBindings);
  if (engineError) {
    res.status(400).json({ message: engineError } as ErrorResponse);
    return;
  }

  const apps = getApps();
  const idx = apps.findIndex((a) => a.appId === appId);
  if (idx === -1) {
    const err: ErrorResponse = { message: "AppNotFound" };
    res.status(404).json(err);
    return;
  }

  const updated = body as AppD;
  const next = [...apps];
  next[idx] = updated;
  setApps(next);
  res.json(updated);
});

/**
 * DELETE /v2/apps/:appId
 * Remove an application from the directory.
 *
 * Response: 204 on success, 404 if not found
 */
router.delete("/apps/:appId", (req: Request, res: Response) => {
  const { appId } = req.params;
  const apps = getApps();
  const idx = apps.findIndex((a) => a.appId === appId);

  if (idx === -1) {
    const err: ErrorResponse = { message: "AppNotFound" };
    res.status(404).json(err);
    return;
  }

  setApps(apps.filter((a) => a.appId !== appId));
  res.status(204).send();
});

export default router;
