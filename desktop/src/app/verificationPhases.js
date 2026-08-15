// SPDX-License-Identifier: MPL-2.0

export const LAST_REPOSITORY_KEY = "verity.lastRepository.v1";

export const VISIBLE_PHASES = [
  { id: "detect", backend: ["detect"] },
  { id: "plan", backend: [] },
  { id: "acquire", backend: ["acquire"] },
  { id: "build", backend: ["build"] },
  { id: "exercise", backend: ["test", "launch"] },
  { id: "oracle", backend: ["oracle", "receipt"] },
];

export function selectDefaultTarget(plan) {
  if (!plan?.targets?.length) return "";
  const recommended = plan.targets.find((target) => target.recommended && target.plan_status === "complete");
  if (recommended) return recommended.id;
  const products = plan.targets.filter((target) => ["product", "service"].includes(target.role));
  return products.length === 1 ? products[0].id : "";
}

export function visiblePhaseForBackend(phase) {
  return VISIBLE_PHASES.find((item) => item.backend.includes(phase))?.id || "plan";
}

export function derivePhaseItems(plan, target, session, receipt) {
  const observed = new Map((receipt?.phases || []).map((item) => [item.phase, item]));
  const liveProgress = new Map((session?.phase_progress || []).map((item) => [item.phase, item]));
  const sessionBlocked = session?.status === "blocked" || session?.status === "internal_error";

  return VISIBLE_PHASES.map((definition) => {
    const commands = (target?.commands || []).filter((command) => definition.backend.includes(command.phase));
    const observations = definition.backend.map((phase) => observed.get(phase)).filter(Boolean);
    const liveEntries = definition.backend.map((phase) => liveProgress.get(phase)).filter(Boolean);
    const planBlockerHere = (target?.blockers || []).some((blocker) => definition.backend.includes(blocker.phase));
    const detectionBlocked = (target?.blockers || []).some((blocker) => blocker.phase === "detect");
    const active = session?.status === "running" && definition.backend.includes(session.current_phase);
    const observedFailure = observations.find((item) => !item.success);
    const blockedHere = sessionBlocked && definition.backend.includes(session?.current_phase);
    let state = "pending";

    if (definition.id === "detect") state = planBlockerHere ? "failed" : plan ? "done" : "pending";
    else if (definition.id === "plan") state = target ? (detectionBlocked ? "planned" : "done") : "pending";
    else if (planBlockerHere) state = "failed";
    else if (definition.id === "oracle") {
      if (receipt?.oracle?.passed) state = "done";
      else if (receipt?.result === "started_unverified") state = "unverified";
      else if (receipt || blockedHere) state = "failed";
      else if (active) state = "running";
      else state = "planned";
    } else if (liveEntries.some((entry) => entry.event_kind === "blocked" || entry.event_kind === "cancelled")) state = "failed";
    else if (active) state = "running";
    else if (liveEntries.length && liveEntries.every((entry) => entry.event_kind === "completed")) state = "done";
    else if (!commands.length) state = "skipped";
    else if (observedFailure || blockedHere) state = "failed";
    else if (observations.length >= commands.length && observations.every((item) => item.success)) state = "done";
    else state = "planned";

    return {
      ...definition,
      state,
      commands,
      observations,
      activeObservation: observedFailure || observations.at(-1) || null,
    };
  });
}

export function commandText(command) {
  if (!command) return "";
  if (Array.isArray(command.command)) return command.command.join(" ");
  return [command.program, ...(command.args || [])].filter(Boolean).join(" ");
}
