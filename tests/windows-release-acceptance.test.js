// SPDX-License-Identifier: MPL-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  selectAcceptanceTarget,
  validateProjectManifest,
  validateReleaseEvidence,
} from "../scripts/windows-release-acceptance-lib.mjs";

const readJson = async (path) => JSON.parse(await readFile(new URL(`../${path}`, import.meta.url), "utf8"));

test("Windows acceptance manifest pins 15 unique real repositories", async () => {
  const manifest = validateProjectManifest(await readJson("docs/qa/windows-x64-real-projects.json"));
  assert.equal(manifest.projects.length, 15);
  assert.deepEqual(new Set(manifest.projects.map(({ stack }) => stack)), new Set(["node", "go", "rust"]));
});

test("Windows acceptance rejects floating revisions and duplicate repositories", async () => {
  const manifest = await readJson("docs/qa/windows-x64-real-projects.json");
  manifest.projects[0].revision = "main";
  assert.throws(() => validateProjectManifest(manifest), /full commit SHA/);

  const duplicate = await readJson("docs/qa/windows-x64-real-projects.json");
  duplicate.projects[1].repository = duplicate.projects[0].repository;
  assert.throws(() => validateProjectManifest(duplicate), /duplicate repository/);
});

test("acceptance target selection requires one complete machine oracle", () => {
  const project = { id: "sample", stack: "node", relativeRoot: "" };
  const target = { id: "node-1", stack: "node", relative_root: "", plan_status: "complete", oracle_status: "machine" };
  assert.equal(selectAcceptanceTarget({ targets: [target] }, project), target);
  assert.throws(() => selectAcceptanceTarget({ targets: [{ ...target, oracle_status: "limited" }] }, project), /exactly one/);
});

test("release evidence requires both signed installer lifecycles and the complete pinned matrix", async () => {
  const manifest = await readJson("docs/qa/windows-x64-real-projects.json");
  const artifact = (kind) => ({
    kind,
    name: `Verity.${kind === "msi" ? "msi" : "exe"}`,
    sha256: "a".repeat(64),
    signerSubject: "CN=SignPath Foundation",
    timestampSubject: "CN=Trusted Timestamp Authority",
    trustedSignature: true,
    installSmoke: true,
    launchSmoke: true,
    uninstallSmoke: true,
  });
  const evidence = {
    schema: "verity-windows-release-evidence.v1",
    version: "0.1.0-beta.2",
    platform: "windows-x64",
    sourceCommit: "b".repeat(40),
    generatedAt: "2026-08-19T00:00:00.000Z",
    runner: {
      name: "verity.exe",
      version: "0.1.0-beta.2",
      sha256: "e".repeat(64),
    },
    artifacts: [artifact("msi"), artifact("nsis")],
    trustedSignature: true,
    installSmoke: true,
    launchSmoke: true,
    uninstallSmoke: true,
    projects: manifest.projects.map((project, index) => ({
      id: project.id,
      repository: project.repository,
      revision: project.revision,
      stack: project.stack,
      targetId: `target-${index}`,
      receiptSha256: "d".repeat(64),
      result: "passed",
    })),
  };
  assert.equal(validateReleaseEvidence(evidence, evidence.version, manifest), evidence);
  evidence.artifacts[1].trustedSignature = false;
  assert.throws(() => validateReleaseEvidence(evidence, evidence.version, manifest), /nsis.trustedSignature/);

  evidence.artifacts[1].trustedSignature = true;
  evidence.projects[0].revision = "f".repeat(40);
  assert.throws(() => validateReleaseEvidence(evidence, evidence.version, manifest), /revision must match/);
});

test("Windows smoke uses trusted Authenticode and full install lifecycles", async () => {
  const smoke = await readFile(new URL("../scripts/windows-installer-smoke.ps1", import.meta.url), "utf8");
  assert.match(smoke, /signtool\.exe/);
  assert.match(smoke, /verify \/pa \/all/);
  assert.match(smoke, /Get-AuthenticodeSignature/);
  assert.match(smoke, /TimeStamperCertificate/);
  assert.match(smoke, /msiexec\.exe/);
  assert.match(smoke, /MainWindowHandle/);
  assert.match(smoke, /uninstall\.exe/);
});

test("Windows runner exposes a pinned-project preflight without executing checks", async () => {
  const runner = await readFile(new URL("../scripts/run-windows-release-acceptance.mjs", import.meta.url), "utf8");
  assert.match(runner, /--preflight-projects/);
  assert.match(runner, /preflightProject/);
  assert.doesNotMatch(runner.match(/async function preflightProject[\s\S]*?\n}/)[0], /\["check"/);
});
