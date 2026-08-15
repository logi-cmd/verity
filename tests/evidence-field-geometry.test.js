// SPDX-License-Identifier: MPL-2.0

import assert from "node:assert/strict";
import test from "node:test";
import {
  EVIDENCE_DOT_COLUMNS,
  EVIDENCE_DOT_FIELD_SEED,
  EVIDENCE_DOT_JITTER,
  EVIDENCE_DOT_ROWS,
  EVIDENCE_TERRAIN_LATERAL,
  EVIDENCE_TERRAIN_VERTICAL,
  createEvidenceDotField,
  createEvidenceStaticPaths,
} from "../desktop/src/app/evidenceDotFieldGeometry.js";
import {
  EVIDENCE_EVENT_SPEC,
  normalizeEvidenceEventProgress,
  resolveEvidenceEvent,
} from "../desktop/src/app/evidenceFieldMotion.js";
import { degradeEvidenceFieldQuality } from "../desktop/src/app/evidenceFieldQuality.js";

test("evidence dot field is fixed, bounded, and triangulated", () => {
  const first = createEvidenceDotField();
  const second = createEvidenceDotField();
  assert.equal(first.seed, EVIDENCE_DOT_FIELD_SEED);
  assert.equal(first.columns, 64);
  assert.equal(first.rows, 42);
  assert.equal(first.signature, second.signature);
  assert.equal(first.signature, "46759b43");
  assert.equal(first.dotPositions.length, EVIDENCE_DOT_COLUMNS * EVIDENCE_DOT_ROWS * 2);
  assert.equal(first.dotStyles.length, EVIDENCE_DOT_COLUMNS * EVIDENCE_DOT_ROWS * 3);
  assert.equal(first.triangles.length, 5166 * 3);
  assert.equal(first.edgeIndices.length, 7853 * 2);
  assert.equal(first.edgePositions.length, first.edgeIndices.length * 2);
  assert.equal(first.edgeStyles.length, first.edgeIndices.length * 3);
  assert.equal(first.dotTerrain.length, EVIDENCE_DOT_COLUMNS * EVIDENCE_DOT_ROWS * 3);
  assert.equal(first.edgeTerrain.length, first.edgeIndices.length * 3);
  assert.equal("fiberPolar" in first, false);
  assert.equal("fiberSeed" in first, false);
  for (const value of first.dotPositions) assert.ok(Number.isFinite(value) && value >= 0 && value <= 1);
  for (const value of first.edgePositions) assert.ok(Number.isFinite(value) && value >= 0 && value <= 1);
  for (const value of first.dotTerrain) assert.ok(Number.isFinite(value) && value >= -1 && value <= 1);
  for (const value of first.edgeTerrain) assert.ok(Number.isFinite(value) && value >= -1 && value <= 1);
  const edges = new Set();
  for (let index = 0; index < first.edgeIndices.length; index += 2) {
    const left = first.edgeIndices[index];
    const right = first.edgeIndices[index + 1];
    assert.ok(left < right);
    assert.ok(left < EVIDENCE_DOT_COLUMNS * EVIDENCE_DOT_ROWS);
    assert.ok(right < EVIDENCE_DOT_COLUMNS * EVIDENCE_DOT_ROWS);
    edges.add(`${left}:${right}`);
  }
  assert.equal(edges.size, first.edgeIndices.length / 2);
  for (let index = 0; index < first.dotStyles.length; index += 3) {
    assert.ok(first.dotStyles[index] >= 0.9 && first.dotStyles[index] <= 1.7);
    assert.ok(first.dotStyles[index + 1] >= 0.58 && first.dotStyles[index + 1] <= 1);
  }
});

test("terrain projection is deterministic and used by the static surface", () => {
  const field = createEvidenceDotField();
  const paths = createEvidenceStaticPaths(field);
  assert.equal(EVIDENCE_TERRAIN_VERTICAL, 0.05);
  assert.equal(EVIDENCE_TERRAIN_LATERAL, 0.018);
  assert.match(paths.edges, /^M/);
  assert.match(paths.dots, /^M/);
  const rawFirstY = (field.dotPositions[1] * 1000).toFixed(2);
  assert.notEqual(paths.dots.split("h.01", 1)[0].split(" ")[1], rawFirstY);
});

test("changing the evidence seed changes dots without changing their contract", () => {
  const original = createEvidenceDotField();
  const changed = createEvidenceDotField({ seed: EVIDENCE_DOT_FIELD_SEED + 1 });
  assert.notEqual(changed.signature, original.signature);
  assert.equal(changed.dotPositions.length, original.dotPositions.length);
  assert.ok(changed.edgeIndices.length > 0);
});

test("dot jitter cannot exceed the fixed material envelope", () => {
  assert.throws(() => createEvidenceDotField({ jitter: EVIDENCE_DOT_JITTER + 0.001 }));
});

test("evidence material degrades once per tier at the frame budget", () => {
  assert.equal(degradeEvidenceFieldQuality("full", 20), "full");
  assert.equal(degradeEvidenceFieldQuality("full", 20.01), "compact");
  assert.equal(degradeEvidenceFieldQuality("compact", 24), "static");
  assert.equal(degradeEvidenceFieldQuality("static", 40), "static");
});

test("evidence event timing is fixed and returns to rest", () => {
  assert.deepEqual(EVIDENCE_EVENT_SPEC.selection, { duration: 720, radius: 140, compressionEnd: 90, waveEnd: 580 });
  assert.deepEqual(EVIDENCE_EVENT_SPEC.heartbeat, { duration: 260, radius: 80 });
  assert.equal(normalizeEvidenceEventProgress(1000, 1000, 760), 0);
  assert.equal(normalizeEvidenceEventProgress(1000, 1090, 720), 90 / 720);
  assert.equal(normalizeEvidenceEventProgress(1000, 1580, 720), 580 / 720);
  assert.equal(normalizeEvidenceEventProgress(1000, 1720, 720), 1);
  assert.equal(normalizeEvidenceEventProgress(1000, 1900, 720), 1);
});

test("terminal runner events outrank selection and heartbeat", () => {
  assert.equal(resolveEvidenceEvent({ interactionChanged: true, heartbeatChanged: true, statusChanged: true, status: "blocked" }), "blocked");
  assert.equal(resolveEvidenceEvent({ interactionChanged: true, heartbeatChanged: true, statusChanged: true, status: "verified" }), "verified");
  assert.equal(resolveEvidenceEvent({ interactionChanged: true, heartbeatChanged: true, statusChanged: false, status: "running" }), "selection");
  assert.equal(resolveEvidenceEvent({ interactionChanged: false, heartbeatChanged: true, statusChanged: false, status: "running" }), "heartbeat");
  assert.equal(resolveEvidenceEvent({ interactionChanged: false, heartbeatChanged: false, statusChanged: false, status: "idle" }), "none");
});
