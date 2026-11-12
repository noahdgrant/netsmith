// The 3D view's drawing: three.js over a canvas, showing the scene's `.glb` file as the server
// wrote it (ADR 0105). It draws what the file has and nothing else: the materials are coloured by
// their names from the page's theme (palette.ts), a part of unknown height stays the flat outline
// the file gives it, hatched, and nothing is moved. This file is only in the chunk that is loaded
// when the 3D view first opens.
//
// Frame: the file is in metres, Y up; the model is scaled by 1000 so the view works in mm (the
// camera's numbers in presets.ts). Drawing is on demand: a frame is drawn when something changed
// (the camera, the colours, a highlight), not in a loop.

import {
  BufferAttribute,
  CanvasTexture,
  Color,
  DirectionalLight,
  DoubleSide,
  EdgesGeometry,
  Group,
  LineBasicMaterial,
  LineDashedMaterial,
  LineSegments,
  type Material,
  Mesh,
  MeshStandardMaterial,
  NoToneMapping,
  type Object3D,
  OrthographicCamera,
  PMREMGenerator,
  PerspectiveCamera,
  Raycaster,
  RepeatWrapping,
  Scene,
  Sphere,
  Box3,
  SRGBColorSpace,
  Vector2,
  Vector3,
  WebGLRenderer,
  MOUSE,
} from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js';

import { type BoardInfo, type Info, type LimitInfo, type PartInfo3d, readInfo } from './model';
import { LOOKS, type Role, lit, roleOf } from './palette';
import { type Preset, type Vec3, distanceFor, easeOut, fitPoints, fitHalfHeight, flipped, halfHeightAt, orbit, presetDirection, presetOf, slerpDir } from './presets';

const FOV = 30;
/** How long a camera move takes (the page's motion is 120 to 180 ms). */
const MOVE_MS = 180;
/** The hatch's period on the board, in mm. */
const HATCH_MM = 0.9;

export type Hit = { kind: 'part'; info: PartInfo3d } | { kind: 'limit'; info: LimitInfo };

/** The colours the view is drawn in, as CSS colours: one per role and the four of `TOKENS`. */
export type Colours = Record<Role, string> & { background: string; edge: string; select: string; hover: string };

interface PartEntry {
  info: PartInfo3d;
  meshes: Mesh[];
  lines: LineSegments[];
}

type State = 'rest' | 'hover' | 'select';

/** A canvas-made stripe, white on nothing: the material's colour tints it. */
function hatchTexture(): CanvasTexture {
  const size = 64;
  const c = document.createElement('canvas');
  c.width = c.height = size;
  const g = c.getContext('2d')!;
  g.clearRect(0, 0, size, size);
  g.strokeStyle = 'white';
  g.lineWidth = size * 0.2;
  g.lineCap = 'square';
  for (let k = -1; k <= 1; k++) {
    g.beginPath();
    g.moveTo(k * size - size * 0.1, size * 1.1);
    g.lineTo(k * size + size * 1.1, -size * 0.1);
    g.stroke();
  }
  const t = new CanvasTexture(c);
  t.wrapS = t.wrapT = RepeatWrapping;
  t.colorSpace = SRGBColorSpace;
  t.anisotropy = 4;
  return t;
}

/** Texture coordinates for a flat outline from where its points are on the board, so the hatch keeps its size and angle on every part. */
function planarUv(mesh: Mesh) {
  const pos = mesh.geometry.getAttribute('position');
  if (!pos) return;
  const uv = new Float32Array(pos.count * 2);
  for (let i = 0; i < pos.count; i++) {
    uv[i * 2] = (pos.getX(i) * 1000) / HATCH_MM;
    uv[i * 2 + 1] = (pos.getZ(i) * 1000) / HATCH_MM;
  }
  mesh.geometry.setAttribute('uv', new BufferAttribute(uv, 2));
}

/** `lit` of palette.ts on three's colours, in the sRGB values the theme gives. */
function litColour(base: Color, ink: Color): Color {
  const channels = (c: Color): [number, number, number] => {
    const h = c.getHex();
    return [(h >> 16) & 255, (h >> 8) & 255, h & 255];
  };
  const [r, g, b] = lit(channels(base), channels(ink));
  return new Color().setHex((r << 16) | (g << 8) | b);
}

