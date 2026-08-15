// SPDX-License-Identifier: MPL-2.0

import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import { Color, Geometry, Mesh, Post, Program, Renderer, Transform, Vec2 } from "ogl";
import { PHASE_POSITIONS } from "./VerificationEvidencePath.jsx";
import {
  createEvidenceDotField,
  createEvidenceStaticPaths,
  EVIDENCE_TERRAIN_LATERAL,
  EVIDENCE_TERRAIN_VERTICAL,
} from "./evidenceDotFieldGeometry.js";
import {
  EVIDENCE_EVENT_KIND,
  EVIDENCE_EVENT_SPEC,
  normalizeEvidenceEventProgress,
  resolveEvidenceEvent,
} from "./evidenceFieldMotion.js";
import { degradeEvidenceFieldQuality } from "./evidenceFieldQuality.js";

const SELECTION_COMPRESSION_END = EVIDENCE_EVENT_SPEC.selection.compressionEnd / EVIDENCE_EVENT_SPEC.selection.duration;
const SELECTION_WAVE_END = EVIDENCE_EVENT_SPEC.selection.waveEnd / EVIDENCE_EVENT_SPEC.selection.duration;

const FIELD_DEFORMATION = `
  uniform vec2 uFocus;
  uniform vec2 uHoverFocus;
  uniform float uAspect;
  uniform float uStaticEnergy;
  uniform float uEventKind;
  uniform float uEventProgress;
  uniform float uEventRadius;
  uniform float uEventWidth;
  uniform float uHoverRadius;
  uniform float uHoverDisplacement;
  uniform float uHoverNodeMix;
  uniform float uTerrainVertical;
  uniform float uTerrainLateral;
  uniform vec2 uAnchor0;
  uniform vec2 uAnchor1;
  uniform vec2 uAnchor2;
  uniform vec2 uAnchor3;
  uniform vec2 uAnchor4;
  uniform vec2 uAnchor5;

  vec2 measuredDelta(vec2 point, vec2 target) {
    vec2 delta = point - target;
    delta.x *= uAspect;
    return delta;
  }

  vec2 fieldPosition(vec2 source, vec3 fieldStyle, vec3 terrain, out float staticInfluence, out float eventInfluence, out float eventScale, out float hoverInfluence, out float terrainLight) {
    vec2 point = source + vec2(terrain.y * uTerrainLateral, terrain.x * uTerrainVertical + terrain.z * uTerrainLateral * 0.28);
    terrainLight = clamp(0.44 + terrain.x * 0.22 - terrain.y * 0.46 + terrain.z * 0.34, 0.04, 1.0);
    vec2 measured = measuredDelta(point, uFocus);
    float distanceToFocus = length(measured);
    vec2 direction = distanceToFocus > 0.0001 ? normalize(measured) : vec2(0.0);
    direction.x /= max(uAspect, 0.001);
    vec2 displacement = vec2(0.0);
    float fieldAngle = atan(measured.y, measured.x);
    float radialFold = 0.72 + 0.28 * cos(fieldAngle * 14.0 + terrain.x * 2.6 + fieldStyle.z * 0.8);
    staticInfluence = exp(-distanceToFocus * 6.2) * radialFold;
    eventInfluence = 0.0;
    eventScale = 0.0;
    vec2 tangent = vec2(-direction.y, direction.x);
    displacement -= direction * staticInfluence * uStaticEnergy * 0.048 * (0.76 + fieldStyle.z * 0.24);
    displacement += tangent * staticInfluence * uStaticEnergy * sin(fieldAngle * 7.0 + terrain.z * 2.2) * 0.006;

    vec2 hoverMeasured = measuredDelta(point, uHoverFocus);
    float hoverDistance = length(hoverMeasured);
    vec2 hoverDirection = hoverDistance > 0.0001 ? normalize(hoverMeasured) : vec2(0.0);
    hoverDirection.x /= max(uAspect, 0.001);
    hoverInfluence = exp(-pow(hoverDistance / max(uHoverRadius, 0.0001), 2.0) * 2.15) * step(0.00001, uHoverDisplacement);
    displacement -= hoverDirection * hoverInfluence * uHoverDisplacement * (0.72 + fieldStyle.z * 0.28);

    if (uEventKind > 0.5 && uEventKind < 1.5) {
      float compressionProgress = clamp(uEventProgress / ${SELECTION_COMPRESSION_END.toFixed(6)}, 0.0, 1.0);
      float compression = sin(compressionProgress * 3.14159265) * (1.0 - step(${SELECTION_COMPRESSION_END.toFixed(6)}, uEventProgress));
      float waveProgress = clamp((uEventProgress - ${SELECTION_COMPRESSION_END.toFixed(6)}) / ${(SELECTION_WAVE_END - SELECTION_COMPRESSION_END).toFixed(6)}, 0.0, 1.0);
      float secondProgress = clamp((waveProgress - 0.18) / 0.82, 0.0, 1.0);
      float recovery = 1.0 - smoothstep(${SELECTION_WAVE_END.toFixed(6)}, 1.0, uEventProgress);
      float firstRadius = uEventRadius * smoothstep(0.0, 1.0, waveProgress);
      float secondRadius = uEventRadius * 0.82 * smoothstep(0.0, 1.0, secondProgress);
      float firstRing = exp(-pow((distanceToFocus - firstRadius) / max(uEventWidth, 0.001), 2.0) * 2.25) * recovery;
      float secondRing = exp(-pow((distanceToFocus - secondRadius) / max(uEventWidth * 0.82, 0.001), 2.0) * 2.45) * recovery * step(0.001, secondProgress);
      float compressionField = exp(-distanceToFocus * 11.0) * compression;
      displacement -= direction * compressionField * 0.011;
      displacement += direction * (firstRing * 0.014 + secondRing * 0.009) * (0.82 + fieldStyle.z * 0.22);
      eventInfluence = max(max(firstRing, secondRing * 0.84), compressionField * 0.52);
      eventScale = firstRing * 1.05 + secondRing * 0.72 + compressionField * 0.28;
    } else if (uEventKind > 1.5 && uEventKind < 2.5) {
      float envelope = sin(uEventProgress * 3.14159265);
      float radius = uEventRadius * smoothstep(0.0, 1.0, uEventProgress);
      float ring = exp(-pow((distanceToFocus - radius) / max(uEventWidth, 0.001), 2.0) * 2.4) * envelope;
      displacement += direction * ring * 0.007;
      eventInfluence = ring * 0.58;
      eventScale = ring * 0.62;
    } else if (uEventKind > 2.5 && uEventKind < 3.5) {
      float envelope = sin(uEventProgress * 3.14159265);
      float collapse = exp(-distanceToFocus * 7.0) * envelope;
      displacement -= direction * collapse * 0.025;
      eventInfluence = collapse;
      eventScale = collapse * 0.72;
    } else if (uEventKind > 3.5) {
      vec2 bestDelta = measuredDelta(point, uAnchor0);
      float bestDistance = length(bestDelta);
      vec2 candidate = measuredDelta(point, uAnchor1);
      if (length(candidate) < bestDistance) { bestDelta = candidate; bestDistance = length(candidate); }
      candidate = measuredDelta(point, uAnchor2);
      if (length(candidate) < bestDistance) { bestDelta = candidate; bestDistance = length(candidate); }
      candidate = measuredDelta(point, uAnchor3);
      if (length(candidate) < bestDistance) { bestDelta = candidate; bestDistance = length(candidate); }
      candidate = measuredDelta(point, uAnchor4);
      if (length(candidate) < bestDistance) { bestDelta = candidate; bestDistance = length(candidate); }
      candidate = measuredDelta(point, uAnchor5);
      if (length(candidate) < bestDistance) { bestDelta = candidate; bestDistance = length(candidate); }
      vec2 towardPath = bestDistance > 0.0001 ? -normalize(bestDelta) : vec2(0.0);
      towardPath.x /= max(uAspect, 0.001);
      float envelope = sin(uEventProgress * 3.14159265);
      float convergence = exp(-bestDistance * 8.0) * envelope;
      displacement += towardPath * convergence * 0.017;
      eventInfluence = convergence * 0.82;
      eventScale = convergence * 0.62;
    }

    return point + displacement;
  }
`;

