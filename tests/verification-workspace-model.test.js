// SPDX-License-Identifier: MPL-2.0

import assert from "node:assert/strict";
import test from "node:test";
import {
  derivePhaseItems,
  selectDefaultTarget,
  visiblePhaseForBackend,
} from "../desktop/src/app/verificationPhases.js";

const plan = {
  inspection_fingerprint: "fingerprint",
  targets: [
    { id: "blocked", role: "product", recommended: false, plan_status: "incomplete", commands: [], blockers: [{ summary: "blocked" }] },
    {
      id: "ready",
      role: "product",
      recommended: true,
      plan_status: "complete",
      blockers: [],
      commands: [
        { phase: "acquire", program: "npm", args: ["ci"] },
        { phase: "build", program: "npm", args: ["run", "build"] },
        { phase: "test", program: "npm", args: ["test"] },
        { phase: "launch", program: "npm", args: ["start"] },
      ],
    },
  ],
};

test("default selection prefers the unique recommended product target", () => {
  assert.equal(selectDefaultTarget(plan), "ready");
  assert.equal(selectDefaultTarget({ targets: [] }), "");
});

test("backend phases map into the six visible verification stages", () => {
  assert.equal(visiblePhaseForBackend("test"), "exercise");
  assert.equal(visiblePhaseForBackend("launch"), "exercise");
  assert.equal(visiblePhaseForBackend("receipt"), "oracle");
});

test("phase derivation reports only observed or declared state", () => {
  const target = plan.targets[1];
  const session = { status: "running", current_phase: "build" };
  const states = Object.fromEntries(derivePhaseItems(plan, target, session, null).map((item) => [item.id, item.state]));
  assert.deepEqual(states, {
    detect: "done",
    plan: "done",
    acquire: "planned",
    build: "running",
    exercise: "planned",
    oracle: "planned",
  });
});

test("live completed progress is visible before the final receipt exists", () => {
  const target = plan.targets[1];
  const session = {
    status: "running",
    current_phase: "build",
    phase_progress: [
      { phase: "acquire", event_kind: "completed" },
      { phase: "build", event_kind: "heartbeat" },
    ],
  };
  const states = Object.fromEntries(derivePhaseItems(plan, target, session, null).map((item) => [item.id, item.state]));
  assert.equal(states.acquire, "done");
  assert.equal(states.build, "running");
});

test("an observed failure cannot be promoted to completion", () => {
  const target = plan.targets[1];
  const receipt = {
    result: "blocked",
    phases: [
      { phase: "test", success: true },
      { phase: "launch", success: false },
    ],
    oracle: { passed: false },
  };
  const states = Object.fromEntries(derivePhaseItems(plan, target, { status: "blocked", current_phase: "launch" }, receipt).map((item) => [item.id, item.state]));
  assert.equal(states.exercise, "failed");
  assert.equal(states.oracle, "failed");
});

test("a bounded launch without a machine oracle remains explicitly unverified", () => {
  const target = { ...plan.targets[1], commands: plan.targets[1].commands.filter((command) => command.phase !== "test"), oracle: { machine_verifiable: false } };
  const receipt = {
    result: "started_unverified",
    phases: [{ phase: "launch", success: true }],
    oracle: { passed: false },
  };
  const states = Object.fromEntries(derivePhaseItems(plan, target, { status: "started_unverified", current_phase: "receipt" }, receipt).map((item) => [item.id, item.state]));
  assert.equal(states.exercise, "done");
  assert.equal(states.oracle, "unverified");
});

test("a planning blocker is rendered at its real execution phase", () => {
  const target = {
    ...plan.targets[0],
    blockers: [{ phase: "acquire", code: "node_lockfile_missing", summary: "missing lock" }],
  };
  const states = Object.fromEntries(derivePhaseItems(plan, target, null, null).map((item) => [item.id, item.state]));
  assert.equal(states.detect, "done");
  assert.equal(states.plan, "done");
  assert.equal(states.acquire, "failed");
});
