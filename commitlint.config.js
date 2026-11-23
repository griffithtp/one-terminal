// Conventional Commits, as documented in docs/conventions/general.md (Git and pull requests).
// Checked on every commit by lefthook's commit-msg hook.
export default {
  extends: ["@commitlint/config-conventional"],
  rules: {
    // Known scopes; an unknown one warns rather than blocks, so a new area can be added
    // here in the same commit that introduces it.
    "scope-enum": [
      1,
      "always",
      [
        "one-terminal",
        "desktop-agent",
        "fdc3",
        "appd",
        "ot-core",
        "scaffolder",
        "electron-host",
        "webview-host",
        "samples",
        "conventions",
        "deps",
        "ci",
        "ai",
        "security",
        "telemetry",
      ],
    ],
  },
};
