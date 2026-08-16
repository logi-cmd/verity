// SPDX-License-Identifier: MPL-2.0

import assert from "node:assert/strict";
import { readFile, stat } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const siteRoot = join(root, "site");
const pages = [
  ["site/index.html", "https://agent-guardrails.com/", "en", "https://agent-guardrails.com/zh/"],
  ["site/zh/index.html", "https://agent-guardrails.com/zh/", "zh-CN", "https://agent-guardrails.com/"],
  ["site/download/index.html", "https://agent-guardrails.com/download/", "en", "https://agent-guardrails.com/zh/download/"],
  ["site/zh/download/index.html", "https://agent-guardrails.com/zh/download/", "zh-CN", "https://agent-guardrails.com/download/"],
  ["site/how-it-works/index.html", "https://agent-guardrails.com/how-it-works/", "en", "https://agent-guardrails.com/zh/how-it-works/"],
  ["site/zh/how-it-works/index.html", "https://agent-guardrails.com/zh/how-it-works/", "zh-CN", "https://agent-guardrails.com/how-it-works/"],
  ["site/verification-receipts/index.html", "https://agent-guardrails.com/verification-receipts/", "en", "https://agent-guardrails.com/zh/verification-receipts/"],
  ["site/zh/verification-receipts/index.html", "https://agent-guardrails.com/zh/verification-receipts/", "zh-CN", "https://agent-guardrails.com/verification-receipts/"],
  ["site/supported-stacks/index.html", "https://agent-guardrails.com/supported-stacks/", "en", "https://agent-guardrails.com/zh/supported-stacks/"],
  ["site/zh/supported-stacks/index.html", "https://agent-guardrails.com/zh/supported-stacks/", "zh-CN", "https://agent-guardrails.com/supported-stacks/"],
  ["site/privacy/index.html", "https://agent-guardrails.com/privacy/", "en", "https://agent-guardrails.com/zh/privacy/"],
  ["site/zh/privacy/index.html", "https://agent-guardrails.com/zh/privacy/", "zh-CN", "https://agent-guardrails.com/privacy/"],
  ["site/terms/index.html", "https://agent-guardrails.com/terms/", "en", "https://agent-guardrails.com/zh/terms/"],
  ["site/zh/terms/index.html", "https://agent-guardrails.com/zh/terms/", "zh-CN", "https://agent-guardrails.com/terms/"],
];
const supportFiles = [
  "site/404.html",
  "site/styles.css",
  "site/site.js",
  "site/site.webmanifest",
  "site/robots.txt",
  "site/sitemap.xml",
  "site/llms.txt",
  "site/75f8ffba8b104196a7add3d2d84c5eb2.txt",
  "site/assets/geist-latin-wght-normal.woff2",
  "site/assets/geist-mono-latin-wght-normal.woff2",
  "site/assets/verity-desktop.png",
  "site/assets/verity-desktop.webp",
  "site/assets/verity-social-preview.png",
];

const escapeRegExp = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

for (const path of [...pages.map(([path]) => path), ...supportFiles]) {
  assert((await stat(join(root, path))).isFile(), `${path} must exist`);
}

const pageContents = [];
for (const [path, canonical, language, alternate] of pages) {
  const html = await readFile(join(root, path), "utf8");
  pageContents.push(html);
  assert.match(html, new RegExp(`<html lang=["']${language}["']`), `${path} must declare ${language}`);
  assert.match(
    html,
    new RegExp(`<link\\s+rel=["']canonical["']\\s+href=["']${escapeRegExp(canonical)}["']\\s*/?>`),
    `${path} canonical mismatch`,
  );
  assert(html.includes(`hreflang="en"`), `${path} English alternate missing`);
  assert(html.includes(`hreflang="zh-CN"`), `${path} Chinese alternate missing`);
  assert(html.includes(`hreflang="x-default"`), `${path} x-default alternate missing`);
  assert(html.includes(`href="${alternate}"`), `${path} reciprocal alternate missing`);
  assert(html.includes("<title>"), `${path} missing title`);
  for (const [attribute, value] of [
    ["name", "description"],
    ["property", "og:title"],
    ["property", "og:description"],
    ["property", "og:url"],
    ["property", "og:image"],
    ["name", "twitter:card"],
    ["name", "twitter:title"],
    ["name", "twitter:description"],
    ["name", "twitter:image"],
  ]) {
    assert.match(
      html,
      new RegExp(`<meta\\s+[^>]*${attribute}=["']${escapeRegExp(value)}["'][^>]*>`),
      `${path} missing ${value}`,
    );
  }
  assert(!/[—–]/u.test(html), `${path} contains a visible em or en dash`);
}

