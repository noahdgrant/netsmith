// The camera of the 3D view, as plain numbers: the preset views, the flip, the fit and the change
// between a perspective and an orthographic camera. The view works in the page's frame of the
// scene: millimetres, Y up out of the board's top face, X to the right, +Z toward the board's
// bottom edge (the `.glb` file's frame, scaled from metres). Nothing here touches three.js.

import { PRESETS, type Preset } from './mode';

export { PRESETS, type Preset };

export type Vec3 = [number, number, number];

/** How far off the pole a view straight down or up is, so the board's top edge stays up on the
 * screen (an orbit about Y has no "up" at the pole itself). */
const POLE = 1e-4;

const norm = (v: Vec3): Vec3 => {
  const l = Math.hypot(...v) || 1;
  return [v[0] / l, v[1] / l, v[2] / l];
};

/** The direction from the board's centre to the camera for a preset. Top looks down with the
 * board's top edge up; bottom looks up at the bottom face with the top edge still up (so left
 * and right are mirrored, as the face is seen from below); iso looks from the front right, above. */
export function presetDirection(p: Preset): Vec3 {
  switch (p) {
    case 'top':
      return norm([0, 1, POLE]);
    case 'bottom':
      return norm([0, -1, -POLE]);
    case 'front':
      return [0, 0, 1];
    case 'back':
      return [0, 0, -1];
    case 'left':
      return [-1, 0, 0];
    case 'right':
      return [1, 0, 0];
    case 'iso':
      return norm([1, 1.15, 1.3]);
  }
}

/** The direction seen from the other face: the camera's place mirrored through the board's plane. */
export function flipped(dir: Vec3): Vec3 {
  const [x, y, z] = dir;
  // Looking straight down or up, the mirror keeps the top edge up: the small offset changes side too.
  if (Math.abs(y) > 0.999) return norm([x, -y, -z]);
  return [x, -y, z];
}

/** The preset a direction is, if it is one (to within a degree or so), else null. */
export function presetOf(dir: Vec3): Preset | null {
  const d = norm(dir);
  for (const { id } of PRESETS) {
    const p = presetDirection(id);
    if (d[0] * p[0] + d[1] * p[1] + d[2] * p[2] > 0.9998) return id;
  }
  return null;
}

/** The distance from the centre of a sphere of `radius` at which a perspective camera of vertical
 * field `fovDeg` shows all of it in a window of `aspect` (width over height), with `margin` to spare. */
export function fitDistance(radius: number, fovDeg: number, aspect: number, margin = 1.08): number {
  const v = (fovDeg * Math.PI) / 360;
  const h = Math.atan(Math.tan(v) * aspect);
  return (radius * margin) / Math.sin(Math.min(v, h));
}

/** How far from the centre of `points` (given relative to it) a perspective camera looking from
 * `dir` must stand to show every one of them, and the half height an orthographic one needs: the
 * tight fit of the box's corners, not of the sphere round them, with `margin` to spare. */
export function fitPoints(points: Vec3[], dir: Vec3, fovDeg: number, aspect: number, margin = 1.1): { distance: number; halfHeight: number } {
  const back = norm(dir);
  // The camera's right and up, with the world's Y up (a view from straight above has its top edge toward -Z).
  let right = norm([back[2], 0, -back[0]]);
  if (Math.hypot(back[2], back[0]) < 1e-9) right = [1, 0, 0];
  const up: Vec3 = [back[1] * right[2] - back[2] * right[1], back[2] * right[0] - back[0] * right[2], back[0] * right[1] - back[1] * right[0]];
  const tv = Math.tan((fovDeg * Math.PI) / 360);
  const th = tv * aspect;
  let distance = 0;
  let half = 0;
  for (const p of points) {
    const x = Math.abs(p[0] * right[0] + p[1] * right[1] + p[2] * right[2]) * margin;
    const y = Math.abs(p[0] * up[0] + p[1] * up[1] + p[2] * up[2]) * margin;
    const z = p[0] * back[0] + p[1] * back[1] + p[2] * back[2];
    distance = Math.max(distance, z + x / th, z + y / tv);
    half = Math.max(half, y, x / aspect);
  }
  return { distance: Math.max(distance, 1), halfHeight: Math.max(half, 0.5) };
}

/** The half height of an orthographic view that shows a sphere of `radius` in a window of `aspect`. */
export function fitHalfHeight(radius: number, aspect: number, margin = 1.08): number {
  return (radius * margin) / Math.min(1, aspect);
}

/** The orthographic half height that shows at `distance` what a perspective camera of `fovDeg` shows there. */
export const halfHeightAt = (distance: number, fovDeg: number): number => distance * Math.tan((fovDeg * Math.PI) / 360);

/** The perspective distance that shows what an orthographic half height shows. */
export const distanceFor = (halfHeight: number, fovDeg: number): number => halfHeight / Math.tan((fovDeg * Math.PI) / 360);

/** The camera's direction turned about the vertical by `yaw` and toward or away from the pole by
 * `pitch` (radians), as an arrow key turns it; it stops short of the poles. */
export function orbit(dir: Vec3, yaw: number, pitch: number): Vec3 {
  const d = norm(dir);
  const theta = Math.atan2(d[0], d[2]) + yaw;
  const polar = Math.min(Math.PI - 0.01, Math.max(0.01, Math.acos(Math.max(-1, Math.min(1, d[1]))) + pitch));
  return [Math.sin(polar) * Math.sin(theta), Math.cos(polar), Math.sin(polar) * Math.cos(theta)];
}

/** Ease out, for a camera move: fast at first, gentle at the end. */
export const easeOut = (t: number): number => 1 - (1 - Math.min(1, Math.max(0, t))) ** 3;

/** A direction part of the way from `a` to `b` along the great circle between them. */
export function slerpDir(a: Vec3, b: Vec3, t: number): Vec3 {
  const x = norm(a);
  const y = norm(b);
  const dot = Math.max(-1, Math.min(1, x[0] * y[0] + x[1] * y[1] + x[2] * y[2]));
  const angle = Math.acos(dot);
  if (angle < 1e-6) return y;
  if (Math.PI - angle < 1e-3) {
    // Opposite directions (a flip): go round by the board's front.
    const mid: Vec3 = norm([x[0] + 1e-3, 0, Math.abs(x[2]) > 0.5 ? x[2] : 1]);
    return t < 0.5 ? slerpDir(x, mid, t * 2) : slerpDir(mid, y, t * 2 - 1);
  }
  const s = Math.sin(angle);
  const wa = Math.sin((1 - t) * angle) / s;
  const wb = Math.sin(t * angle) / s;
  return norm([x[0] * wa + y[0] * wb, x[1] * wa + y[1] * wb, x[2] * wa + y[2] * wb]);
}