export class Viewer {
  readonly canvas: HTMLCanvasElement;
  private renderer: WebGLRenderer;
  private scene = new Scene();
  private persp = new PerspectiveCamera(FOV, 1, 0.1, 10000);
  private ortho = new OrthographicCamera(-1, 1, 1, -1, 0.1, 10000);
  private orthoOn = false;
  private controls: OrbitControls;
  private model: Group | null = null;
  private edges = new Group();
  private parts = new Map<string, PartEntry>();
  private limits: { info: LimitInfo; mesh: Mesh; lines: LineSegments[] }[] = [];
  private pickable: Mesh[] = [];
  private infoOf = new Map<Object3D, Info>();
  private materials = new Map<string, Material>();
  private disposables: { dispose(): void }[] = [];
  private hatch = hatchTexture();
  private colours: Colours | null = null;
  private bounds = new Sphere(new Vector3(), 50);
  private box = new Box3(new Vector3(-50, -50, -50), new Vector3(50, 50, 50));
  private orthoBase = 50;
  private frame = 0;
  private anim = 0;
  private drawn = false;
  private selected = new Set<string>();
  private hovered = new Set<string>();
  private ray = new Raycaster();
  board: BoardInfo | null = null;
  /** Whether camera moves are instant (reduced motion). */
  still = false;

  constructor(
    host: HTMLElement,
    private hooks: { onFirstFrame(): void; onCamera?(): void },
  ) {
    this.canvas = document.createElement('canvas');
    this.canvas.className = 'v3-canvas';
    host.prepend(this.canvas);
    const renderer = new WebGLRenderer({ canvas: this.canvas, antialias: true, powerPreference: 'high-performance' });
    if (!renderer.capabilities.isWebGL2) {
      renderer.dispose();
      throw new Error('This browser has no WebGL 2');
    }
    this.renderer = renderer;
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.outputColorSpace = SRGBColorSpace;
    renderer.toneMapping = NoToneMapping;

    // Light from a room, made here (no file from the network), and a key light from above.
    const pmrem = new PMREMGenerator(renderer);
    const room = new RoomEnvironment();
    const env = pmrem.fromScene(room, 0.04).texture;
    room.dispose?.();
    pmrem.dispose();
    this.scene.environment = env;
    this.scene.environmentIntensity = 0.85;
    this.disposables.push(env);
    const key = new DirectionalLight(0xffffff, 1.1);
    key.position.set(-0.35, 1, 0.55);
    this.scene.add(key);
    this.scene.add(this.edges);

    this.persp.position.set(100, 100, 100);
    const controls = new OrbitControls(this.persp, this.canvas);
    controls.enableDamping = false;
    controls.screenSpacePanning = true;
    controls.zoomToCursor = true;
    controls.rotateSpeed = 0.8;
    // As the 2D board: the main button is the pointer's (here it orbits; a click picks), the middle and right buttons pan, the wheel zooms.
    controls.mouseButtons = { LEFT: MOUSE.ROTATE, MIDDLE: MOUSE.PAN, RIGHT: MOUSE.PAN };
    controls.addEventListener('change', () => {
      this.clip();
      this.request();
      this.hooks.onCamera?.();
    });
    this.controls = controls;
  }

  /** Draw a frame soon (once, however often it is asked before then). */
  request() {
    if (!this.frame) this.frame = requestAnimationFrame(() => this.draw());
  }

  private camera() {
    return this.orthoOn ? this.ortho : this.persp;
  }

  private draw() {
    this.frame = 0;
    this.renderer.render(this.scene, this.camera());
    if (!this.drawn && this.model) {
      this.drawn = true;
      this.hooks.onFirstFrame();
    }
  }

  /** The canvas's size changed. */
  resize(w: number, h: number) {
    if (w < 1 || h < 1) return;
    this.renderer.setSize(w, h, false);
    this.persp.aspect = w / h;
    this.persp.updateProjectionMatrix();
    this.orthoFrustum();
    this.request();
  }

