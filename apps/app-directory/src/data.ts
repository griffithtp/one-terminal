import type { AppD } from "./types.js";

/**
 * Mutable in-memory app catalogue.
 * All entries are FDC3 2.2-compliant AppD records.
 */
let _apps: AppD[] = [
  // ── Ticker-Plant ────────────────────────────────────────────────────────────
  {
    appId: "ticker-plant",
    name: "Ticker-Plant",
    type: "web",
    details: { url: "http://localhost:3010" },

    title: "Market Data Ticker Plant",
    description:
      "High-throughput market data distribution service for FX spot rates. " +
      "Ingests raw tick data from upstream liquidity providers and normalises " +
      "it into FDC3-compliant fx.rate and fdc3.instrument context objects " +
      "broadcast on the Green user channel.",
    version: "2.1.0",
    tooltip: "Launch the Ticker-Plant market data feed",
    lang: "en-US",
    publisher: "OneTerminal",
    contactEmail: "support@one-terminal.local",
    supportEmail: "support@one-terminal.local",
    moreInfo: "https://docs.one-terminal.local/ticker-plant",

    categories: ["Market Data", "Data Feed", "FX"],

    icons: [
      {
        src: "http://localhost:3005/static/icons/ticker-plant.svg",
        size: "32x32",
        type: "image/svg+xml",
      },
      {
        src: "http://localhost:3005/static/icons/ticker-plant-128.png",
        size: "128x128",
        type: "image/png",
      },
    ],

    screenshots: [
      {
        src: "http://localhost:3005/static/screenshots/ticker-plant-rates.png",
        size: "1280x720",
        type: "image/png",
        label: "Live EUR/USD, GBP/USD and USD/JPY spot rates",
      },
      {
        src: "http://localhost:3005/static/screenshots/ticker-plant-settings.png",
        size: "1280x720",
        type: "image/png",
        label: "Feed configuration — symbols, precision, throttle",
      },
    ],

    interop: {
      intents: {
        // Ticker-Plant raises ViewChart when the user clicks a symbol row,
        // passing the selected instrument as context.
        raises: {
          ViewChart: ["fdc3.instrument"],
          ViewQuote: ["fdc3.instrument"],
        },
      },
      userChannels: {
        // Continuously broadcasts normalised tick data on the Green channel.
        broadcasts: ["fdc3.instrument", "fx.rate"],
        listensFor: [],
      },
    },

    hostManifests: {
      OpenFin: {
        config: {
          autoShow: true,
          defaultWidth: 900,
          defaultHeight: 560,
          minWidth: 600,
          resizable: true,
        },
      },
    },

    customConfig: {
      symbols: ["EUR/USD", "GBP/USD", "USD/JPY", "AUD/USD", "USD/CHF"],
      tickIntervalMs: 250,
      precisionOverride: { "USD/JPY": 3 },
    },

    engineBindings: {
      windows: [
        { family: "webview2", version: "system" },
      ],
      macos: [
        { family: "wkwebview", version: "system" },
      ],
    },
  },

  // ── Chart-Viewer ────────────────────────────────────────────────────────────
  {
    appId: "chart-viewer",
    name: "Chart-Viewer",
    type: "web",
    details: { url: "http://localhost:3011" },

    title: "FX Candlestick Chart Viewer",
    description:
      "Interactive candlestick charting application for FX pairs. " +
      "Responds to ViewChart intents and fdc3.instrument / fx.rate context " +
      "broadcasts to update the displayed symbol and timeframe in real time. " +
      "Supports 1-minute through weekly candles with technical indicator overlays.",
    version: "3.0.2",
    tooltip: "Open the charting workspace",
    lang: "en-US",
    publisher: "OneTerminal",
    contactEmail: "support@one-terminal.local",
    supportEmail: "support@one-terminal.local",
    moreInfo: "https://docs.one-terminal.local/chart-viewer",

    categories: ["Analytics", "Charting", "FX", "Trading"],

    icons: [
      {
        src: "http://localhost:3005/static/icons/chart-viewer.svg",
        size: "32x32",
        type: "image/svg+xml",
      },
      {
        src: "http://localhost:3005/static/icons/chart-viewer-128.png",
        size: "128x128",
        type: "image/png",
      },
    ],

    screenshots: [
      {
        src: "http://localhost:3005/static/screenshots/chart-viewer-eurusd.png",
        size: "1280x800",
        type: "image/png",
        label: "EUR/USD 1-hour candlestick chart with Bollinger Bands",
      },
      {
        src: "http://localhost:3005/static/screenshots/chart-viewer-multi.png",
        size: "1280x800",
        type: "image/png",
        label: "Multi-pane layout — price, volume, RSI(14)",
      },
    ],

    interop: {
      intents: {
        // Primary entry-point: any app can raise ViewChart with an instrument.
        listensFor: {
          ViewChart: {
            displayName: "View Chart",
            contexts: ["fdc3.instrument"],
            resultType: "fdc3.chart",
          },
          ViewQuote: {
            displayName: "View Quote",
            contexts: ["fdc3.instrument"],
          },
        },
      },
      userChannels: {
        broadcasts: [],
        // Reacts to instrument and live rate ticks pushed by Ticker-Plant.
        listensFor: ["fdc3.instrument", "fx.rate"],
      },
    },

    hostManifests: {
      OpenFin: {
        config: {
          autoShow: true,
          defaultWidth: 1100,
          defaultHeight: 700,
          minWidth: 800,
          resizable: true,
          saveWindowState: true,
        },
      },
    },

    customConfig: {
      defaultSymbol: "EUR/USD",
      defaultTimeframe: "1H",
      availableTimeframes: ["1M", "5M", "15M", "30M", "1H", "4H", "1D", "1W"],
      indicators: {
        default: ["EMA(20)", "EMA(50)", "Volume"],
        available: ["SMA", "EMA", "VWAP", "Bollinger", "RSI", "MACD", "ATR"],
      },
    },

    engineBindings: {
      windows: [
        { family: "webview2", version: "system" },
        { family: "webview2", version: "124.0.2478.97" },
        { family: "electron", version: "system" },
      ],
      macos: [
        { family: "wkwebview", version: "system" },
        { family: "electron", version: "system" },
      ],
    },
  },
];

export function getApps(): AppD[] {
  return _apps;
}
export function setApps(apps: AppD[]): void {
  _apps = apps;
}
