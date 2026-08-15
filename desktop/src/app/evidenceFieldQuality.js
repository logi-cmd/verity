// SPDX-License-Identifier: MPL-2.0

export const EVIDENCE_FIELD_FRAME_BUDGET_MS = 20;

export function degradeEvidenceFieldQuality(current, p95, budget = EVIDENCE_FIELD_FRAME_BUDGET_MS) {
  if (!Number.isFinite(p95) || p95 <= budget || current === "static") return current;
  return current === "full" ? "compact" : "static";
}
