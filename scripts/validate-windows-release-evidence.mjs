// SPDX-License-Identifier: MPL-2.0
import { readFile } from "node:fs/promises";
import { validateReleaseEvidence } from "./windows-release-acceptance-lib.mjs";

const evidencePath = new URL("../docs/qa/windows-x64-release-evidence.json", import.meta.url);
const manifestPath = new URL("../docs/qa/windows-x64-real-projects.json", import.meta.url);
const packagePath = new URL("../package.json", import.meta.url);
const evidence = JSON.parse(await readFile(evidencePath, "utf8"));
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const packageManifest = JSON.parse(await readFile(packagePath, "utf8"));

validateReleaseEvidence(evidence, packageManifest.version, manifest);
console.log(`Validated ${evidence.artifacts.length} signed installers and ${evidence.projects.length} Windows x64 real projects.`);
