<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="ot-fdc3-app-id" content="<%= widgetName %>" />
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
      // The plugin connects once per page. Using its shared connection lets the
      // Terminal's "Set channel" menu control this widget's channel.
      import { getAgent } from "/fdc3-plugin.js";
      const statusEl = document.getElementById("status");
      try {
        const fdc3 = await getAgent();
        const { appMetadata } = await fdc3.getInfo();
        statusEl.textContent = `Connected · ${appMetadata.instanceId.slice(0, 8)}`;
        statusEl.className = "ok";
      } catch (e) {
        statusEl.textContent = "FDC3 agent unavailable";
        statusEl.className = "err";
      }
    </script>
  </body>
</html>
