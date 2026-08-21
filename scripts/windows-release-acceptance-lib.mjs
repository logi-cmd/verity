// SPDX-License-Identifier: MPL-2.0
import assert from "node:assert/strict";

export const PROJECT_MANIFEST_SCHEMA = "verity-windows-real-project-manifest.v1";
export const RELEASE_EVIDENCE_SCHEMA = "verity-windows-release-evidence.v1";

const SHA40 = /^[0-9a-f]{40}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const SUPPORTED_STACKS = new Set(["node", "go", "rust"]);

export function validateProjectManifest(manifest) {
  assert.equal(manifest.schema, PROJECT_MANIFEST_SCHEMA);
  assert(Array.isArray(manifest.projects), "projects must be an array");
  assert(manifest.projects.length >= 15, "at least 15 real projects are required");

  const ids = new Set();
  const repositories = new Set();
  for (const project of manifest.projects) {
    assert.match(project.id, /^[a-z0-9]+(?:-[a-z0-9]+)*$/, "project id must be stable kebab-case");
    assert(!ids.has(project.id), `duplicate project id: ${project.id}`);
    ids.add(project.id);

    assert.match(project.repository, /^https:\/\/github\.com\/[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/);
    assert(!repositories.has(project.repository), `duplicate repository: ${project.repository}`);
    repositories.add(project.repository);

    assert.match(project.revision, SHA40, `${project.id} must pin a full commit SHA`);
    assert(SUPPORTED_STACKS.has(project.stack), `${project.id} uses an unsupported acceptance stack`);
    assert.equal(typeof project.relativeRoot, "string");
  }
  return manifest;
}

export function selectAcceptanceTarget(plan, project) {
  assert(Array.isArray(plan.targets), `${project.id} inspect output must contain targets`);
  const matches = plan.targets.filter((target) =>
    target.stack === project.stack
    && target.relative_root === project.relativeRoot
    && target.plan_status === "complete"
    && target.oracle_status === "machine"
  );
  assert.equal(matches.length, 1, `${project.id} must expose exactly one complete machine-verifiable target`);
  return matches[0];
}

export function validateReleaseEvidence(evidence, expectedVersion, manifest) {
  assert.equal(evidence.schema, RELEASE_EVIDENCE_SCHEMA);
  assert.equal(evidence.version, expectedVersion);
  assert.equal(evidence.platform, "windows-x64");
  assert.match(evidence.sourceCommit, SHA40);
  assert(!Number.isNaN(Date.parse(evidence.generatedAt)), "generatedAt must be ISO-8601");

  assert.equal(evidence.runner.name, "verity.exe");
  assert.equal(evidence.runner.version, expectedVersion);
  assert.match(evidence.runner.sha256, SHA256);

  assert(Array.isArray(evidence.artifacts), "artifacts must be an array");
  assert.equal(evidence.artifacts.length, 2, "evidence must contain exactly MSI and NSIS artifacts");
  assert.deepEqual(new Set(evidence.artifacts.map(({ kind }) => kind)), new Set(["msi", "nsis"]));
  for (const artifact of evidence.artifacts) {
    assert(artifact.name.length > 0, `${artifact.kind}.name must not be empty`);
    assert.match(artifact.sha256, SHA256);
    assert(artifact.signerSubject.length > 0, `${artifact.kind}.signerSubject must not be empty`);
    assert(artifact.timestampSubject.length > 0, `${artifact.kind}.timestampSubject must not be empty`);
    for (const gate of ["trustedSignature", "installSmoke", "launchSmoke", "uninstallSmoke"]) {
      assert.equal(artifact[gate], true, `${artifact.kind}.${gate} must pass`);
    }
  }

  for (const gate of ["trustedSignature", "installSmoke", "launchSmoke", "uninstallSmoke"]) {
    assert.equal(evidence[gate], true, `${gate} must pass`);
  }

  assert(Array.isArray(evidence.projects), "projects must be an array");
  assert(evidence.projects.length >= 15, "at least 15 real-project results are required");
  assert.equal(new Set(evidence.projects.map(({ id }) => id)).size, evidence.projects.length, "project IDs must be unique");
  assert.equal(new Set(evidence.projects.map(({ repository }) => repository)).size, evidence.projects.length, "project repositories must be unique");
  for (const project of evidence.projects) {
    assert.match(project.revision, SHA40);
    assert.match(project.receiptSha256, SHA256);
    assert.equal(project.result, "passed", `${project.id} must pass`);
    assert(project.targetId.length > 0, `${project.id} targetId must not be empty`);
  }

  if (manifest) {
    validateProjectManifest(manifest);
    assert.equal(evidence.projects.length, manifest.projects.length, "evidence must cover the complete pinned manifest");
    const evidenceById = new Map(evidence.projects.map((project) => [project.id, project]));
    for (const expected of manifest.projects) {
      const actual = evidenceById.get(expected.id);
      assert(actual, `missing evidence for ${expected.id}`);
      assert.equal(actual.repository, expected.repository, `${expected.id} repository must match the manifest`);
      assert.equal(actual.revision, expected.revision, `${expected.id} revision must match the manifest`);
      assert.equal(actual.stack, expected.stack, `${expected.id} stack must match the manifest`);
    }
  }
  return evidence;
}