const DOT_VERTEX = `
  attribute vec2 position;
  attribute vec3 fieldStyle;
  attribute vec3 terrain;
  varying float vAlpha;
  varying float vTintMix;
  uniform float uDpr;
  ${FIELD_DEFORMATION}

  void main() {
    float staticInfluence;
    float eventInfluence;
    float eventScale;
    float hoverInfluence;
    float terrainLight;
    vec2 point = fieldPosition(position, fieldStyle, terrain, staticInfluence, eventInfluence, eventScale, hoverInfluence, terrainLight);
    float edgeDistance = min(min(point.x, 1.0 - point.x), min(point.y, 1.0 - point.y));
    float edgeFactor = mix(0.58, 1.0, smoothstep(0.0, 0.12, edgeDistance));
    vTintMix = clamp(staticInfluence * uStaticEnergy * 0.7 + eventInfluence * 0.94 + hoverInfluence * (0.08 + uHoverNodeMix * 0.2), 0.0, 0.96);
    vAlpha = edgeFactor * fieldStyle.y * (0.3 + terrainLight * 0.2 + staticInfluence * uStaticEnergy * 0.32 + eventInfluence * 0.56 + hoverInfluence * 0.2);
    gl_PointSize = max(1.0, fieldStyle.x * uDpr * (0.96 + terrainLight * 0.24 + staticInfluence * uStaticEnergy * 0.38 + eventScale + hoverInfluence * (0.16 + uHoverNodeMix * 0.3)));
    gl_Position = vec4(point.x * 2.0 - 1.0, 1.0 - point.y * 2.0, 0.0, 1.0);
  }
`;