  private aspect() {
    const s = this.renderer.getSize(new Vector2());
    return s.y > 0 ? s.x / s.y : 1;
  }

  private orthoFrustum() {
    const a = this.aspect();
    const h = this.orthoBase;
    Object.assign(this.ortho, { left: -h * a, right: h * a, top: h, bottom: -h });
    this.ortho.updateProjectionMatrix();
  }

  /** Near and far planes that hold the board wherever the camera is. */
  private clip() {
    const r = this.bounds.radius;
    const d = this.persp.position.distanceTo(this.bounds.center);
    this.persp.near = Math.max(0.05, (d - r * 2) * 0.5, d / 2000);
    this.persp.far = d + r * 4 + 10;
    this.persp.updateProjectionMatrix();
    const od = this.ortho.position.distanceTo(this.controls.target);
    this.ortho.near = 0.01;
    this.ortho.far = od + r * 4 + 10;
    this.ortho.updateProjectionMatrix();
  }

  /** Show a scene's file. The first one is framed; a later one (the board was edited) keeps the camera. */
  async load(bytes: ArrayBuffer) {
    const gltf = await new GLTFLoader().parseAsync(bytes, '');
    const root = new Group();
    root.scale.setScalar(1000);
    root.add(gltf.scene);
    root.updateMatrixWorld(true);
    const first = this.model === null;
    this.clearModel();
    this.model = root;
    this.scene.add(root);
    this.index(gltf.scene);
    this.restyle();
    const box = new Box3().setFromObject(root);
    if (!box.isEmpty()) {
      this.box = box;
      box.getBoundingSphere(this.bounds);
    }
    this.bounds.radius = Math.max(this.bounds.radius, 1);
    if (first) {
      this.orthoBase = fitHalfHeight(this.bounds.radius, this.aspect());
      this.orthoFrustum();
      this.preset('iso', false);
    }
    this.clip();
    this.request();
  }

  private clearModel() {
    if (this.model) {
      this.scene.remove(this.model);
      this.model.traverse((o) => {
        if (o instanceof Mesh) o.geometry.dispose();
      });
    }
    for (const l of this.edges.children) (l as LineSegments).geometry.dispose();
    this.edges.clear();
    this.parts.clear();
    this.limits = [];
    this.pickable = [];
    this.infoOf.clear();
    this.board = null;
  }

  /** Find the nodes the contract names, by their extras, and give the meshes their roles and edge lines. */
  private index(root: Object3D) {
    const kindOf = (o: Object3D): { node: Object3D; info: Info } | null => {
      for (let n: Object3D | null = o; n; n = n.parent) {
        const info = readInfo(n.userData);
        if (info && info.kind !== 'board') return { node: n, info };
      }
      return null;
    };
    root.traverse((o) => {
      const info = readInfo(o.userData);
      if (info?.kind === 'board') this.board = info as BoardInfo;
    });
    root.traverse((o) => {
      if (!(o instanceof Mesh)) return;
      const owner = kindOf(o);
      const info = owner?.info;
      const name = (Array.isArray(o.material) ? o.material[0] : o.material)?.name ?? '';
      const part = info?.kind === 'part' ? (info as PartInfo3d) : null;
      const role = roleOf(name, part ? part.provenance : null);
      o.userData.role = role;
      if (role === 'body-unknown') planarUv(o);
      if (role && LOOKS[role].hatch) o.renderOrder = 2;
      if (role === 'limit' || role === 'limit-violated') o.renderOrder = 3;
      const lines: LineSegments[] = [];
      if (info?.kind === 'part' || info?.kind === 'substrate' || info?.kind === 'height_limit') {
        const geo = new EdgesGeometry(o.geometry, 28);
        const line = new LineSegments(geo);
        line.matrixAutoUpdate = false;
        line.matrix.copy(o.matrixWorld);
        line.userData.edgeRole = role === 'body-unknown' ? 'edge-unknown' : info.kind === 'height_limit' ? role : 'edge';
        if (role === 'body-unknown') line.computeLineDistances();
        line.renderOrder = 4;
        this.edges.add(line);
        lines.push(line);
      }
      if (part) {
        let entry = this.parts.get(part.path);
        if (!entry) this.parts.set(part.path, (entry = { info: part, meshes: [], lines: [] }));
        entry.meshes.push(o);
        entry.lines.push(...lines);
        this.pickable.push(o);
        this.infoOf.set(o, part);
      } else if (info?.kind === 'height_limit') {
        this.limits.push({ info: info as LimitInfo, mesh: o, lines });
        this.pickable.push(o);
        this.infoOf.set(o, info);
      }
    });
  }

