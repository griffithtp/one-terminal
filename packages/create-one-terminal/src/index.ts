#!/usr/bin/env node
import { runCreate } from "./create/index.js";
import { runUpgrade } from "./upgrade/index.js";
import { runNewWidget } from "./new-widget/index.js";

const cmd = process.argv[2];

if (cmd === "upgrade") {
  await runUpgrade();
} else if (cmd === "new-widget") {
  await runNewWidget();
} else {
  await runCreate();
}