const joined = pageContents.join("\n");
assert(joined.includes("https://github.com/logi-cmd/verity"), "public Verity repository URL missing");
assert(joined.includes("v0.1.0-beta.2"), "current source beta missing");
assert(!joined.includes("v0.1.0-beta.1"), "retired source beta remains in site");
assert(!/github\.com\/logi-cmd\/agent-guardrails-[a-z]+/i.test(joined), "legacy private repository URL found");
assert(!/href=["'][^"']*\/(pricing|access|refund|pro)(?:\/|["'])/i.test(joined), "legacy commercial route found");
assert(!/<a[^>]+(?:\.msi|\.exe|\.dmg|\.appimage)/i.test(joined), "installer link is not release-gated");
assert(!/future team|control plane|paddle|entitlement|paid-value|internal discussion/i.test(joined), "internal or commercial direction found");
assert(!/(?:[A-Z]:\\Users\\|[A-Z]:\\verity|\\\\[^\\]+\\)/i.test(joined), "private local path found");
assert(joined.includes('"@type":"WebSite"'), "WebSite structured data missing");
assert(joined.includes('"@type":"SoftwareSourceCode"'), "SoftwareSourceCode structured data missing");
assert(!joined.includes('"@type":"SoftwareApplication"'), "installer-only SoftwareApplication schema must not ship");

const notFound = await readFile(join(siteRoot, "404.html"), "utf8");
assert(notFound.includes('name="robots" content="noindex"'), "404 must be noindex");
assert(!/[—–]/u.test(notFound), "404 contains a visible em or en dash");

const robots = await readFile(join(siteRoot, "robots.txt"), "utf8");
assert.match(robots, /User-agent: OAI-SearchBot[\s\S]*Allow: \//, "OAI-SearchBot must be allowed");
assert(robots.includes("https://agent-guardrails.com/sitemap.xml"), "robots sitemap missing");

const sitemap = await readFile(join(siteRoot, "sitemap.xml"), "utf8");
for (const [, canonical] of pages) assert(sitemap.includes(`<loc>${canonical}</loc>`), `sitemap missing ${canonical}`);
assert.equal((sitemap.match(/<loc>/g) || []).length, pages.length, "sitemap contains unexpected canonical URLs");
assert.equal((sitemap.match(/<lastmod>2026-08-16<\/lastmod>/g) || []).length, pages.length, "sitemap lastmod coverage mismatch");

const screenshot = await readFile(join(siteRoot, "assets/verity-desktop.png"));
assert.equal(screenshot.readUInt32BE(16), 1280, "desktop screenshot width must be 1280");
assert.equal(screenshot.readUInt32BE(20), 640, "desktop screenshot height must be 640");
for (const asset of ["verity-desktop.png", "verity-desktop.webp", "verity-social-preview.png"]) {
  const info = await stat(join(siteRoot, "assets", asset));
  assert(info.size < 1_000_000, `${asset} must stay below 1 MB`);
}

const manifest = JSON.parse(await readFile(join(siteRoot, "site.webmanifest"), "utf8"));
assert.equal(manifest.name, "Verity");
assert.equal(manifest.theme_color, "#09090d");

console.log(`Validated ${pages.length} canonical pages and ${supportFiles.length} support files.`);