  /** The parts of the scene, in the file's order. */
  partList(): PartInfo3d[] {
    return [...this.parts.values()].map((p) => p.info);
  }

  /** The height limits' volumes, one per face of each. */
  limitList(): LimitInfo[] {
    return this.limits.map((l) => l.info);
  }

  /** Colour the scene in the theme's colours. */
  setColours(c: Colours) {
    this.colours = c;
    for (const m of this.materials.values()) m.dispose();
    this.materials.clear();
    this.renderer.setClearColor(new Color(c.background), 1);
    this.restyle();
    this.request();
  }

  private material(key: string, make: () => Material): Material {
    let m = this.materials.get(key);
    if (!m) this.materials.set(key, (m = make()));
    return m;
  }

  private surface(role: Role, state: State): Material {
    const c = this.colours!;
    return this.material(`${role}:${state}`, () => {
      const look = LOOKS[role];
      const base = new Color(c[role]);
      const colour = state === 'select' ? new Color(c.select) : state === 'hover' ? litColour(base, new Color(c.hover)) : base;
      const translucent = look.opacity < 1;
      const m = new MeshStandardMaterial({
        name: role,
        color: colour,
        metalness: look.metalness,
        roughness: look.roughness,
        transparent: translucent,
        opacity: state === 'rest' ? look.opacity : Math.max(look.opacity, translucent ? 0.32 : 1),
        depthWrite: !translucent && !look.hatch,
        side: DoubleSide,
      });
      if (look.hatch) {
        m.map = this.hatch;
        m.transparent = true;
        m.alphaTest = 0.05;
        m.polygonOffset = true;
        m.polygonOffsetFactor = -2;
        m.polygonOffsetUnits = -4;
      }
      if (state === 'select') m.emissive = new Color(c.select).multiplyScalar(0.18);
      if (role === 'limit' || role === 'limit-violated') {
        // The volume's floor is the board's face: it stays behind the mask instead of flickering through it.
        m.polygonOffset = true;
        m.polygonOffsetFactor = 2;
        m.polygonOffsetUnits = 4;
      }
      return m;
    });
  }

  private line(role: string, state: State): Material {
    const c = this.colours!;
    return this.material(`line:${role}:${state}`, () => {
      const colour =
        state === 'select'
          ? c.select
          : state === 'hover'
            ? `#${litColour(new Color(c.edge), new Color(c.hover)).getHexString()}`
            : role === 'limit' || role === 'limit-violated'
              ? c[role]
              : role === 'edge-unknown'
                ? c['body-unknown']
                : c.edge;
      const opacity = state !== 'rest' ? 1 : role === 'edge' ? 0.55 : role === 'edge-unknown' ? 0.95 : 0.7;
      if (role === 'edge-unknown' && state === 'rest') {
        return new LineDashedMaterial({ color: new Color(colour), dashSize: 0.0007, gapSize: 0.0005, transparent: true, opacity });
      }
      return new LineBasicMaterial({ color: new Color(colour), transparent: opacity < 1, opacity });
    });
  }

  private stateOf(path: string): State {
    return this.selected.has(path) ? 'select' : this.hovered.has(path) ? 'hover' : 'rest';
  }

