#!/usr/bin/env node
import { runCreate } from "./create/index.js";
import { runUpgrade } from "./upgrade/index.js";

const cmd = process.argv[2];

if (cmd === "upgrade") {
  await runUpgrade();
} else {
  await runCreate();
}
