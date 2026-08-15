// SPDX-License-Identifier: MPL-2.0

import Delaunator from "delaunator";

export const EVIDENCE_DOT_FIELD_SEED = 0x56455249;
export const EVIDENCE_DOT_COLUMNS = 64;
export const EVIDENCE_DOT_ROWS = 42;
export const EVIDENCE_DOT_JITTER = 0.18;
export const EVIDENCE_TERRAIN_VERTICAL = 0.05;
export const EVIDENCE_TERRAIN_LATERAL = 0.018;

function seededRandom(seed) {
  let value = seed >>> 0;
  return () => {
    value += 0x6d2b79f5;
    let mixed = value;
    mixed = Math.imul(mixed ^ (mixed >>> 15), mixed | 1);
    mixed ^= mixed + Math.imul(mixed ^ (mixed >>> 7), mixed | 61);
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4294967296;
  };
}

function hashTypedArrays(arrays) {
  let hash = 0x811c9dc5;
  for (const array of arrays) {
    const bytes = new Uint8Array(array.buffer, array.byteOffset, array.byteLength);
    for (const byte of bytes) {
      hash ^= byte;
      hash = Math.imul(hash, 0x01000193);
    }
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

function anisotropicFold(x, y, centerX, centerY, angle, width, length) {
  const dx = x - centerX;
  const dy = y - centerY;
  const cosine = Math.cos(angle);
  const sine = Math.sin(angle);
  const across = -dx * sine + dy * cosine;
  const along = dx * cosine + dy * sine;
  return Math.exp(-((across * across) / (width * width) + (along * along) / (length * length)));
}

function terrainHeight(x, y) {
  const primaryRidge = anisotropicFold(x, y, 0.42, 0.48, -0.58, 0.105, 0.72) * 0.92;
  const upperRidge = anisotropicFold(x, y, 0.29, 0.2, -0.24, 0.12, 0.46) * 0.44;
  const lowerShelf = anisotropicFold(x, y, 0.61, 0.76, -0.8, 0.16, 0.48) * 0.36;
  const centralValley = anisotropicFold(x, y, 0.57, 0.49, -0.56, 0.09, 0.6) * 0.72;
  const broadUndulation = Math.sin((x * 0.68 + y * 0.34) * Math.PI * 2 - 0.82) * 0.16;
  const crossSlope = (0.5 - y) * 0.12 + (x - 0.5) * 0.08;
  return Math.max(-1, Math.min(1, primaryRidge + upperRidge + lowerShelf - centralValley + broadUndulation + crossSlope - 0.2));
}

function terrainSample(x, y) {
  const epsilon = 0.002;
  const height = terrainHeight(x, y);
  const slopeX = (terrainHeight(Math.min(1, x + epsilon), y) - terrainHeight(Math.max(0, x - epsilon), y)) / (epsilon * 2);
  const slopeY = (terrainHeight(x, Math.min(1, y + epsilon)) - terrainHeight(x, Math.max(0, y - epsilon))) / (epsilon * 2);
  const slopeScale = 1 / 12;
  return [height, Math.max(-1, Math.min(1, slopeX * slopeScale)), Math.max(-1, Math.min(1, slopeY * slopeScale))];
}

function projectTerrainPoint(x, y, terrain) {
  return [
    x + terrain[1] * EVIDENCE_TERRAIN_LATERAL,
    y + terrain[0] * EVIDENCE_TERRAIN_VERTICAL + terrain[2] * EVIDENCE_TERRAIN_LATERAL * 0.28,
  ];
}

function createDots(random, columns, rows, jitter) {
  const positions = new Float32Array(columns * rows * 2);
  const styles = new Float32Array(columns * rows * 3);
  const terrain = new Float32Array(columns * rows * 3);
  const xStep = 1 / (columns - 1);
  const yStep = 1 / (rows - 1);
  let point = 0;
  for (let row = 0; row < rows; row += 1) {
    for (let column = 0; column < columns; column += 1) {
      const boundary = row === 0 || row === rows - 1 || column === 0 || column === columns - 1;
      const offsetX = boundary ? 0 : (random() * 2 - 1) * xStep * jitter;
      const offsetY = boundary ? 0 : (random() * 2 - 1) * yStep * jitter;
      const x = Math.min(1, Math.max(0, column * xStep + offsetX));
      const y = Math.min(1, Math.max(0, row * yStep + offsetY));
      positions.set([x, y], point * 2);
      styles.set([0.9 + random() * 0.8, 0.58 + random() * 0.42, random()], point * 3);
      terrain.set(terrainSample(x, y), point * 3);
      point += 1;
    }
  }
  return { positions, styles, terrain };
}

function triangulate(positions, styles, terrain) {
  const points = Array.from({ length: positions.length / 2 }, (_, index) => [positions[index * 2], positions[index * 2 + 1]]);
  const triangles = Uint32Array.from(Delaunator.from(points).triangles);
  const edgeKeys = new Set();
  const edges = [];

  const addEdge = (left, right) => {
    const start = Math.min(left, right);
    const end = Math.max(left, right);
    const key = `${start}:${end}`;
    if (edgeKeys.has(key)) return;
    edgeKeys.add(key);
    edges.push(start, end);
  };

  for (let index = 0; index < triangles.length; index += 3) {
    const a = triangles[index];
    const b = triangles[index + 1];
    const c = triangles[index + 2];
    addEdge(a, b);
    addEdge(b, c);
    addEdge(c, a);
  }

  const edgeIndices = Uint32Array.from(edges);
  const edgePositions = new Float32Array(edgeIndices.length * 2);
  const edgeStyles = new Float32Array(edgeIndices.length * 3);
  const edgeTerrain = new Float32Array(edgeIndices.length * 3);
  for (let vertex = 0; vertex < edgeIndices.length; vertex += 1) {
    const point = edgeIndices[vertex];
    edgePositions.set(positions.subarray(point * 2, point * 2 + 2), vertex * 2);
    edgeStyles.set(styles.subarray(point * 3, point * 3 + 3), vertex * 3);
    edgeTerrain.set(terrain.subarray(point * 3, point * 3 + 3), vertex * 3);
  }

  return { triangles, edgeIndices, edgePositions, edgeStyles, edgeTerrain };
}

export function createEvidenceStaticPaths(field, size = 1000) {
  const fixed = (value) => (value * size).toFixed(2);
  const edges = [];
  for (let index = 0; index < field.edgePositions.length; index += 4) {
    const first = projectTerrainPoint(field.edgePositions[index], field.edgePositions[index + 1], field.edgeTerrain.subarray((index / 2) * 3, (index / 2) * 3 + 3));
    const second = projectTerrainPoint(field.edgePositions[index + 2], field.edgePositions[index + 3], field.edgeTerrain.subarray((index / 2 + 1) * 3, (index / 2 + 1) * 3 + 3));
    edges.push(`M${fixed(first[0])} ${fixed(first[1])}L${fixed(second[0])} ${fixed(second[1])}`);
  }
  const dots = [];
  for (let index = 0; index < field.dotPositions.length; index += 2) {
    const point = projectTerrainPoint(field.dotPositions[index], field.dotPositions[index + 1], field.dotTerrain.subarray((index / 2) * 3, (index / 2) * 3 + 3));
    const x = fixed(point[0]);
    const y = fixed(point[1]);
    dots.push(`M${x} ${y}h.01`);
  }
  return { viewBox: `0 0 ${size} ${size}`, edges: edges.join(""), dots: dots.join("") };
}

export function createEvidenceDotField({
  seed = EVIDENCE_DOT_FIELD_SEED,
  columns = EVIDENCE_DOT_COLUMNS,
  rows = EVIDENCE_DOT_ROWS,
  jitter = EVIDENCE_DOT_JITTER,
} = {}) {
  if (columns < 3 || rows < 3) throw new Error("Evidence dot field requires at least a 3x3 matrix.");
  if (jitter < 0 || jitter > EVIDENCE_DOT_JITTER) throw new Error("Evidence dot jitter exceeds its material contract.");
  const random = seededRandom(seed);
  const dots = createDots(random, columns, rows, jitter);
  const topology = triangulate(dots.positions, dots.styles, dots.terrain);
  const signature = hashTypedArrays([dots.positions, dots.styles, dots.terrain, topology.triangles, topology.edgeIndices]);
  return {
    seed,
    columns,
    rows,
    jitter,
    dotPositions: dots.positions,
    dotStyles: dots.styles,
    dotTerrain: dots.terrain,
    ...topology,
    signature,
  };
}