  /** Give every mesh and line the material of its role and state. */
  private restyle() {
    if (!this.colours || !this.model) return;
    this.model.traverse((o) => {
      if (!(o instanceof Mesh)) return;
      const role = o.userData.role as Role | null;
      if (!role) return;
      const part = this.infoOf.get(o);
      const state = part?.kind === 'part' ? this.stateOf((part as PartInfo3d).path) : 'rest';
      o.material = this.surface(role, state);
    });
    for (const l of this.edges.children as LineSegments[]) l.material = this.line(l.userData.edgeRole, 'rest');
    for (const [path, p] of this.parts) {
      const state = this.stateOf(path);
      if (state !== 'rest') for (const l of p.lines) l.material = this.line(l.userData.edgeRole, state);
    }
  }

  /** The parts picked (lit in the page's selection colour) and the parts the shared hover lights. */
  highlight(selected: Iterable<string>, hovered: Iterable<string>) {
    this.selected = new Set(selected);
    this.hovered = new Set(hovered);
    this.restyle();
    this.request();
  }

  /** What is under a point of the window: a part before the limit volume around it. */
  pick(clientX: number, clientY: number): Hit | null {
    const r = this.canvas.getBoundingClientRect();
    if (!this.model || r.width === 0) return null;
    const at = new Vector2(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1);
    this.ray.setFromCamera(at, this.camera());
    const hits = this.ray.intersectObjects(this.pickable, false);
    let limit: LimitInfo | null = null;
    for (const h of hits) {
      const info = this.infoOf.get(h.object);
      if (info?.kind === 'part') return { kind: 'part', info: info as PartInfo3d };
      if (info?.kind === 'height_limit' && !limit) limit = info as LimitInfo;
    }
    return limit ? { kind: 'limit', info: limit } : null;
  }

  /** A point of the window where a pick finds the part (the one nearest the middle of its bounds), or null when none does (hidden or off screen); for a recipe. */
  project(path: string): { x: number; y: number } | null {
    const p = this.parts.get(path);
    if (!p) return null;
    const box = new Box3();
    for (const m of p.meshes) box.expandByObject(m);
    if (box.isEmpty()) return null;
    // The window box of the part's bounds, then points over it from its middle outward until one
    // that a pick says is the part: a package shape (thin leads, a body lower than its box) has
    // nothing at its box's centre, and another part or a limit may be in front.
    const r = this.canvas.getBoundingClientRect();
    const cam = this.camera();
    let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
    for (const x of [box.min.x, box.max.x])
      for (const y of [box.min.y, box.max.y])
        for (const z of [box.min.z, box.max.z]) {
          const v = new Vector3(x, y, z).project(cam);
          const sx = r.left + ((v.x + 1) / 2) * r.width;
          const sy = r.top + ((1 - v.y) / 2) * r.height;
          [x0, y0, x1, y1] = [Math.min(x0, sx), Math.min(y0, sy), Math.max(x1, sx), Math.max(y1, sy)];
        }
    const cx = (x0 + x1) / 2;
    const cy = (y0 + y1) / 2;
    const n = 12;
    const points: { x: number; y: number; d: number }[] = [];
    for (let i = 0; i <= n; i++)
      for (let j = 0; j <= n; j++) {
        const x = x0 + ((x1 - x0) * i) / n;
        const y = y0 + ((y1 - y0) * j) / n;
        points.push({ x, y, d: Math.hypot(x - cx, y - cy) });
      }
    points.sort((a, b) => a.d - b.d);
    for (const q of points) {
      if (q.x < r.left || q.y < r.top || q.x > r.right || q.y > r.bottom) continue;
      const hit = this.pick(q.x, q.y);
      if (hit?.kind === 'part' && hit.info.path === path) return { x: Math.round(q.x), y: Math.round(q.y) };
    }
    return null;
  }

  // ---- the camera ----

  private direction(): Vec3 {
    const cam = this.camera();
    const d = cam.position.clone().sub(this.controls.target).normalize();
    return [d.x, d.y, d.z];
  }

  /** The preset the camera looks from now, if it is one. */
  currentPreset(): Preset | null {
    return presetOf(this.direction());
  }

  isOrtho() {
    return this.orthoOn;
  }