const LINE_VERTEX = `
  attribute vec2 position;
  attribute vec3 fieldStyle;
  attribute vec3 terrain;
  varying float vAlpha;
  varying float vTintMix;
  ${FIELD_DEFORMATION}

  void main() {
    float staticInfluence;
    float eventInfluence;
    float eventScale;
    float hoverInfluence;
    float terrainLight;
    vec2 point = fieldPosition(position, fieldStyle, terrain, staticInfluence, eventInfluence, eventScale, hoverInfluence, terrainLight);
    float edgeDistance = min(min(point.x, 1.0 - point.x), min(point.y, 1.0 - point.y));
    float edgeFactor = mix(0.56, 1.0, smoothstep(0.0, 0.14, edgeDistance));
    vTintMix = clamp(staticInfluence * uStaticEnergy * 0.58 + eventInfluence * 0.86 + hoverInfluence * (0.06 + uHoverNodeMix * 0.16), 0.0, 0.9);
    vAlpha = edgeFactor * (0.03 + terrainLight * 0.052 + fieldStyle.y * 0.02 + staticInfluence * uStaticEnergy * 0.075 + eventInfluence * 0.16 + hoverInfluence * 0.066);
    gl_Position = vec4(point.x * 2.0 - 1.0, 1.0 - point.y * 2.0, 0.0, 1.0);
  }
`;

const FIELD_FRAGMENT = `
  precision highp float;
  varying float vAlpha;
  varying float vTintMix;
  uniform vec3 uTint;
  void main() {
    vec3 graphite = vec3(0.36, 0.4, 0.5);
    vec3 color = mix(graphite, uTint, vTintMix);
    gl_FragColor = vec4(color * vAlpha, vAlpha);
  }
`;

const DOT_FRAGMENT = `
  precision highp float;
  varying float vAlpha;
  varying float vTintMix;
  uniform vec3 uTint;
  void main() {
    float distanceToCenter = length(gl_PointCoord - vec2(0.5));
    float alpha = (1.0 - smoothstep(0.16, 0.5, distanceToCenter)) * vAlpha;
    vec3 graphite = vec3(0.49, 0.54, 0.66);
    vec3 color = mix(graphite, uTint, vTintMix);
    gl_FragColor = vec4(color * alpha, alpha);
  }
`;

const POST_FRAGMENT = `
  precision highp float;
  uniform sampler2D tMap;
  uniform vec2 uTexel;
  varying vec2 vUv;
  vec3 glowAt(vec2 position) {
    vec3 sampleColor = texture2D(tMap, position).rgb;
    return max(sampleColor - vec3(0.16), vec3(0.0));
  }
  void main() {
    vec4 center = texture2D(tMap, vUv);
    vec3 bloom = vec3(0.0);
    bloom += glowAt(vUv + uTexel * vec2(1.5, 0.0));
    bloom += glowAt(vUv - uTexel * vec2(1.5, 0.0));
    bloom += glowAt(vUv + uTexel * vec2(0.0, 1.5));
    bloom += glowAt(vUv - uTexel * vec2(0.0, 1.5));
    bloom += glowAt(vUv + uTexel * vec2(1.1, 1.1));
    bloom += glowAt(vUv + uTexel * vec2(-1.1, 1.1));
    bloom += glowAt(vUv + uTexel * vec2(1.1, -1.1));
    bloom += glowAt(vUv - uTexel * vec2(1.1, 1.1));
    bloom *= 0.125;
    gl_FragColor = vec4(center.rgb + bloom * 0.28, center.a);
  }
`;

