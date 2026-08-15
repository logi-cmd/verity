// SPDX-License-Identifier: MPL-2.0

import assert from "node:assert/strict";
import { readFile, stat } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const required = [
  "site/index.html",
  "site/404.html",
  "site/zh/index.html",
  "site/download/index.html",
  "site/zh/download/index.html",
  "site/privacy/index.html",
  "site/zh/privacy/index.html",
  "site/terms/index.html",
  "site/zh/terms/index.html",
  "site/robots.txt",
  "site/sitemap.xml",
];

for (const path of required) {
  assert((await stat(join(root, path))).isFile(), `${path} must exist`);
}

const files = await Promise.all(required.map((path) => readFile(join(root, path), "utf8")));
const joined = files.join("\n");
assert(!/github\.com\/logi-cmd\/agent-guardrails-[a-z]+/i.test(joined), "legacy private repository URL found");
assert(!/href=["'][^"']*\/(pricing|access|refund|pro)(?:\/|["'])/i.test(joined), "legacy commercial route found");
assert(joined.includes("https://github.com/logi-cmd/verity"), "public Verity repository URL missing");
assert(!/<a[^>]+(?:\.msi|\.exe|\.dmg|\.appimage)/i.test(joined), "installer link is not release-gated");

console.log(`Validated ${required.length} public site files.`);
