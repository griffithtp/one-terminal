/**
 * Write authentication for the App Directory API.
 *
 * `POST` / `PUT` / `DELETE /v2/apps` need `Authorization: Bearer <token>`.
 *
 * - The Desktop Agent opens this UI with its per-run token in the URL
 *   fragment (`#ot-appd-token=…`). The fragment is never sent to the server,
 *   so fetching this page doesn't reveal it. It is moved into
 *   `sessionStorage` and removed from the address bar on load.
 * - A standalone server's operator enters the server's `APPD_WRITE_TOKEN`
 *   when a write is first refused.
 */

const KEY = "ot-appd-token";

function stored(): string | null {
  try {
    return sessionStorage.getItem(KEY);
  } catch {
    return null;
  }
}

function store(token: string): void {
  try {
    sessionStorage.setItem(KEY, token);
  } catch {
    // Private mode or storage disabled: the token lasts for this page only.
  }
  memoryToken = token;
}

let memoryToken: string | null = null;

/** Move a token from the URL fragment into session storage. Call once on load. */
export function captureTokenFromFragment(): void {
  const hash = window.location.hash.replace(/^#/, "");
  if (!hash) return;
  const params = new URLSearchParams(hash);
  const token = params.get(KEY);
  if (!token) return;
  store(token);
  params.delete(KEY);
  const rest = params.toString();
  const { pathname, search } = window.location;
  window.history.replaceState(null, "", `${pathname}${search}${rest ? `#${rest}` : ""}`);
}

function currentToken(): string | null {
  return memoryToken ?? stored();
}

function withAuth(init: RequestInit, token: string | null): RequestInit {
  const headers = new Headers(init.headers);
  if (token) headers.set("Authorization", `Bearer ${token}`);
  return { ...init, headers };
}

/**
 * `fetch` for a write. On `401` asks once for a token and retries with it.
 * A `403` means the directory is read-only: the response is returned as is.
 */
export async function writeFetch(url: string, init: RequestInit): Promise<Response> {
  const res = await fetch(url, withAuth(init, currentToken()));
  if (res.status !== 401) return res;
  const entered = window.prompt(
    "Changing apps needs this App Directory's write token. Open the App Directory from the " +
      "Desktop Agent, or enter the server's APPD_WRITE_TOKEN:"
  );
  if (!entered?.trim()) return res;
  store(entered.trim());
  return fetch(url, withAuth(init, entered.trim()));
}

/** A user-facing message for a refused write, or `undefined` for other errors. */
export function writeRefusal(status: number): string | undefined {
  if (status === 401) return "Not authorised to change apps: the write token is missing or wrong.";
  if (status === 403) return "This App Directory is read-only.";
  return undefined;
}
