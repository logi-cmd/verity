// SPDX-License-Identifier: MPL-2.0
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  RELEASE_EVIDENCE_SCHEMA,
  selectAcceptanceTarget,
  validateProjectManifest,
  validateReleaseEvidence,
} from "./windows-release-acceptance-lib.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const defaults = {
  manifest: join(root, "docs/qa/windows-x64-real-projects.json"),
  output: join(root, "docs/qa/windows-x64-release-evidence.json"),
};

function parseArgs(argv) {
  const options = { ...defaults, validateManifest: false, preflightProjects: false };
  for (let index = 0; index < argv.length; index += 1) {
    const name = argv[index];
    if (name === "--validate-manifest") {
      options.validateManifest = true;
      continue;
    }
    if (name === "--preflight-projects") {
      options.preflightProjects = true;
      continue;
    }
    const key = ({
      "--manifest": "manifest",
      "--output": "output",
      "--cli": "cli",
      "--msi": "msi",
      "--setup": "setup",
    })[name];
    assert(key, `unknown argument: ${name}`);
    const value = argv[index + 1];
    assert(value && !value.startsWith("--"), `${name} requires a value`);
    options[key] = resolve(value);
    index += 1;
  }
  return options;
}

function run(program, args, options = {}) {
  const result = spawnSync(program, args, {
    cwd: options.cwd ?? root,
    encoding: "utf8",
    maxBuffer: 100 * 1024 * 1024,
    timeout: options.timeout ?? 20 * 60 * 1000,
    windowsHide: true,
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} ${args.join(" ")} failed\n${result.stderr}`);
  return result.stdout.trim();
}

function parseJsonOutput(output, label) {
  try {
    return JSON.parse(output);
  } catch (error) {
    throw new Error(`${label} did not return one JSON document: ${error.message}`);
  }
}

function runInstallerSmoke(kind, path) {
  const output = run("powershell.exe", [
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    join(root, "scripts/windows-installer-smoke.ps1"),
    "-InstallerType",
    kind,
    "-InstallerPath",
    path,
  ]);
  return parseJsonOutput(output.split(/\r?\n/).at(-1), `${kind} smoke`);
}

function clonePinnedProject(project, destination) {
  run("git", ["init", "--quiet", destination]);
  run("git", ["-C", destination, "remote", "add", "origin", project.repository]);
  run("git", ["-C", destination, "fetch", "--quiet", "--depth", "1", "origin", project.revision]);
  run("git", ["-C", destination, "checkout", "--quiet", "--detach", "FETCH_HEAD"]);
  assert.equal(run("git", ["-C", destination, "rev-parse", "HEAD"]), project.revision);
}

async function sha256File(path) {
  return createHash("sha256").update(await readFile(path)).digest("hex");
}

async function runProject(cli, project, workRoot) {
  const destination = join(workRoot, project.id);
  await mkdir(destination, { recursive: true });
  clonePinnedProject(project, destination);
  const plan = parseJsonOutput(run(cli, ["inspect", destination, "--json"]), `${project.id} inspect`);
  const target = selectAcceptanceTarget(plan, project);
  const receiptText = run(cli, ["check", destination, "--target", target.id], { timeout: 45 * 60 * 1000 });
  const receipt = parseJsonOutput(receiptText, `${project.id} check`);
  assert.equal(receipt.schema, "verity-verification-receipt.v3");
  assert.equal(receipt.result, "verified", `${project.id} did not verify`);
  return {
    id: project.id,
    repository: project.repository,
    revision: project.revision,
    stack: project.stack,
    targetId: target.id,
    receiptSha256: createHash("sha256").update(receiptText).digest("hex"),
    result: "passed",
  };
}

async function preflightProject(cli, project, workRoot) {
  const destination = join(workRoot, project.id);
  await mkdir(destination, { recursive: true });
  clonePinnedProject(project, destination);
  const plan = parseJsonOutput(run(cli, ["inspect", destination, "--json"]), `${project.id} inspect`);
  const target = selectAcceptanceTarget(plan, project);
  console.log(`${project.id}: ${project.revision.slice(0, 12)} -> ${target.id}`);
}

const options = parseArgs(process.argv.slice(2));
const manifest = validateProjectManifest(JSON.parse(await readFile(options.manifest, "utf8")));
if (options.validateManifest) {
  console.log(`Validated ${manifest.projects.length} pinned Windows x64 acceptance projects.`);
  process.exit(0);
}

if (options.preflightProjects) {
  assert(options.cli, "--cli is required with --preflight-projects");
  const preflightRoot = await mkdtemp(join(tmpdir(), "verity-windows-preflight-"));
  try {
    for (const project of manifest.projects) await preflightProject(options.cli, project, preflightRoot);
    console.log(`Preflight validated ${manifest.projects.length} pinned repositories.`);
  } finally {
    await rm(preflightRoot, { recursive: true, force: true });
  }
  process.exit(0);
}

assert.equal(process.platform, "win32", "Windows acceptance must run on Windows");
for (const key of ["cli", "msi", "setup"]) assert(options[key], `--${key} is required`);
const packageManifest = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
const sourceCommit = run("git", ["rev-parse", "HEAD"]);
assert.match(sourceCommit, /^[0-9a-f]{40}$/);
assert.equal(run("git", ["status", "--porcelain", "--untracked-files=no"]), "", "acceptance must run from a clean tracked source tree");
const runnerVersionOutput = run(options.cli, ["--version"]);
assert.equal(runnerVersionOutput, `verity ${packageManifest.version}`, "CLI version must match package.json");

await rm(options.output, { force: true });
const workRoot = await mkdtemp(join(tmpdir(), "verity-windows-acceptance-"));
try {
  const artifacts = [
    runInstallerSmoke("msi", options.msi),
    runInstallerSmoke("nsis", options.setup),
  ];
  const projects = [];
  for (const project of manifest.projects) {
    console.log(`Running ${project.id} at ${project.revision.slice(0, 12)}...`);
    projects.push(await runProject(options.cli, project, workRoot));
  }

  const evidence = {
    schema: RELEASE_EVIDENCE_SCHEMA,
    version: packageManifest.version,
    platform: "windows-x64",
    sourceCommit,
    generatedAt: new Date().toISOString(),
    runner: {
      name: basename(options.cli),
      version: packageManifest.version,
      sha256: await sha256File(options.cli),
    },
    artifacts,
    trustedSignature: artifacts.every(({ trustedSignature }) => trustedSignature),
    installSmoke: artifacts.every(({ installSmoke }) => installSmoke),
    launchSmoke: artifacts.every(({ launchSmoke }) => launchSmoke),
    uninstallSmoke: artifacts.every(({ uninstallSmoke }) => uninstallSmoke),
    projects,
  };
  validateReleaseEvidence(evidence, packageManifest.version, manifest);
  await mkdir(dirname(options.output), { recursive: true });
  await writeFile(options.output, `${JSON.stringify(evidence, null, 2)}\n`, "utf8");
  console.log(`Wrote ${options.output} with ${projects.length} passing real projects.`);
} finally {
  await rm(workRoot, { recursive: true, force: true });
}
