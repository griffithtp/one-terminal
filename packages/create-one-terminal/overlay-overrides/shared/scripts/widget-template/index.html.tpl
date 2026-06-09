<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <title><%= widgetTitle %></title>
    <style>
      body {
        background: #0d1117;
        color: #e6edf3;
        font-family: ui-monospace, monospace;
        font-size: 13px;
        padding: 20px;
        margin: 0;
      }
      h1 { font-size: 15px; margin: 0 0 12px; }
      #status { color: #8b949e; font-size: 11px; }
      #status.ok { color: #3fb950; }
      #status.err { color: #f85149; }
    </style>
  </head>
  <body>
    <h1><%= widgetTitle %></h1>
    <div id="status">Connecting…</div>

    <script type="module">
      import { DesktopAgentClient } from "/fdc3-plugin.js";
      const statusEl = document.getElementById("status");
      try {
        const fdc3 = await DesktopAgentClient.connect("<%= widgetName %>");
        statusEl.textContent = `Connected · ${fdc3.getIdentity().instanceId.slice(0, 8)}`;
        statusEl.className = "ok";
      } catch (e) {
        statusEl.textContent = "FDC3 agent unavailable";
        statusEl.className = "err";
      }
    </script>
  </body>
</html>