  /** Move the camera to look at `target` from `dir`, at `dist` (perspective) or `zoom` (orthographic). */
  private moveTo(target: Vector3, dir: Vec3, dist: number, zoom: number, animate: boolean) {
    cancelAnimationFrame(this.anim);
    const t0 = this.controls.target.clone();
    const d0 = this.direction();
    const dist0 = this.persp.position.distanceTo(this.controls.target);
    const z0 = this.ortho.zoom;
    const orthoDist = this.bounds.radius * 6 + 10;
    const put = (e: number) => {
      const t = t0.clone().lerp(target, e);
      const d = slerpDir(d0, dir, e);
      const dd = dist0 + (dist - dist0) * e;
      this.controls.target.copy(t);
      this.persp.position.set(t.x + d[0] * dd, t.y + d[1] * dd, t.z + d[2] * dd);
      this.ortho.position.set(t.x + d[0] * orthoDist, t.y + d[1] * orthoDist, t.z + d[2] * orthoDist);
      this.ortho.zoom = z0 + (zoom - z0) * e;
      this.ortho.updateProjectionMatrix();
      this.persp.lookAt(t);
      this.ortho.lookAt(t);
      this.controls.update();
    };
    if (!animate || this.still) {
      put(1);
      this.request();
      return;
    }
    const start = performance.now();
    const step = (now: number) => {
      const k = Math.min(1, (now - start) / MOVE_MS);
      put(easeOut(k));
      this.request();
      if (k < 1) this.anim = requestAnimationFrame(step);
    };
    this.anim = requestAnimationFrame(step);
  }

  /** The camera that shows all of `box` from `dir`: its target, distance and orthographic zoom. */
  private framing(box: Box3, dir: Vec3): { target: Vector3; dist: number; zoom: number } {
    const c = box.getCenter(new Vector3());
    const corners: Vec3[] = [];
    for (const x of [box.min.x, box.max.x]) for (const y of [box.min.y, box.max.y]) for (const z of [box.min.z, box.max.z]) corners.push([x - c.x, y - c.y, z - c.z]);
    const f = fitPoints(corners, dir, FOV, this.aspect());
    return { target: c, dist: f.distance, zoom: this.orthoBase / f.halfHeight };
  }

  /** Look at the whole board from a preset. */
  preset(p: Preset, animate = true) {
    const dir = presetDirection(p);
    const f = this.framing(this.box, dir);
    this.moveTo(f.target, dir, f.dist, f.zoom, animate);
  }

  /** Look at the board from the other face, as from the same place mirrored through the board. */
  flip() {
    this.moveTo(this.controls.target.clone().setY(this.bounds.center.y), flipped(this.direction()), this.persp.position.distanceTo(this.controls.target), this.ortho.zoom, true);
  }

  /** The whole board, from where the camera looks now. */
  fit(animate = true) {
    const dir = this.direction();
    const f = this.framing(this.box, dir);
    this.moveTo(f.target, dir, f.dist, f.zoom, animate);
  }

  /** The parts at these paths filling the view, from where the camera looks now. */
  frameParts(paths: string[]) {
    const box = new Box3();
    for (const path of paths) for (const m of this.parts.get(path)?.meshes ?? []) box.expandByObject(m);
    if (box.isEmpty()) return;
    // Room round a small part, so what is near it shows too.
    box.expandByScalar(Math.max(0, 14 - box.getSize(new Vector3()).length() / 2));
    const dir = this.direction();
    const f = this.framing(box, dir);
    this.moveTo(f.target, dir, f.dist, f.zoom, true);
  }

  /** Zoom in (above 1) or out about the middle of the view. */
  zoomBy(f: number) {
    const t = this.controls.target.clone();
    const dist = this.persp.position.distanceTo(t);
    this.moveTo(t, this.direction(), dist / f, this.ortho.zoom * f, true);
  }

  /** Turn the camera about the board, as an arrow key does. */
  orbitBy(yaw: number, pitch: number) {
    const t = this.controls.target.clone();
    this.moveTo(t, orbit(this.direction(), yaw, pitch), this.persp.position.distanceTo(t), this.ortho.zoom, false);
  }

