// SPDX-License-Identifier: MPL-2.0

export const EVIDENCE_EVENT_KIND = Object.freeze({
  none: 0,
  selection: 1,
  heartbeat: 2,
  blocked: 3,
  verified: 4,
});

export const EVIDENCE_EVENT_SPEC = Object.freeze({
  selection: Object.freeze({ duration: 720, radius: 140, compressionEnd: 90, waveEnd: 580 }),
  heartbeat: Object.freeze({ duration: 260, radius: 80 }),
  blocked: Object.freeze({ duration: 300, radius: 150 }),
  verified: Object.freeze({ duration: 300, radius: 0 }),
});

export function normalizeEvidenceEventProgress(startedAt, now, duration) {
  if (!Number.isFinite(duration) || duration <= 0) return 1;
  return Math.min(1, Math.max(0, (now - startedAt) / duration));
}

export function resolveEvidenceEvent({ interactionChanged, heartbeatChanged, statusChanged, status }) {
  if (statusChanged && status === "blocked") return "blocked";
  if (statusChanged && status === "verified") return "verified";
  if (interactionChanged) return "selection";
  if (heartbeatChanged) return "heartbeat";
  return "none";
}
