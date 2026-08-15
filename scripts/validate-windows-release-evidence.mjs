// SPDX-License-Identifier: MPL-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const path = new URL("../docs/qa/windows-x64-release-evidence.json", import.meta.url);
const evidence = JSON.parse(await readFile(path, "utf8"));

assert.equal(evidence.schema, "verity-windows-release-evidence.v1");
assert.equal(evidence.version, "0.1.0-beta.1");
for (const gate of ["trustedSignature", "installSmoke", "uninstallSmoke", "launchSmoke"]) {
  assert.equal(evidence[gate], true, `${gate} must pass`);
}
assert(Array.isArray(evidence.projects), "projects must be an array");
assert(evidence.projects.length >= 15, "at least 15 real-project results are required");
assert.equal(new Set(evidence.projects.map(({ id }) => id)).size, evidence.projects.length, "project IDs must be unique");
assert(evidence.projects.every(({ result }) => result === "passed"), "every real-project result must pass");

console.log(`Validated ${evidence.projects.length} Windows x64 real-project results.`);
