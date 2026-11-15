# React / TypeScript conventions

Applies to `apps/one-terminal/src`, `apps/desktop-agent/src` and `apps/app-directory/ui`.
`packages/fdc3-plugin` and the sample apps are excluded from root lint/format.

## Tooling (enforced)

| Check  | Command             | Notes                                                                                                   |
| ------ | ------------------- | ------------------------------------------------------------------------------------------------------- |
| Format | `npm run format`    | Prettier: 2 spaces, double quotes, semicolons, `trailingComma: es5`, width 100                          |
| Lint   | `npm run lint`      | ESLint flat config (`eslint.config.js`): typescript-eslint recommended, react + react-hooks recommended |
| Types  | `npm run typecheck` | `strict`, `noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`                          |

Unused variables are errors unless prefixed with `_`. `any` is a warning — treat it as an
error in new code; use `unknown` and narrow.

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
- `listen()` returns a `Promise<UnlistenFn>`. Clean up in the effect's return:

  ```ts
  useEffect(() => {
    const unlisten = listen<T>("wm:layout", (e) => setState(e.payload));
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);
  ```

- When combining an initial `invoke` snapshot with a subscription, the snapshot can
  resolve _after_ a newer event. Subscribe first, and ignore a snapshot that arrives after
  an event has already been applied.
- Type every `invoke<T>` / `listen<T>` with the shared types in `src/types.ts`; keep them
  in step with the Rust `serde` structs (field names are camelCase on the wire).

## Performance

The chrome webview re-renders on every layout emit, so render cost there is user-visible
during drag and resize.

- Key lists by stable identity — the webview `label`, never an array index.
- Keep per-frame work (drag, splitter moves) out of React state where possible; use refs
  and `requestAnimationFrame`, commit to state/IPC on release.
- Memoise (`useMemo`, `useCallback`, `memo`) where a profile shows a cost or where a
  callback is a dependency of a child effect — not by reflex.
- Entry chunk < 60 KB gzipped; vendor code split by `manualChunks` in each `vite.config.ts`.
  Lazy-load dialogs and settings panels that aren't needed at first paint.
- Apply persisted theme before first render (as `main.tsx` does) to avoid a flash.

## Accessibility

- Interactive elements are `<button>` / `<a>` / form controls, not clickable `<div>`s.
- Menus, the command palette and dialogs are fully keyboard-operable (arrow keys, Enter,
  Escape) and return focus to the trigger on close.
- Icon-only buttons get an `aria-label`.

## Tests

There are currently no frontend unit tests. Vitest + React Testing Library are planned;
until then, put logic that needs testing in plain functions under `src/lib/` or
`src/commands/` so it can be unit tested without rendering.
