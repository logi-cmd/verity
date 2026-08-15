// SPDX-License-Identifier: MPL-2.0

import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { commandText } from "./verificationPhases.js";

function formatDuration(milliseconds) {
  if (!Number.isFinite(milliseconds)) return null;
  if (milliseconds < 1000) return `${milliseconds} ms`;
  const seconds = milliseconds / 1000;
  return seconds < 60 ? `${seconds.toFixed(seconds < 10 ? 1 : 0)} s` : `${Math.floor(seconds / 60)}m ${Math.round(seconds % 60)}s`;
}

function EvidenceRows({ rows, emptyLabel }) {
  return <dl className="vr-evidence-rows">{rows.map((row) => <div key={row.label}><dt>{row.label}</dt><dd className={row.code ? "is-code" : undefined}>{row.value || emptyLabel}</dd></div>)}</dl>;
}

export function PhaseEvidenceRail({ item, phaseName, currentPhase, following, onFollowCurrent, mode, plan, target, repositoryRoot, session, receipt, blockerText, labels }) {
  const reduceMotion = useReducedMotion();
  const backendPhases = new Set(item.backend);
  const liveProgress = [...(session?.phase_progress || [])].reverse().find((entry) => backendPhases.has(entry.phase));
  const observed = item.activeObservation;
  const planned = item.commands[0];
  const command = liveProgress?.command?.join(" ") || commandText(observed || planned);
  const source = liveProgress?.command_source || observed?.command_source || planned?.evidence || (item.id === "oracle" ? target?.oracle?.evidence?.[0] : null);
  const observations = (session?.observations || []).filter((entry) => backendPhases.has(entry.phase)).slice(-3).reverse();
  if (!observations.length && observed?.output_excerpt) {
    observations.push({ at: observed.finished_at, text: observed.output_excerpt });
  }
  const elapsed = liveProgress?.elapsed_ms ?? observed?.duration_ms;
  const determinate = liveProgress && !liveProgress.indeterminate && Number.isFinite(liveProgress.total_units) && liveProgress.total_units > 0;
  const indeterminate = Boolean(liveProgress?.indeterminate && item.state === "running");
  const percentage = determinate ? Math.min(100, Math.round((liveProgress.completed_units / liveProgress.total_units) * 100)) : null;
  const statusText = labels.phaseState(item.state);
  const stepNumber = labels.phaseOrder[item.id];
  const environmentRows = [
    { label: labels.started, value: liveProgress?.started_at ? new Date(liveProgress.started_at).toLocaleString() : labels.notYetProduced },
    { label: labels.workingDirectory, value: liveProgress?.working_directory || repositoryRoot, code: true },
    { label: labels.environment, value: labels.formatTechnical(liveProgress?.execution_environment || receipt?.execution_environment || (planned?.native ? "confirmed native snapshot" : "isolated runtime")), code: true },
    { label: labels.network, value: labels.formatTechnical(liveProgress?.network || observed?.network || planned?.network || "none"), code: true },
  ];

  return <aside className="vr-evidence-rail" aria-live="polite">
    <header className="vr-evidence-rail__header">
      <div><span>{labels.step} {stepNumber}/6</span><h2>{phaseName}</h2></div>
      <div className="vr-evidence-rail__header-actions">
        {!following && currentPhase !== item.id ? <button type="button" className="vr-follow-current" data-action-id="phase.follow-current" onClick={onFollowCurrent}>{labels.followCurrent}</button> : null}
        <strong data-state={item.state}>{statusText}</strong>
      </div>
    </header>
    <AnimatePresence mode="wait" initial={false}>
      <motion.div
        key={`${item.id}-${mode}`}
        className="vr-evidence-rail__body"
        initial={reduceMotion ? false : { opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        exit={reduceMotion ? undefined : { opacity: 0, y: -6 }}
        transition={{ duration: reduceMotion ? 0 : 0.24, ease: [0.23, 1, 0.32, 1] }}
      >
        {mode === "receipt" ? <section className="vr-evidence-section"><h3>{labels.proof}</h3><p>{labels.formatTechnical(receipt?.oracle?.detail) || labels.notYetProduced}</p>{receipt ? <EvidenceRows emptyLabel={labels.notYetProduced} rows={[{ label: labels.result, value: receipt.result }, { label: labels.signature, value: receipt.local_signature, code: true }, { label: "Snapshot", value: receipt.snapshot_fingerprint, code: true }]} /> : null}</section> : <>
          <section className="vr-evidence-section vr-evidence-section--progress">
            <div className="vr-progress-heading"><span>{labels.progress}</span><strong>{percentage !== null ? `${percentage}%` : indeterminate ? labels.indeterminate : labels.notYetProduced}</strong></div>
            {percentage !== null ? <div className="vr-progress-line" aria-label={`${percentage}%`}><i style={{ transform: `scaleX(${percentage / 100})` }} /></div> : indeterminate ? <div className="vr-progress-line is-indeterminate" aria-label={labels.indeterminate}><i /></div> : <div className="vr-progress-line" aria-label={labels.notYetProduced} />}
            <div className="vr-primary-readouts">
              <div><span>{labels.elapsed}</span><strong>{formatDuration(elapsed) || labels.notYetProduced}</strong></div>
              <div><span>{labels.estimate}</span><strong>{liveProgress || observed ? labels.noEstimate : labels.notYetProduced}</strong></div>
            </div>
          </section>
          <section className="vr-evidence-section vr-evidence-section--environment">
            <h3>{labels.environment}</h3>
            <EvidenceRows emptyLabel={labels.notYetProduced} rows={environmentRows} />
          </section>
          {mode === "logs" ? null : <section className="vr-evidence-section vr-evidence-section--command">
            <h3>{labels.liveEvidence}</h3>
            <div className="vr-command-readout"><span>{labels.command}</span><code>{command || labels.notYetProduced}</code></div>
            <div className="vr-command-source"><span>{labels.commandSource}</span><code>{source ? `${source.path} / ${source.key}` : labels.notYetProduced}</code></div>
          </section>}
          <section className="vr-evidence-section vr-evidence-section--observations"><h3>{labels.latestObservations}</h3>{observations.length ? <ol>{observations.map((entry, index) => <li key={`${entry.at}-${index}`}><time>{entry.at ? new Date(entry.at).toLocaleTimeString() : ""}</time><code>{entry.text}</code></li>)}</ol> : <p>{labels.noObservations}</p>}</section>
          {item.state === "failed" ? <section className="vr-evidence-section vr-evidence-section--blocker"><h3>{receipt?.first_observed_blocker ? labels.firstBlocker : labels.planBlocker}</h3><strong>{blockerText?.summary || session?.error || session?.message}</strong><p>{blockerText?.detail || ""}</p></section> : null}
        </>}
      </motion.div>
    </AnimatePresence>
  </aside>;
}
