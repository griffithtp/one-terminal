# @one-terminal/fdc3-client

TypeScript FDC3 2.2 types and a Tauri-native Desktop Agent client for **host apps** (Tauri windows) within the OneTerminal framework.

## What it does

This package provides two things:

1. **TypeScript types** — FDC3 2.2 interfaces (`Context`, `Channel`, `AppIdentifier`, BMP bridge types, etc.) that mirror the Rust types in `packages/ot-core`. See [src/types.ts](src/types.ts).

2. **`Fdc3Agent`** — a Tauri-native FDC3 agent that talks to the Desktop Agent broker via Tauri IPC (`invoke` / `listen`). Requires `@tauri-apps/api`.

3. **`BridgeAgentProxy`** — wraps an `Fdc3Agent` with a WebSocket connection to the FDC3 2.2 Bridge backplane, enabling cross-agent context broadcasting and intent routing across multiple Desktop Agent instances.

## Rust backend requirement

`fdc3-client` calls Tauri IPC commands (`fdc3_register`, `fdc3_broadcast`, etc.) that must be registered by the **`ot-fdc3`** Rust plugin. Add it to every Tauri spoke app that uses this package:

```rust
// src-tauri/src/lib.rs
tauri::Builder::default()
    .plugin(ot_fdc3::init())
    // ...
```

See [packages/ot-fdc3/README.md](../ot-fdc3/README.md) for full setup instructions.

## Difference from `fdc3-plugin`

| | `fdc3-plugin` | `fdc3-client` |
|---|---|---|
| Target | Browser spoke apps (no Tauri) | Tauri spoke app frontends |
| Transport | WebSocket to FDC3 Bus (port 7891) | Tauri IPC → `ot-fdc3` plugin → TCP (port 7890) |
| Language | Vanilla JS, no build step | TypeScript, compiled |
| Dependencies | None | `@tauri-apps/api` |

## Usage

### Bootstrap in a Tauri app (`main.tsx` / `index.ts`)

```ts
import { initFdc3 } from '@one-terminal/fdc3-client';

// Call once before rendering. Connects to the local broker via Tauri IPC
// and optionally to the FDC3 2.2 bridge backplane.
const fdc3 = await initFdc3('my-app-id', {
  connectBridge: true,                          // default: true
  bridgeUrl: 'ws://127.0.0.1:4000/v2/bridge',  // default
});

// window.fdc3 is now set
```

`initFdc3` falls back silently to a plain `Fdc3Agent` if the bridge WebSocket is unavailable.

### Using `Fdc3Agent` directly

```ts
import { Fdc3Agent } from '@one-terminal/fdc3-client';
import { getCurrentWindow } from '@tauri-apps/api/window';

const agent = await Fdc3Agent.create('my-app', getCurrentWindow().label);

// Channels
const channels = await agent.getUserChannels();
await agent.joinUserChannel(channels[0].id);

// Broadcast context
await agent.broadcast({ type: 'fdc3.instrument', id: { ticker: 'AAPL' } });

// Listen for context
const listener = agent.addContextListener('fdc3.instrument', (ctx, meta) => {
  console.log('received from', meta?.source?.appId, '→', ctx.id.ticker);
});

// Intents
const resolution = await agent.raiseIntent('ViewChart', {
  type: 'fdc3.instrument',
  id: { ticker: 'AAPL' },
});

listener.unsubscribe();
```

### Using `BridgeAgentProxy` for cross-agent routing

```ts
import { Fdc3Agent, BridgeAgentProxy } from '@one-terminal/fdc3-client';
import { getCurrentWindow } from '@tauri-apps/api/window';

const inner = await Fdc3Agent.create('my-app', getCurrentWindow().label);
const proxy = await BridgeAgentProxy.connect(inner, 'ws://127.0.0.1:4000/v2/bridge');

window.fdc3 = proxy;
console.log('connected to bridge as', proxy.desktopAgentId);

// Raise an intent targeting an app on a different Desktop Agent
await proxy.raiseIntent('ViewChart', context, {
  appId: 'chart-app',
  desktopAgent: 'remote-agent-id',
});
```

## Where it is used

| Location | Usage |
|---|---|
| `apps/desktop-agent/src/hooks/useFdcBus.ts` | CDA-side bridge hook referencing FDC3 bus types |

## Exports

```ts
// Types
export * from './types';  // Context, Channel, AppIdentifier, BmpMessage, …

// Classes
export { Fdc3Agent };        // Tauri IPC agent
export { BridgeAgentProxy }; // Bridge-aware wrapper

// Bootstrap helper
export { initFdc3 };         // convenience one-liner for app entry points
export type { InitFdc3Options };
```