  /** Move the view by a share of its height (an arrow key with Shift). */
  panBy(sx: number, sy: number) {
    const cam = this.camera();
    const half = this.orthoOn ? this.ortho.top / this.ortho.zoom : halfHeightAt(this.persp.position.distanceTo(this.controls.target), FOV);
    const right = new Vector3().setFromMatrixColumn(cam.matrix, 0).multiplyScalar(sx * half * 2);
    const up = new Vector3().setFromMatrixColumn(cam.matrix, 1).multiplyScalar(sy * half * 2);
    const t = this.controls.target.clone().add(right).add(up);
    this.moveTo(t, this.direction(), this.persp.position.distanceTo(this.controls.target), this.ortho.zoom, false);
  }

  /** Change between a perspective and an orthographic camera, keeping what is seen at the target. */
  toggleProjection() {
    const t = this.controls.target;
    if (this.orthoOn) {
      const half = this.ortho.top / this.ortho.zoom;
      const d = this.direction();
      const dist = distanceFor(half, FOV);
      this.persp.position.set(t.x + d[0] * dist, t.y + d[1] * dist, t.z + d[2] * dist);
      this.persp.lookAt(t);
      this.orthoOn = false;
    } else {
      const half = halfHeightAt(this.persp.position.distanceTo(t), FOV);
      this.ortho.zoom = this.orthoBase / half;
      const d = this.direction();
      const od = this.bounds.radius * 6 + 10;
      this.ortho.position.set(t.x + d[0] * od, t.y + d[1] * od, t.z + d[2] * od);
      this.ortho.lookAt(t);
      this.orthoOn = true;
    }
    this.controls.object = this.camera();
    this.clip();
    this.controls.update();
    this.request();
  }

  /** Draw now and count what is on screen (for a recipe: the view is not blank). The pixels are
   * read in the same task as the drawing, so no buffer needs keeping. */
  sample(): { width: number; height: number; drawn: number; colours: number } {
    this.renderer.render(this.scene, this.camera());
    const gl = this.renderer.getContext();
    const w = gl.drawingBufferWidth;
    const h = gl.drawingBufferHeight;
    const px = new Uint8Array(w * h * 4);
    gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, px);
    const bg = new Color(this.colours?.background ?? 'black').getHex();
    const seen = new Set<number>();
    let drawn = 0;
    for (let i = 0; i < px.length; i += 16) {
      const hex = (px[i] << 16) | (px[i + 1] << 8) | px[i + 2];
      seen.add(hex >> 2);
      const dr = Math.abs(px[i] - ((bg >> 16) & 255));
      const dg = Math.abs(px[i + 1] - ((bg >> 8) & 255));
      const db = Math.abs(px[i + 2] - (bg & 255));
      if (dr + dg + db > 12) drawn++;
    }
    return { width: w, height: h, drawn: drawn / (px.length / 16), colours: seen.size };
  }

  /** Draw now and read the colour at a point of the window, as `#rrggbb` (for a recipe). */
  colourAt(clientX: number, clientY: number): string | null {
    const r = this.canvas.getBoundingClientRect();
    const gl = this.renderer.getContext();
    const x = Math.round(((clientX - r.left) / r.width) * gl.drawingBufferWidth);
    const y = Math.round(((r.bottom - clientY) / r.height) * gl.drawingBufferHeight);
    if (x < 0 || y < 0 || x >= gl.drawingBufferWidth || y >= gl.drawingBufferHeight) return null;
    this.renderer.render(this.scene, this.camera());
    const px = new Uint8Array(4);
    gl.readPixels(x, y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
    return `#${((1 << 24) | (px[0] << 16) | (px[1] << 8) | px[2]).toString(16).slice(1)}`;
  }

  dispose() {
    cancelAnimationFrame(this.frame);
    cancelAnimationFrame(this.anim);
    this.controls.dispose();
    this.clearModel();
    for (const m of this.materials.values()) m.dispose();
    this.hatch.dispose();
    for (const d of this.disposables) d.dispose();
    this.renderer.dispose();
    this.canvas.remove();
  }
}
