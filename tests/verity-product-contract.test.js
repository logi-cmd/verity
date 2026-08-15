// SPDX-License-Identifier: MPL-2.0
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";

const text = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("product exposes only the deterministic verification surface", async () => {
  const main = await text("desktop/src-tauri/src/main.rs");
  for (const command of ["inspect_repository", "execute_run_session", "list_receipts", "verify_receipt", "runtime_doctor"]) {
    assert.match(main, new RegExp(`verification::${command}`));
  }
  for (const legacy of ["unified_graph", "integrations", "project_cognition", "entitlement", "account"]) {
    assert.doesNotMatch(main, new RegExp(legacy));
  }
});

test("desktop package keeps only the focused visual runtimes", async () => {
  const manifest = JSON.parse(await text("desktop/package.json"));
  const dependencies = { ...manifest.dependencies, ...manifest.devDependencies };
  for (const retired of ["cytoscape", "three", "@react-three/fiber", "remotion", "@remotion/renderer", "gsap", "d3", "pg"]) {
    assert.equal(dependencies[retired], undefined, `${retired} must be removed`);
  }
  assert.match(dependencies.ogl, /^\^1\.0\./, "the evidence field uses the focused OGL runtime only");
  assert.match(dependencies.motion, /^\^13\.0\./, "Motion owns DOM and SVG state transitions");
  assert.equal(dependencies.delaunator, "5.1.0", "Delaunator owns the deterministic evidence topology");
  assert.equal(dependencies["@fontsource-variable/geist"], "5.2.8");
  assert.equal(dependencies["@fontsource-variable/geist-mono"], "5.2.8");
  assert.equal(dependencies["robust-predicates"], undefined, "the retired topology predicate runtime must be absent");
});

test("desktop uses one continuous task surface and stable action ids", async () => {
  const app = await text("desktop/src/app/App.jsx");
  const css = await text("desktop/src/verity.css");
  assert.doesNotMatch(`${app}\n${css}`, /\b(?:card|graph|plugin|briefing)\b/i);
  assert.match(app, /data-action-id=/);
  assert.match(app, /aria-disabled=/);
  assert.doesNotMatch(css, /transition\s*:\s*all/i);
  assert.match(css, /prefers-reduced-motion/);
  assert.match(app, /当前检查/);
  assert.match(app, /机器验证完成/);
  assert.doesNotMatch(app, /�|锟斤拷|褰撳|銆|鈥/);
});

test("current check uses the evidence path, rail, and fixed session bar only", async () => {
  const app = await text("desktop/src/app/App.jsx");
  const rail = await text("desktop/src/app/PhaseEvidenceRail.jsx");
  const css = await text("desktop/src/verity.css");
  assert.match(app, /<VerificationEvidencePath/);
  assert.match(app, /<PhaseEvidenceRail/);
  assert.match(app, /<BlockerBranch/);
  assert.match(app, /className="vr-session-bar"/);
  assert.doesNotMatch(`${app}\n${css}`, /vr-(?:repository-bar|target-list|verification-surface|phase-sequence|phase-evidence|outcome-band)/);
  assert.match(rail, /liveProgress\?\.indeterminate && item\.state === "running"/);
  assert.match(rail, /labels\.notYetProduced/);
});

test("current check state survives navigation and restore never starts a run", async () => {
  const app = await text("desktop/src/app/App.jsx");
  const workspace = await text("desktop/src/app/VerificationWorkspaceContext.jsx");
  const phases = await text("desktop/src/app/verificationPhases.js");
  assert.match(app, /<VerificationWorkspaceProvider/);
  assert.match(app, /useVerificationWorkspace\(\)/);
  assert.match(phases, /verity\.lastRepository\.v1/);
  assert.match(workspace, /inspect\(repositoryRoot, \{ restoring: true \}\)/);
  assert.doesNotMatch(workspace, /localStorage[^\n]+session/i);
  assert.doesNotMatch(app, /vr-action-pane|vr-orbit/);
});

test("elastic evidence field is progressive and releases WebGL resources", async () => {
  const field = await text("desktop/src/app/ElasticEvidenceField.jsx");
  const dots = await text("desktop/src/app/evidenceDotFieldGeometry.js");
  assert.match(field, /from "ogl"/);
  assert.match(field, /\bPost\b/);
  assert.match(field, /data-material-quality/);
  assert.match(field, /mode: gl\.POINTS/);
  assert.match(field, /mode: gl\.LINES/);
  assert.match(dots, /Delaunator/);
  assert.match(field, /vr-elastic-field__static/);
  assert.match(field, /percentile95/);
  assert.match(field, /36/);
  assert.match(dots, /0x56455249/);
  assert.match(dots, /EVIDENCE_DOT_COLUMNS = 64/);
  assert.match(dots, /EVIDENCE_DOT_ROWS = 42/);
  assert.match(field, /motionProfile === "reduced"/);
  assert.match(field, /visibilitychange/);
  assert.match(field, /webglcontextlost/);
  assert.match(field, /WEBGL_lose_context/);
  assert.match(field, /pointermove/);
  assert.doesNotMatch(field, /mousemove|touchmove/);
  assert.doesNotMatch(field, /PARTICLE_|particleGeometry|particlePolar/i);
  assert.doesNotMatch(field, /fiber/i);
  assert.doesNotMatch(dots, /fiber/i);
  assert.doesNotMatch(`${field}\n${dots}`, /Math\.random/);
});

