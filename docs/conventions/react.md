# React / TypeScript conventions

Applies to `apps/one-terminal/src`, `apps/desktop-agent/src` and `apps/app-directory/ui`.
`packages/fdc3-plugin` and the sample apps are excluded from root lint/format.

## Tooling (enforced)

| Check  | Command             | Notes                                                                                                                                               |
| ------ | ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Format | `npm run format`    | Prettier: 2 spaces, double quotes, semicolons, `trailingComma: es5`, width 100                                                                      |
| Lint   | `npm run lint`      | ESLint flat config (`eslint.config.js`): typescript-eslint recommended, react + react-hooks recommended, jsx-a11y recommended, plus the rules below |
| Types  | `npm run typecheck` | `strict`, `noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`                                                                      |

Unused variables are errors unless prefixed with `_`. Also errors:

| Rule                                       | Scope             | In practice                                                                             |
| ------------------------------------------ | ----------------- | --------------------------------------------------------------------------------------- |
| `no-explicit-any`                          | all TS            | Use `unknown` and narrow                                                                |
| `no-non-null-assertion`                    | all TS            | No `!` — check and throw, or narrow (`if (!root) throw …`)                              |
| `consistent-type-imports`                  | all TS            | `import type { … }` for type-only imports                                               |
| `no-floating-promises` (type-aware)        | the three UIs     | Await, return, `.catch(console.error)`, or `void` a promise that handles its own errors |
| `switch-exhaustiveness-check` (type-aware) | the three UIs     | Name every union member; don't rely on `default`                                        |
| `jsx-a11y` recommended                     | `.tsx` in the UIs | Labels tied to controls, keyboard-operable interactions, complete ARIA roles            |

`jsx-a11y/no-autofocus` is off: dialogs, the palette and rename fields move focus into
themselves on open (the WAI-ARIA dialog pattern). Type-aware rules use each app's
`tsconfig.json` through `projectService`, so a file outside every tsconfig's `include`
fails to lint — add it to one.

The React Compiler rules `react-hooks/refs`, `set-state-in-effect` and `static-components`
are **off** because the codebase does not use the React Compiler. Don't rely on
compiler auto-memoisation.

## Architecture rules

- **Rust owns Terminal layout state.** Components render the `LayoutSnapshot` /
  `HostLayout` that Rust emits (`wm:layout`, `wm:host-layout`) and call Tauri commands to
  change it. No local optimistic copies of the layout tree.
- **Data access lives in hooks** (`src/hooks/useX.ts`); non-React helpers live in
  `src/lib/`; command/keybinding logic in `src/commands/`. Components stay presentational
  where practical.
- **One bundle, many webviews.** Code that must run in only the chrome, overlay or popout
  webview checks which one it is in (see `main.tsx`), rather than assuming.
- Component styles live in a sibling `Component.css`; use the `wm-` BEM-style class
  prefix used throughout (`wm-panel-header__title`).

## Tauri IPC from React

- `invoke` promises must be handled: `.catch(console.error)` at minimum, or surface the
  error in UI. Don't add bare `.catch(() => {})` — say why if swallowing is intended.
- `listen()` returns a `Promise<UnlistenFn>`. Clean up in the effect's return, catching a
  rejected `listen()` so it can't become an unhandled rejection:

  ```ts
  useEffect(() => {
    const unlisten = listen<T>("wm:layout", (e) => setState(e.payload));
    return () => {
      unlisten.then((fn) => fn()).catch(console.error);
    };
  }, []);
  ```

- When combining an initial `invoke` snapshot with a subscription, the snapshot can
  resolve _after_ a newer event. Subscribe first, and ignore a snapshot that arrives after
  an event has already been applied.
- Type every `invoke<T>` / `listen<T>` with the shared types in `src/types.ts`; keep them
  in step with the Rust `serde` structs (field names are camelCase on the wire).

## Errors and telemetry

- Each UI entry point (`main.tsx`, `popout.tsx`) calls `startTelemetry()` (from
  `src/lib/telemetry.ts`) **first** and wraps its root in `TelemetryErrorBoundary`
  (`@one-terminal/telemetry/react`). A new entry point must do the same.
- Uncaught errors, unhandled rejections and `console.error` / `console.warn` reach the
  owning process's log file (and OTLP when configured), so `.catch(console.error)` stays
  the minimum for IPC promises — it is reported, not lost.
- For an explicit event, `telemetry.log(level, message, { attributes })`. Attribute keys
  are `a-z0-9_.`; never log secrets, tokens or FDC3 context payloads.

## Performance

The chrome webview re-renders on every layout emit, so render cost there is user-visible
during drag and resize.

- Key lists by stable identity — the webview `label`, never an array index.
- Keep per-frame work (drag, splitter moves) out of React state where possible; use refs
  and `requestAnimationFrame`, commit to state/IPC on release.
- Memoise (`useMemo`, `useCallback`, `memo`) where a profile shows a cost or where a
  callback is a dependency of a child effect — not by reflex.
- Entry chunk < 60 KB gzipped; vendor code split by `manualChunks` in each `vite.config.ts`.
  CI enforces this plus initial-JS/CSS ratchets (`npm run check:bundle-size`,
  `scripts/bundle-budgets.json`); React's vendor chunk (~58 KB) dominates first paint, so
  new dependencies in the entry path show up immediately.
  Lazy-load dialogs and settings panels that aren't needed at first paint.
- Apply persisted theme before first render (as `main.tsx` does) to avoid a flash.

## Accessibility

- Interactive elements are `<button>` / `<a>` / form controls, not clickable `<div>`s.
- Menus, the command palette and dialogs are fully keyboard-operable (arrow keys, Enter,
  Escape) and return focus to the trigger on close.
- Icon-only buttons get an `aria-label`.

## Tests

The Terminal has Vitest + React Testing Library tests (`npm run test:terminal`, also run
in CI). Tests sit next to the code as `<name>.test.ts(x)` and run in jsdom with Tauri IPC
mocked; nothing needs the Rust side.

- Put logic in plain functions under `src/lib/` or `src/commands/` and unit-test it
  directly; test components through what the user sees — query by role and accessible
  name, interact with `user-event`.
- Mock IPC with `mockIPC` from `@tauri-apps/api/mocks` and assert the exact
  `[command, args]` the UI sends.
- `src/test/setup.ts` resets the DOM, IPC mocks and `localStorage` after each test, and
  registers the jest-dom matchers on this package's Vitest (don't import
  `@testing-library/jest-dom/vitest` — it binds to the repo-root Vitest used by
  fdc3-plugin; types live in `src/test/jest-dom.d.ts`).
- Module singletons (the command `registry`, context-menu registry) must be cleaned up
  per test; module-level state needs `vi.resetModules()` + dynamic `import()`.

The [test-author](../../.ai/agents/test-author.md) role has the full patterns. The Desktop
Agent dashboard and App Directory UI have no tests yet — add the same setup there when
they gain logic worth testing.