const POST_VERTEX = `
  attribute vec2 uv;
  attribute vec2 position;
  varying vec2 vUv;
  void main() {
    vUv = uv;
    gl_Position = vec4(position, 0.0, 1.0);
  }
`;

function focusForPhase(phase) {
  const point = PHASE_POSITIONS[phase] || PHASE_POSITIONS.plan;
  return [point.x / 720, point.y / 690];
}

function statusTint(status) {
  if (status === "blocked") return "#e6ad61";
  if (status === "verified") return "#61d6a0";
  return "#8d7cff";
}

function eventTint(eventName, status) {
  if (eventName === "blocked") return "#e6ad61";
  if (eventName === "verified") return "#61d6a0";
  if (eventName === "selection" || eventName === "heartbeat") return "#8d7cff";
  return statusTint(status);
}

function percentile95(values) {
  if (!values.length) return 0;
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * 0.95) - 1)];
}

function removeRenderTarget(gl, target) {
  if (!target) return;
  for (const texture of target.textures || []) gl.deleteTexture(texture.texture);
  if (target.depthTexture) gl.deleteTexture(target.depthTexture.texture);
  if (target.depthBuffer) gl.deleteRenderbuffer(target.depthBuffer);
  if (target.stencilBuffer) gl.deleteRenderbuffer(target.stencilBuffer);
  if (target.depthStencilBuffer) gl.deleteRenderbuffer(target.depthStencilBuffer);
  gl.deleteFramebuffer(target.buffer);
}

function eased(progress) {
  return 1 - ((1 - progress) ** 3);
}