test("stage activation starts imperatively while arrow navigation stays quiet", async () => {
  const app = await text("desktop/src/app/App.jsx");
  const path = await text("desktop/src/app/VerificationEvidencePath.jsx");
  const field = await text("desktop/src/app/ElasticEvidenceField.jsx");
  assert.match(app, /const evidenceFieldRef = useRef\(null\)/);
  assert.match(app, /evidenceFieldRef\.current\?\.activateSelection\(phase, anchorElement\)/);
  assert.doesNotMatch(app, /interactionPulse/);
  assert.match(field, /useImperativeHandle/);
  assert.match(field, /activateSelection\(nextPhase, anchorElement\)/);
  assert.match(field, /pendingSelectionRef/);
  assert.match(field, /eventSerial \+= 1/);
  assert.doesNotMatch(field, /focusedPhase|focusChanged/);
  assert.doesNotMatch(app, /<ElasticEvidenceField[^>]+focusedPhase=/);
  assert.match(path, /onPointerDown/);
  assert.match(path, /event\.key === "Enter" \|\| event\.key === " "/);
  assert.match(path, /onFocusPhase\(items\[next\]\.id\)/);
  assert.doesNotMatch(path, /onVisualActivate\(items\[next\]/);
  assert.doesNotMatch(path, /onDoubleClick/);
});

test("stage nodes use one layered precision-instrument structure", async () => {
  const path = await text("desktop/src/app/VerificationEvidencePath.jsx");
  const css = await text("desktop/src/verity.css");
  for (const layer of ["vr-stage-node__orb", "vr-stage-node__bezel", "vr-stage-node__lens", "vr-stage-node__core"]) {
    assert.match(path, new RegExp(layer));
    assert.match(css, new RegExp(`\\.${layer}`));
  }
  assert.doesNotMatch(css, /repeating-conic-gradient/);
});

test("only new desktop Rust modules remain", async () => {
  const names = (await readdir(new URL("../desktop/src-tauri/src", import.meta.url))).sort();
  assert.deepEqual(names, ["main.rs", "verification.rs", "window.rs"]);
});

test("versioned deterministic schemas are present", async () => {
  const model = await text("crates/verity-core/src/model.rs");
  const runner = await text("crates/verity-runner/src/lib.rs");
  for (const schema of ["verity-run-plan.v3", "verity-run-session.v4", "verity-verification-receipt.v3", "verity-remediation-proposal.v1", "verity-agent-repair.v2", "verity-runtime-capability.v2", "verity-diagnostic-report.v1", "verity-cleanup-candidate.v1", "verity-cleanup-preview.v1", "verity-cleanup-session.v1", "verity-cleanup-receipt.v1"]) {
    assert.match(model, new RegExp(schema.replaceAll(".", "\\.")));
  }
  assert.match(model, /pub phase: RunPhase/);
  assert.match(model, /StartedUnverified/);
  assert.match(runner, /receipt\.schema != RECEIPT_SCHEMA/);
  assert.match(runner, /UnsupportedReceiptSchema/);
});

test("cleanup analyzers and expanded adapters preserve evidence boundaries", async () => {
  const cleanup = await text("crates/verity-runner/src/cleanup.rs");
  const adapters = await text("crates/verity-adapters/src/detect.rs");
  const runner = await text("crates/verity-runner/src/lib.rs");
  const app = await text("desktop/src/app/App.jsx");
  for (const analyzer of ["knip", "cargo-machete", "vulture", "go-deadcode"]) {
    assert.match(cleanup, new RegExp(analyzer));
  }
  assert.match(cleanup, /CleanupAnalyzerState::NotInstalled/);
  assert.match(cleanup, /CleanupAnalyzerState::UnsafeConfiguration/);
  assert.match(cleanup, /CleanupEligibility::ReportOnly/);
  for (const manifest of ["pom.xml", "build.gradle.kts", "CMakeLists.txt", "meson.build", "packages.lock.json", "composer.lock", "Gemfile.lock"]) {
    assert.match(adapters, new RegExp(manifest.replaceAll(".", "\\.")));
  }
  for (const image of ["maven:3.9-eclipse-temurin-21", "mcr.microsoft.com/dotnet/sdk:8.0-bookworm-slim", "composer:2", "ruby:3.3-bookworm"]) {
    assert.match(runner, new RegExp(image.replaceAll(".", "\\.")));
  }
  assert.match(app, /cleanupAnalyzers/);
  assert.match(app, /knip_not_in_verified_dependency_graph/);
});

test("runtime, Agent, and diagnostics expose truthful action contracts", async () => {
  const model = await text("crates/verity-core/src/model.rs");
  const runner = await text("crates/verity-runner/src/environment.rs");
  const agents = await text("crates/verity-runner/src/agents.rs");
  const diagnostics = await text("crates/verity-runner/src/diagnostics.rs");
  const main = await text("desktop/src-tauri/src/main.rs");
  const app = await text("desktop/src/app/App.jsx");
  for (const status of ["NotInstalled", "Stopped", "Starting", "DaemonUnreachable", "BuildkitUnavailable", "CapabilityIncomplete", "Ready", "Error"]) {
    assert.match(model, new RegExp(`\\b${status}\\b`));
  }
  for (const command of ["start_docker_desktop", "test_agent_capability", "start_agent_repair", "read_agent_repair", "cancel_agent_repair", "apply_agent_repair", "export_agent_patch", "launch_agent_desktop", "copy_agent_task", "preview_diagnostic_report", "export_diagnostic_report", "copy_diagnostic_issue_summary"]) {
    assert.match(main, new RegExp(`verification::${command}`));
  }
  assert.match(runner, /Docker Desktop\.exe/);
  assert.match(agents, /where\.exe/);
  assert.match(agents, /Get-StartApps/);
  assert.match(agents, /JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE/);
  assert.match(diagnostics, /repository_root/);
  assert.doesNotMatch(app, /Anonymous quality telemetry|匿名质量遥测|telemetryBody/);
  assert.match(app, /previewDiagnosticReport/);
});
