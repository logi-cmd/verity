// SPDX-License-Identifier: MPL-2.0

import { useLayoutEffect, useRef, useState } from "react";
import { AlertTriangle } from "lucide-react";

function clamp(value, minimum, maximum) {
  return Math.min(maximum, Math.max(minimum, value));
}

export function BlockerBranch({ phase, label, summary, onActivate }) {
  const rootRef = useRef(null);
  const [layout, setLayout] = useState(null);

  useLayoutEffect(() => {
    const root = rootRef.current;
    const surface = root?.closest(".vr-verification-stage__path");
    if (!root || !surface || !phase) return undefined;

    const measure = () => {
      const orb = surface.querySelector(`[data-phase-id="${phase}"] .vr-stage-node__orb`);
      if (!orb) return;
      const surfaceBounds = surface.getBoundingClientRect();
      const orbBounds = orb.getBoundingClientRect();
      const anchorX = orbBounds.left + orbBounds.width / 2 - surfaceBounds.left;
      const anchorY = orbBounds.top + orbBounds.height / 2 - surfaceBounds.top;
      const labelWidth = Math.min(250, Math.max(190, surfaceBounds.width * 0.27));
      const placeLeft = anchorX > labelWidth + 90;
      const labelX = clamp(
        placeLeft ? anchorX - labelWidth - 118 : anchorX + 96,
        20,
        surfaceBounds.width - labelWidth - 20,
      );
      const labelY = clamp(anchorY + 58, 22, surfaceBounds.height - 66);
      setLayout({ anchorX, anchorY, labelX, labelY, labelWidth, placeLeft });
    };

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(surface);
    const orb = surface.querySelector(`[data-phase-id="${phase}"] .vr-stage-node__orb`);
    if (orb) observer.observe(orb);
    globalThis.addEventListener("resize", measure);
    return () => {
      observer.disconnect();
      globalThis.removeEventListener("resize", measure);
    };
  }, [phase]);

  const endX = layout ? (layout.placeLeft ? layout.labelX + layout.labelWidth : layout.labelX) : 0;
  const endY = layout ? layout.labelY + 20 : 0;
  const controlX = layout ? layout.anchorX + (endX - layout.anchorX) * 0.58 : 0;
  const path = layout
    ? `M ${layout.anchorX} ${layout.anchorY} C ${controlX} ${layout.anchorY + 10}, ${controlX} ${endY}, ${endX} ${endY}`
    : "";

  return <div ref={rootRef} className="vr-blocker-branch" data-phase={phase}>
    {layout ? <>
      <svg className="vr-blocker-branch__line" aria-hidden="true">
        <path d={path} />
        <circle cx={layout.anchorX} cy={layout.anchorY} r="3" />
      </svg>
      <button
        type="button"
        className="vr-blocker-branch__label"
        data-action-id="blocker.focus"
        style={{ left: layout.labelX, top: layout.labelY, width: layout.labelWidth }}
        onClick={onActivate}
      >
        <AlertTriangle aria-hidden="true" />
        <span><strong>{label}</strong><small>{summary}</small></span>
      </button>
    </> : null}
  </div>;
}