export const ElasticEvidenceField = forwardRef(function ElasticEvidenceField({ phase, status, heartbeat, motionProfile, paused = false }, ref) {
  const hostRef = useRef(null);
  const renderRef = useRef(null);
  const pendingSelectionRef = useRef(null);
  const latestRef = useRef({ phase, status });
  const previousEventRef = useRef({ phase, status, heartbeat });
  const field = useMemo(() => createEvidenceDotField(), []);
  const staticPaths = useMemo(() => createEvidenceStaticPaths(field), [field]);
  const [materialQuality, setMaterialQuality] = useState(motionProfile === "reduced" ? "static" : "full");
  latestRef.current = { phase, status };

  useImperativeHandle(ref, () => ({
    activateSelection(nextPhase, anchorElement) {
      const event = { nextPhase, nextStatus: latestRef.current.status, eventName: "selection", anchorElement, immediate: true };
      if (renderRef.current) renderRef.current(event);
      else pendingSelectionRef.current = event;
    },
  }), []);

  useEffect(() => {
    const host = hostRef.current;
    if (!host || motionProfile === "reduced") {
      setMaterialQuality("static");
      return undefined;
    }

    let renderer;
    try {
      renderer = new Renderer({
        alpha: true,
        antialias: false,
        premultipliedAlpha: true,
        preserveDrawingBuffer: true,
        dpr: Math.min(globalThis.devicePixelRatio || 1, 1.5),
        webgl: 1,
      });
    } catch {
      host.dataset.webgl = "unavailable";
      setMaterialQuality("static");
      return undefined;
    }

    const gl = renderer.gl;
    gl.clearColor(0, 0, 0, 0);
    gl.canvas.setAttribute("aria-hidden", "true");
    host.appendChild(gl.canvas);
    host.dataset.dotField = field.signature;

    const scene = new Transform();
    const anchors = Object.values(PHASE_POSITIONS).map((point) => new Vec2(point.x / 720, point.y / 690));
    const sharedUniforms = {
      uFocus: { value: new Vec2(0.5, 0.5) },
      uHoverFocus: { value: new Vec2(0.5, 0.5) },
      uAspect: { value: 1 },
      uStaticEnergy: { value: 0 },
      uEventKind: { value: EVIDENCE_EVENT_KIND.none },
      uEventProgress: { value: 0 },
      uEventRadius: { value: 0 },
      uEventWidth: { value: 0.025 },
      uHoverRadius: { value: 0.1 },
      uHoverDisplacement: { value: 0 },
      uHoverNodeMix: { value: 0 },
      uTerrainVertical: { value: EVIDENCE_TERRAIN_VERTICAL },
      uTerrainLateral: { value: EVIDENCE_TERRAIN_LATERAL },
      uTint: { value: new Color("#8d7cff") },
      uDpr: { value: renderer.dpr },
      uAnchor0: { value: anchors[0] },
      uAnchor1: { value: anchors[1] },
      uAnchor2: { value: anchors[2] },
      uAnchor3: { value: anchors[3] },
      uAnchor4: { value: anchors[4] },
      uAnchor5: { value: anchors[5] },
    };

    const lineGeometry = new Geometry(gl, {
      position: { size: 2, data: field.edgePositions },
      fieldStyle: { size: 3, data: field.edgeStyles },
      terrain: { size: 3, data: field.edgeTerrain },
    });
    const lineProgram = new Program(gl, {
      vertex: LINE_VERTEX,
      fragment: FIELD_FRAGMENT,
      uniforms: sharedUniforms,
      transparent: true,
      cullFace: null,
      depthTest: false,
      depthWrite: false,
    });
    const fieldLines = new Mesh(gl, { geometry: lineGeometry, program: lineProgram, mode: gl.LINES, frustumCulled: false, renderOrder: 0 });
    fieldLines.setParent(scene);

    const dotGeometry = new Geometry(gl, {
      position: { size: 2, data: field.dotPositions },
      fieldStyle: { size: 3, data: field.dotStyles },
      terrain: { size: 3, data: field.dotTerrain },
    });
    const dotProgram = new Program(gl, {
      vertex: DOT_VERTEX,
      fragment: DOT_FRAGMENT,
      uniforms: sharedUniforms,
      transparent: true,
      cullFace: null,
      depthTest: false,
      depthWrite: false,
    });
    const fieldDots = new Mesh(gl, { geometry: dotGeometry, program: dotProgram, mode: gl.POINTS, frustumCulled: false, renderOrder: 1 });
    fieldDots.setParent(scene);

    const texParameteri = gl.texParameteri.bind(gl);
    gl.texParameteri = (target, parameter, value) => {
      if (target === gl.TEXTURE_2D && parameter === gl.TEXTURE_WRAP_R) return;
      texParameteri(target, parameter, value);
    };
    let post;
    try {
      post = new Post(gl, { dpr: renderer.dpr, depth: false });
    } finally {
      gl.texParameteri = texParameteri;
    }
    const postPass = post.addPass({
      vertex: POST_VERTEX,
      fragment: POST_FRAGMENT,
      uniforms: { uTexel: { value: new Vec2(1, 1) } },
    });

    const interactionSurface = host.closest(".vr-verification-stage__path");
    const finePointer = globalThis.matchMedia?.("(hover: hover) and (pointer: fine)");
    const phaseControlFor = (nextPhase) => interactionSurface?.querySelector(`[data-phase-id="${nextPhase}"]`) || null;
    const focusFromElement = (anchorElement) => {
      const orb = anchorElement?.matches?.(".vr-stage-node__orb") ? anchorElement : anchorElement?.querySelector?.(".vr-stage-node__orb");
      if (!orb) return null;
      const hostBounds = host.getBoundingClientRect();
      const orbBounds = orb.getBoundingClientRect();
      if (!hostBounds.width || !hostBounds.height || !orbBounds.width || !orbBounds.height) return null;
      return [
        Math.min(1, Math.max(0, (orbBounds.left + orbBounds.width / 2 - hostBounds.left) / hostBounds.width)),
        Math.min(1, Math.max(0, (orbBounds.top + orbBounds.height / 2 - hostBounds.top) / hostBounds.height)),
      ];
    };
    const liveFocusForPhase = (nextPhase, anchorElement) => focusFromElement(anchorElement || phaseControlFor(nextPhase)) || focusForPhase(nextPhase);
    const updateLiveAnchors = () => {
      Object.keys(PHASE_POSITIONS).forEach((phaseId, index) => {
        const [x, y] = liveFocusForPhase(phaseId);
        sharedUniforms[`uAnchor${index}`].value.set(x, y);
      });
    };
    let frame = 0;
    let hidden = document.hidden;
    let contextLost = false;
    let quality = "full";
    let activeEvent = "none";
    let eventStartedAt = 0;
    let eventDuration = 0;
    let eventRadiusPx = 0;
    let sampleCosts = [];
    let hoverStartedAt = 0;
    let hoverAnimating = false;
    let hoverFrom = { x: 0.5, y: 0.5, radius: 0, displacement: 0, nodeMix: 0 };
    let hoverCurrent = { ...hoverFrom };
    let hoverTarget = { ...hoverFrom };
    let currentFocusPhase = phase;
    let eventSerial = 0;

    const updateQuality = (nextQuality) => {
      if (quality === nextQuality) return;
      quality = nextQuality;
      host.dataset.materialQuality = nextQuality;
      setMaterialQuality(nextQuality);
      if (nextQuality === "compact") {
        renderer.dpr = Math.min(globalThis.devicePixelRatio || 1, 1);
        sharedUniforms.uDpr.value = renderer.dpr;
      }
    };

    const staticEnergyForStatus = (nextStatus) => {
      if (nextStatus === "blocked") return 0.68;
      if (nextStatus === "running") return 0.86;
      return 0;
    };

    const renderScene = () => {
      if (quality === "static") return;
      const started = performance.now();
      if (quality === "full") post.render({ scene });
      else renderer.render({ scene });
      const cost = performance.now() - started;
      sampleCosts.push(cost);
      if (sampleCosts.length >= 36) {
        const nextQuality = degradeEvidenceFieldQuality(quality, percentile95(sampleCosts));
        sampleCosts = [];
        if (nextQuality !== quality) {
          updateQuality(nextQuality);
          if (nextQuality === "static") {
            cancelAnimationFrame(frame);
            frame = 0;
          } else {
            resize();
          }
        }
      }
    };

    const resize = () => {
      if (quality === "static") return;
      const width = Math.max(1, host.clientWidth);
      const height = Math.max(1, host.clientHeight);
      renderer.setSize(width, height);
      sharedUniforms.uAspect.value = width / height;
      updateLiveAnchors();
      const [focusX, focusY] = liveFocusForPhase(currentFocusPhase);
      sharedUniforms.uFocus.value.set(focusX, focusY);
      sharedUniforms.uEventRadius.value = eventRadiusPx / height;
      sharedUniforms.uEventWidth.value = (activeEvent === "selection" ? 20 : 16) / height;
      sharedUniforms.uHoverRadius.value = hoverCurrent.radius / height;
      sharedUniforms.uHoverDisplacement.value = hoverCurrent.displacement / height;
      post.resize({ width, height, dpr: renderer.dpr });
      postPass.uniforms.uTexel.value.set(1 / Math.max(1, renderer.width), 1 / Math.max(1, renderer.height));
      renderScene();
    };
    const resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(host);

    const applyHover = (now) => {
      if (!hoverAnimating) return false;
      const progress = Math.min(1, Math.max(0, (now - hoverStartedAt) / 170));
      const amount = eased(progress);
      for (const key of ["x", "y", "radius", "displacement", "nodeMix"]) {
        hoverCurrent[key] = hoverFrom[key] + (hoverTarget[key] - hoverFrom[key]) * amount;
      }
      sharedUniforms.uHoverFocus.value.set(hoverCurrent.x, hoverCurrent.y);
      sharedUniforms.uHoverRadius.value = hoverCurrent.radius / Math.max(1, host.clientHeight);
      sharedUniforms.uHoverDisplacement.value = hoverCurrent.displacement / Math.max(1, host.clientHeight);
      sharedUniforms.uHoverNodeMix.value = hoverCurrent.nodeMix;
      if (progress >= 1) hoverAnimating = false;
      return hoverAnimating;
    };

    const animate = (now) => {
      frame = 0;
      if (hidden || paused || contextLost || quality === "static") return;
      const hoverActive = applyHover(now);
      let eventActive = activeEvent !== "none";
      if (eventActive) {
        const normalized = normalizeEvidenceEventProgress(eventStartedAt, now, eventDuration);
        sharedUniforms.uEventProgress.value = normalized;
        if (normalized >= 1) {
          const completedEvent = activeEvent;
          activeEvent = "none";
          eventActive = false;
          host.dataset.event = "none";
          sharedUniforms.uEventKind.value = EVIDENCE_EVENT_KIND.none;
          sharedUniforms.uEventProgress.value = 0;
          sharedUniforms.uTint.value.set(statusTint(latestRef.current.status));
          if (completedEvent === "selection") {
            currentFocusPhase = latestRef.current.phase;
            const [focusX, focusY] = liveFocusForPhase(currentFocusPhase);
            sharedUniforms.uFocus.value.set(focusX, focusY);
          }
        }
      }
      renderScene();
      if (eventActive || hoverActive) {
        host.dataset.animating = "true";
        frame = requestAnimationFrame(animate);
      } else {
        host.dataset.animating = "false";
      }
    };

    const scheduleFrame = () => {
      if (!frame && !hidden && !paused && !contextLost && quality !== "static") frame = requestAnimationFrame(animate);
    };

    const setHoverTarget = (nextTarget) => {
      const wasInactive = hoverCurrent.displacement <= 0.001 && hoverTarget.displacement <= 0.001;
      hoverCurrent.x = nextTarget.x;
      hoverCurrent.y = nextTarget.y;
      if (wasInactive && nextTarget.displacement > 0) {
        hoverCurrent.radius = nextTarget.radius * 0.58;
        hoverCurrent.displacement = nextTarget.displacement * 0.58;
        hoverCurrent.nodeMix = nextTarget.nodeMix * 0.58;
      }
      hoverFrom = { ...hoverCurrent };
      hoverTarget = nextTarget;
      hoverStartedAt = performance.now();
      hoverAnimating = true;
      sharedUniforms.uHoverFocus.value.set(hoverCurrent.x, hoverCurrent.y);
      sharedUniforms.uHoverRadius.value = hoverCurrent.radius / Math.max(1, host.clientHeight);
      sharedUniforms.uHoverDisplacement.value = hoverCurrent.displacement / Math.max(1, host.clientHeight);
      sharedUniforms.uHoverNodeMix.value = hoverCurrent.nodeMix;
      if (wasInactive && nextTarget.displacement > 0) renderScene();
      scheduleFrame();
    };

    const normalizedPointer = (event) => {
      const bounds = host.getBoundingClientRect();
      return {
        x: Math.min(1, Math.max(0, (event.clientX - bounds.left) / Math.max(1, bounds.width))),
        y: Math.min(1, Math.max(0, (event.clientY - bounds.top) / Math.max(1, bounds.height))),
      };
    };

    const onPointerMove = (event) => {
      if (!finePointer?.matches || event.pointerType === "touch") return;
      const phaseControl = event.target instanceof Element ? event.target.closest("[data-phase-id]") : null;
      if (phaseControl) {
        const [x, y] = liveFocusForPhase(phaseControl.dataset.phaseId, phaseControl);
        setHoverTarget({ x, y, radius: 120, displacement: 11, nodeMix: 1 });
      } else {
        const point = normalizedPointer(event);
        setHoverTarget({ ...point, radius: 80, displacement: 5, nodeMix: 0 });
      }
    };
    const onPointerLeave = () => setHoverTarget({ ...hoverCurrent, radius: 0, displacement: 0, nodeMix: 0 });

    renderRef.current = ({ nextPhase, nextStatus, eventName = "none", anchorElement = null, immediate = false }) => {
      currentFocusPhase = nextPhase;
      const [x, y] = liveFocusForPhase(nextPhase, anchorElement);
      host.dataset.eventX = x.toFixed(6);
      host.dataset.eventY = y.toFixed(6);
      if (quality === "static") return;
      sharedUniforms.uFocus.value.set(x, y);
      sharedUniforms.uStaticEnergy.value = staticEnergyForStatus(nextStatus);
      sharedUniforms.uTint.value.set(eventTint(eventName, nextStatus));
      activeEvent = eventName;
      host.dataset.event = eventName;
      host.dataset.eventFocus = nextPhase;
      const spec = EVIDENCE_EVENT_SPEC[eventName];
      if (!spec) {
        sharedUniforms.uEventKind.value = EVIDENCE_EVENT_KIND.none;
        sharedUniforms.uEventProgress.value = 0;
        renderScene();
        return;
      }
      eventSerial += 1;
      host.dataset.eventSerial = String(eventSerial);
      eventDuration = spec.duration;
      eventRadiusPx = spec.radius;
      sharedUniforms.uEventKind.value = EVIDENCE_EVENT_KIND[eventName];
      sharedUniforms.uEventProgress.value = 0.001;
      sharedUniforms.uEventRadius.value = eventRadiusPx / Math.max(1, host.clientHeight);
      sharedUniforms.uEventWidth.value = (eventName === "selection" ? 20 : 16) / Math.max(1, host.clientHeight);
      const now = performance.now();
      eventStartedAt = now - (immediate ? 16 : 0);
      if (immediate) {
        sharedUniforms.uEventProgress.value = normalizeEvidenceEventProgress(eventStartedAt, now, eventDuration);
        host.dataset.animating = "true";
        renderScene();
      }
      scheduleFrame();
    };

    const onVisibility = () => {
      hidden = document.hidden;
      if (hidden) {
        cancelAnimationFrame(frame);
        frame = 0;
        activeEvent = "none";
        hoverAnimating = false;
        hoverCurrent = { x: 0.5, y: 0.5, radius: 0, displacement: 0, nodeMix: 0 };
        hoverFrom = { ...hoverCurrent };
        hoverTarget = { ...hoverCurrent };
        sharedUniforms.uEventKind.value = EVIDENCE_EVENT_KIND.none;
        sharedUniforms.uEventProgress.value = 0;
        sharedUniforms.uHoverDisplacement.value = 0;
        sharedUniforms.uHoverNodeMix.value = 0;
        host.dataset.animating = "false";
        return;
      }
      const latest = latestRef.current;
      renderRef.current?.({ nextPhase: latest.phase, nextStatus: latest.status });
    };
    const onContextLost = (event) => {
      event.preventDefault();
      contextLost = true;
      cancelAnimationFrame(frame);
      frame = 0;
      host.dataset.webgl = "lost";
      host.dataset.animating = "false";
      updateQuality("static");
    };

    document.addEventListener("visibilitychange", onVisibility);
    gl.canvas.addEventListener("webglcontextlost", onContextLost);
    interactionSurface?.addEventListener("pointermove", onPointerMove, { passive: true });
    interactionSurface?.addEventListener("pointerleave", onPointerLeave, { passive: true });
    host.dataset.webgl = "ready";
    host.dataset.event = "none";
    host.dataset.animating = "false";
    host.dataset.materialQuality = quality;
    setMaterialQuality(quality);
    resize();
    renderRef.current({ nextPhase: phase, nextStatus: status });
    if (pendingSelectionRef.current) {
      const pendingSelection = pendingSelectionRef.current;
      pendingSelectionRef.current = null;
      renderRef.current(pendingSelection);
    }

    return () => {
      renderRef.current = null;
      pendingSelectionRef.current = null;
      cancelAnimationFrame(frame);
      resizeObserver.disconnect();
      document.removeEventListener("visibilitychange", onVisibility);
      gl.canvas.removeEventListener("webglcontextlost", onContextLost);
      interactionSurface?.removeEventListener("pointermove", onPointerMove);
      interactionSurface?.removeEventListener("pointerleave", onPointerLeave);
      dotGeometry.remove();
      lineGeometry.remove();
      dotProgram.remove();
      lineProgram.remove();
      for (const pass of post.passes) pass.program.remove();
      post.geometry.remove();
      removeRenderTarget(gl, post.fbo.read);
      removeRenderTarget(gl, post.fbo.write);
      if (gl.canvas.parentElement === host) host.removeChild(gl.canvas);
      gl.getExtension("WEBGL_lose_context")?.loseContext();
    };
  }, [field, motionProfile, paused]);

  useEffect(() => {
    const previous = previousEventRef.current;
    const heartbeatChanged = status === "running" && Boolean(heartbeat && heartbeat !== previous.heartbeat);
    const statusChanged = status !== previous.status;
    const eventName = resolveEvidenceEvent({ interactionChanged: false, heartbeatChanged, statusChanged, status });
    if (eventName !== "none" || statusChanged) {
      renderRef.current?.({ nextPhase: phase, nextStatus: status, eventName });
    }
    previousEventRef.current = { phase, status, heartbeat };
  }, [heartbeat, phase, status]);

  return <div ref={hostRef} className="vr-elastic-field" data-material-quality={materialQuality} aria-hidden="true">
    <svg className="vr-elastic-field__static" viewBox={staticPaths.viewBox} preserveAspectRatio="none" focusable="false">
      <path className="vr-elastic-field__static-lines" d={staticPaths.edges} />
      <path className="vr-elastic-field__static-dots" d={staticPaths.dots} />
    </svg>
  </div>;
});
