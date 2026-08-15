// SPDX-License-Identifier: MPL-2.0

import { useRef } from "react";
import { AlertTriangle, Check } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";

export const PHASE_POSITIONS = {
  detect: { x: 188, y: 76 },
  plan: { x: 258, y: 177 },
  acquire: { x: 324, y: 282 },
  build: { x: 414, y: 386 },
  exercise: { x: 470, y: 500 },
  oracle: { x: 510, y: 620 },
};

const PATH = "M188 76 C198 118 226 146 258 177 C285 203 293 250 324 282 C355 315 376 353 414 386 C443 424 458 463 470 500 C486 540 502 578 510 620";

function PhaseGlyph({ state, index }) {
  if (state === "done") return <Check aria-hidden="true" />;
  if (state === "failed") return <AlertTriangle aria-hidden="true" />;
  return <span aria-hidden="true">{index + 1}</span>;
}

export function VerificationEvidencePath({ items, labels, focusedPhase, currentPhase, onFocusPhase, onVisualActivate }) {
  const reduceMotion = useReducedMotion();
  const buttonRefs = useRef([]);
  const currentIndex = Math.max(0, items.findIndex((item) => item.id === currentPhase));
  const pathProgress = items.length > 1 ? currentIndex / (items.length - 1) : 0;

  function moveFocus(index, direction) {
    const next = Math.min(items.length - 1, Math.max(0, index + direction));
    onFocusPhase(items[next].id);
    buttonRefs.current[next]?.focus();
  }

  return <div className="vr-evidence-path" aria-label={labels.phases}>
    <svg className="vr-evidence-path__lines" viewBox="0 0 720 690" preserveAspectRatio="none" aria-hidden="true">
      <defs>
        <linearGradient id="vr-path-progress-gradient" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#7f6bea" />
          <stop offset="0.58" stopColor="#a390ff" />
          <stop offset="1" stopColor="#6d58d9" />
        </linearGradient>
      </defs>
      <path className="vr-evidence-path__shadow" d={PATH} pathLength="1" />
      <path className="vr-evidence-path__base" d={PATH} pathLength="1" />
      <path className="vr-evidence-path__highlight" d={PATH} pathLength="1" />
      <motion.path
        className="vr-evidence-path__progress"
        d={PATH}
        pathLength="1"
        initial={false}
        animate={{ pathLength: pathProgress }}
        transition={reduceMotion ? { duration: 0 } : { duration: 0.28, ease: [0.23, 1, 0.32, 1] }}
      />
    </svg>
    <ol className="vr-evidence-path__stages">
      {items.map((item, index) => {
        const point = PHASE_POSITIONS[item.id];
        const selected = focusedPhase === item.id;
        const current = currentPhase === item.id;
        return <li key={item.id} style={{ "--phase-x": `${(point.x / 720) * 100}%`, "--phase-y": `${(point.y / 690) * 100}%` }} data-state={item.state} data-current={current || undefined}>
          <motion.button
            ref={(node) => { buttonRefs.current[index] = node; }}
            type="button"
            className="vr-stage-node"
            data-phase-id={item.id}
            data-action-id={`phase.focus.${item.id}`}
            aria-current={current ? "step" : undefined}
            aria-pressed={selected}
            onPointerDown={(event) => {
              if (event.button === 0) onVisualActivate(item.id, event.currentTarget);
            }}
            onClick={() => onFocusPhase(item.id)}
            onKeyDown={(event) => {
              if (!event.repeat && (event.key === "Enter" || event.key === " ")) onVisualActivate(item.id, event.currentTarget);
              if (event.key === "ArrowDown" || event.key === "ArrowRight") { event.preventDefault(); moveFocus(index, 1); }
              if (event.key === "ArrowUp" || event.key === "ArrowLeft") { event.preventDefault(); moveFocus(index, -1); }
              if (event.key === "Home") { event.preventDefault(); moveFocus(index, -items.length); }
              if (event.key === "End") { event.preventDefault(); moveFocus(index, items.length); }
            }}
            whileTap={reduceMotion ? undefined : { scale: 0.97 }}
            transition={{ duration: 0.14 }}
          >
            <span className="vr-stage-node__orb">
              <span className="vr-stage-node__bezel" aria-hidden="true" />
              <span className="vr-stage-node__lens">
                <span className="vr-stage-node__core"><PhaseGlyph state={item.state} index={index} /></span>
              </span>
            </span>
            <span className="vr-stage-node__copy">
              <span><small>{index + 1}</small><strong>{labels.phaseNames[item.id]}</strong></span>
              <em>{labels.phaseState(item.state)}</em>
              <span className="vr-stage-node__detail">{item.detail}</span>
            </span>
          </motion.button>
        </li>;
      })}
    </ol>
  </div>;
}
