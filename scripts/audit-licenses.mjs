// SPDX-License-Identifier: MPL-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";

const allowed = new Set([
  "0BSD",
  "Apache-2.0",
  "BSD-1-Clause",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "BSL-1.0",
  "CC0-1.0",
  "ISC",
  "LGPL-2.1-or-later",
  "LGPL-3.0-or-later",
  "MIT",
  "MIT-0",
  "MPL-2.0",
  "OFL-1.1",
  "Unicode-3.0",
  "Unlicense",
  "Zlib",
]);

function assertAllowed(expression, name) {
  assert(expression, `${name} has no declared license`);
  const identifiers = expression.match(/[A-Za-z0-9][A-Za-z0-9.+-]*/g) ?? [];
  const licenses = identifiers.filter((value) => !["AND", "OR", "WITH", "LLVM-exception"].includes(value));
  const unknown = licenses.filter((value) => !allowed.has(value));
  assert.deepEqual(unknown, [], `${name} has unreviewed license expression: ${expression}`);
}

const cargo = spawnSync("cargo", ["metadata", "--format-version", "1", "--locked"], {
  encoding: "utf8",
  maxBuffer: 64 * 1024 * 1024,
  windowsHide: true,
});
assert.equal(cargo.status, 0, cargo.stderr);
const cargoPackages = JSON.parse(cargo.stdout).packages;
for (const pkg of cargoPackages) assertAllowed(pkg.license, `cargo:${pkg.name}@${pkg.version}`);

const npmLock = JSON.parse(await readFile(new URL("../desktop/package-lock.json", import.meta.url), "utf8"));
for (const [path, pkg] of Object.entries(npmLock.packages)) {
  assertAllowed(pkg.license, `npm:${path || "@verity/desktop"}`);
}

console.log(`Audited ${cargoPackages.length} Cargo packages and ${Object.keys(npmLock.packages).length} npm package entries.`);
