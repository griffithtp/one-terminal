# Sample Widget

Minimal FDC3 2.2 widget bundled with Standalone-variant scaffolds. Demonstrates:

- Connecting to an FDC3 agent (the bundled Desktop Agent in Enterprise, an external agent in Standalone).
- Subscribing to `fdc3.instrument` context broadcasts.
- Sending an `fdc3.instrument` broadcast from a simple input.

## Run

```sh
npm run dev --workspace apps/sample-widget   # http://localhost:3012
```

## Add to the Terminal

Register the widget so it appears in the Terminal's launcher.

**Standalone** — add to `widgets.config.json`:

```jsonc
{
  "widgets": [
    {
      "appId": "sample-widget",
      "title": "Sample Widget",
      "url": "http://localhost:3012",
    },
  ],
}
```

**Enterprise** — POST to App Directory:

```sh
curl -X POST http://localhost:3005/v2/apps \
  -H 'Content-Type: application/json' \
  -d '{"appId":"sample-widget","name":"sample-widget","title":"Sample Widget","type":"web","details":{"url":"http://localhost:3012"}}'
```

## Customize the widget header

The Terminal's `panelHeaders.tsx` registry lets you render a custom title bar
for this widget. See [CLAUDE.md](../../CLAUDE.md#widget-ui-extension-points)
for the registry API.
