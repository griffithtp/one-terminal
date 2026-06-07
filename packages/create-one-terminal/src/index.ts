#!/usr/bin/env node
import { runCreate } from "./create/index.js";
import { runUpgrade } from "./upgrade/index.js";
import { runAddWidget } from "./add-widget/index.js";

const cmd = process.argv[2];

if (cmd === "upgrade") {
  await runUpgrade();
} else if (cmd === "add-widget") {
  await runAddWidget();
} else {
  await runCreate();
}
