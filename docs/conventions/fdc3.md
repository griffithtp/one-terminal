# FDC3 conventions

OneTerminal implements an **FDC3 3.0 Desktop Agent**. Widgets use the official
`@finos/fdc3-agent-proxy` (bundled in `packages/fdc3-plugin`) over a WebSocket speaking
the Desktop Agent Communication Protocol (DACP). User-facing setup:
[docs/fdc3.md](../fdc3.md). Spec: <https://fdc3.finos.org/docs/api/spec>.

Authoritative definitions are installed locally (FDC3 3.0.0) — prefer them to memory:

- DACP/WCP message schemas: `node_modules/@finos/fdc3-schema/dist/schemas/api/<type>.schema.json`
- Context schemas: `node_modules/@finos/fdc3-context/dist/schemas/context/`
- API types and error enums: `node_modules/@finos/fdc3-standard/dist/src/api/` (`Errors.d.ts`, …)

The contract tests validate every message against these schemas
(`packages/fdc3-plugin/test/schemas.ts`).

## Where code goes

| Concern                                     | Location                                                                            |
| ------------------------------------------- | ----------------------------------------------------------------------------------- |
| FDC3 rules: channels, intents, apps, events | `apps/desktop-agent/src-tauri/src/broker/` (`channels.rs`, `intents.rs`, `apps.rs`) |
| DACP/WCP message types                      | `broker/types.rs`                                                                   |
| Socket, handshake, performing `Effect`s     | `apps/desktop-agent/src-tauri/src/fdc3_bus/`                                        |
| Browser client                              | `packages/fdc3-plugin/src/`                                                         |
| Contract tests                              | `packages/fdc3-plugin/test/contract.test.ts`                                        |

The broker is synchronous and transport-free; it must never import sockets or Tauri.
I/O is requested by returning an `Effect` (`Launch`, `ExpireAfter`, `CloseWindow`).

## Protocol rules

1. **Handshake.** The first frame is `WCP4ValidateAppIdentity`; the bus replies
   `WCP5ValidateAppIdentityResponse` with the broker-assigned `instanceId`. `WCP6Goodbye`
   ends the session. WCP1–3 (postMessage discovery) don't apply to a direct socket.
2. **Correlation.** Every response echoes the request's `meta.requestUuid` and carries a
   fresh `responseUuid`; events carry an `eventUuid`. `meta.source` and timestamps are set
   by the agent from the connection's identity — never copied from the client's payload.
3. **Ordering.** The proxy registers a listener only after its response arrives, so a
   response must be queued before any event targeting the new listener. Preserve this
   whenever you add a listener type.
4. **Errors.** `payload.error` carries a spec error value from the enum that applies to
   the operation (`ResolveError`, `OpenError`, `ChannelError`, `ResultError`, …) — e.g.
   `NoAppsFound`, `AccessDenied`, `NoChannelFound`, `MalformedContext`. Don't invent new
   values; if none fits, note it in the PR.
5. **Contexts.** A context must have a string `type`. Unknown fields pass through
   unchanged; the broker never strips or rewrites context content.
6. **Channels.**
   - User channels are seeded from `AgentConfig.user_channels`; `ChannelManager::new()`
     starts empty.
   - Each channel keeps a context history; `getCurrentContext` answers with the newest
     entry, or the newest of the requested `contextType`. Context and metadata are both
     set or both null.
   - Joining or leaving sends `channelChangedEvent` to the instance's
     `USER_CHANNEL_CHANGED` listeners and emits `BrokerEvent::ChannelChanged` for the
     Desktop Agent UI. Delivering the current context to listeners after a join is the
     proxy's job (it calls `getCurrentContext`) — don't duplicate it in the broker.
   - Channel membership must be consistent across the Desktop Agent and the Terminal's
     channel selector UI (regression fixed in #68).
   - Private channels: send disconnect events to the peer and clean up listeners when
     either side disconnects or the instance goes away.
7. **Intents.** Resolution considers the App Directory (`interop.intents.listensFor`,
   result types, `fdc3Version` range) and live instances. Ambiguity goes to the intent
   resolver UI (`fdc3-plugin/src/resolver.ts`). Launch timeouts: `APP_LAUNCH_TIMEOUT`
   (20 s) in the broker must stay shorter than the plugin's `appLaunchTimeout`.
8. **Disconnects.** On disconnect, remove the instance's listeners, private-channel
   membership and pending requests, and fail anything waiting on it with the spec error.
9. **Bridging.** Agent Bridging is **not implemented**. Don't fake cross-agent behaviour;
   return the spec response for "no bridge" and flag the feature as needing bridging.

## Identity and trust

The `appId` is self-asserted (query string) and the bus does not check `Origin` — see
[security.md](security.md#fdc3-bus). Don't add behaviour that grants privileges based on
`appId` until that is fixed.

## Testing a change

```sh
cargo test -p desktop-agent broker     # broker unit tests (broker/tests.rs)
npm run test:fdc3-plugin               # official proxy against the real Rust bus
npm run build:fdc3-plugin              # if packages/fdc3-plugin/src changed
```

- Every new or changed broker rule gets a unit test that drives `Broker::handle` with DACP
  JSON and asserts on the queued frames and returned `Effect`s.
- Behaviour visible through the official proxy also gets a contract test.
- For larger changes, consider running the FINOS FDC3 conformance suite
  (<https://github.com/finos/FDC3-conformance-framework>) against a running Desktop Agent.

## Review checklist

- [ ] No socket/Tauri imports in `broker/`
- [ ] Response correlation and response-before-event ordering preserved
- [ ] Only spec error values returned
- [ ] `source` derived from the connection, not the payload
- [ ] Listeners and private channels cleaned up on unsubscribe and disconnect
- [ ] Unit test + contract test added; bundle regenerated if `src/` changed
- [ ] Spec section cited in the PR for any behaviour change
