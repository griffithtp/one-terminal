import { ResolveError, type AppIntent, type Context } from "@finos/fdc3-standard";
import type { IntentResolver } from "./agent";

type Choice = NonNullable<Awaited<ReturnType<IntentResolver["chooseIntent"]>>>;

const STYLE = `
  .overlay { position: fixed; inset: 0; display: flex; align-items: center; justify-content: center;
    background: rgba(0, 0, 0, .45); font: 13px system-ui, sans-serif; z-index: 2147483647; }
  .dialog { background: #1e2228; color: #e6e8eb; border-radius: 8px; padding: 16px; width: min(360px, 90vw);
    max-height: 80vh; overflow: auto; box-shadow: 0 8px 32px rgba(0, 0, 0, .5); }
  h2 { font-size: 14px; margin: 0 0 12px; }
  h3 { font-size: 12px; margin: 12px 0 4px; color: #9aa3ad; font-weight: 500; }
  button { display: block; width: 100%; text-align: left; padding: 8px; margin: 2px 0; border: 0; border-radius: 4px;
    background: #2a2f37; color: inherit; font: inherit; cursor: pointer; }
  button:hover, button:focus-visible { background: #3a4150; outline: none; }
  button.cancel { text-align: center; margin-top: 12px; background: transparent; color: #9aa3ad; }
`;

/**
 * Asks the user to choose when several apps can handle an intent. The dialog
 * renders in a shadow root so that the host page's CSS can't affect it.
 */
export function createDialogResolver(): IntentResolver {
  return {
    async connect() {},
    async disconnect() {},
    chooseIntent(appIntents: AppIntent[], _context: Context) {
      return new Promise<Choice>((resolve, reject) => {
        const host = document.createElement("div");
        const root = host.attachShadow({ mode: "closed" });
        const style = document.createElement("style");
        style.textContent = STYLE;
        const overlay = document.createElement("div");
        overlay.className = "overlay";
        const dialog = document.createElement("div");
        dialog.className = "dialog";
        dialog.setAttribute("role", "dialog");
        dialog.setAttribute("aria-modal", "true");
        const heading = document.createElement("h2");
        heading.textContent = "Open with";
        dialog.append(heading);

        const finish = (settle: () => void) => {
          document.removeEventListener("keydown", onKey);
          host.remove();
          settle();
        };
        const cancel = () => finish(() => reject(new Error(ResolveError.UserCancelled)));
        const onKey = (e: KeyboardEvent) => {
          if (e.key === "Escape") cancel();
        };

        for (const appIntent of appIntents) {
          if (appIntents.length > 1) {
            const title = document.createElement("h3");
            title.textContent = appIntent.intent.displayName ?? appIntent.intent.name;
            dialog.append(title);
          }
          for (const app of appIntent.apps) {
            const button = document.createElement("button");
            const name = app.title ?? app.appId;
            button.textContent = app.instanceId
              ? `${name} (running, ${app.instanceId.slice(0, 8)})`
              : `${name} (new window)`;
            button.addEventListener("click", () =>
              finish(() =>
                resolve({
                  intent: appIntent.intent.name,
                  appId: { appId: app.appId, instanceId: app.instanceId },
                })
              )
            );
            dialog.append(button);
          }
        }

        const cancelButton = document.createElement("button");
        cancelButton.className = "cancel";
        cancelButton.textContent = "Cancel";
        cancelButton.addEventListener("click", cancel);
        dialog.append(cancelButton);

        overlay.append(dialog);
        root.append(style, overlay);
        document.addEventListener("keydown", onKey);
        document.documentElement.append(host);
        (dialog.querySelector("button") as HTMLButtonElement | null)?.focus();
      });
    },
  };
}
