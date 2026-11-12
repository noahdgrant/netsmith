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
  AlwaysStencilFunc,
  BackSide,
  BufferAttribute,
  BufferGeometry,
  DecrementWrapStencilOp,
  FrontSide,
  IncrementWrapStencilOp,
  Matrix3,
  MeshBasicMaterial,
  NotEqualStencilFunc,
  Plane,
  PlaneGeometry,
  ReplaceStencilOp,
  type Side,
  type StencilOp,
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

import { DIM, type KindSwatch, type MarginBand, type NetState, extrudeFlat, kindSwatch, litReport, marginColour, pieceNet, xrayOpacity } from './inspect';
import type { ColourBy } from './inspectMode';
import { type BoardInfo, type CopperInfo, type Info, type LimitInfo, type PartInfo3d, type StackLayer, readInfo } from './model';
import { LOOKS, type Role, lit, roleOf } from './palette';
import { type Preset, type Vec3, distanceFor, easeOut, fitPoints, fitHalfHeight, flipped, halfHeightAt, orbit, presetDirection, presetOf, slerpDir } from './presets';
import { type Axis, type LimitPlan, type P3, type Slot, type Snap, bestSnap, easeInOut, explodeGap, explodeSlots, faceSlot, keepBelow, slabRole } from './tools';

const FOV = 30;
/** How long a camera move takes (the page's motion is 120 to 180 ms). */
const MOVE_MS = 180;
/** The hatch's period on the board, in mm. */
const HATCH_MM = 0.9;
/** How long the stackup takes to come apart or back together. */
const EXPLODE_MS = 420;
/** The roles whose cut is filled first: the board's layers, then the copper, then the parts. */
const CAP_ORDER = ['substrate', 'prepreg', 'mask-top', 'mask-bottom', 'mask-over-copper', 'copper', 'finish', 'metal', 'body', 'body-catalog', 'mark'];

/** The scene's frame (mm, z up) from the view's (mm, Y up, +Z toward the board's bottom edge). */
const sceneOf = (v: Vector3): P3 => [v.x, -v.z, v.y];
/** The view's frame from the scene's. */
const worldOf = (p: P3): Vector3 => new Vector3(p[0], p[2], -p[1]);
/** The direction of a scene axis in the view's frame. */
const AXIS_DIR: Record<Axis, Vector3> = { x: new Vector3(1, 0, 0), y: new Vector3(0, 0, -1), z: new Vector3(0, 1, 0) };

/** Whether every edge of a mesh's triangles is shared by an even number of them: a closed solid,
 * whose cut can be filled (a barrel, a flat outline or a sheet is open, and is not). */
function isClosed(geo: BufferGeometry): boolean {
  const pos = geo.getAttribute('position');
  if (!pos) return false;
  const ids = new Map<string, number>();
  const id = (i: number) => {
    const k = `${Math.round(pos.getX(i) * 1e7)},${Math.round(pos.getY(i) * 1e7)},${Math.round(pos.getZ(i) * 1e7)}`;
    let v = ids.get(k);
    if (v === undefined) ids.set(k, (v = ids.size));
    return v;
  };
  const index = geo.getIndex();
  const n = index ? index.count : pos.count;
  if (n < 12) return false;
  const edges = new Map<number, number>();
  const at = (k: number) => id(index ? index.getX(k) : k);
  for (let t = 0; t + 2 < n; t += 3) {
    const v = [at(t), at(t + 1), at(t + 2)];
    for (let e = 0; e < 3; e++) {
      const a = v[e];
      const b = v[(e + 1) % 3];
      if (a === b) continue;
      const key = a < b ? a * 4194304 + b : b * 4194304 + a;
      edges.set(key, (edges.get(key) ?? 0) + 1);
    }
  }
  for (const c of edges.values()) if (c % 2) return false;
  return edges.size > 0;
}

/** A copy of a laminate's geometry stretched in Y from its own range to `[bottom, top]` (metres):
 * the same outline, holes and cutouts, as thick as one layer. */
function slabOf(geo: BufferGeometry, y0: number, y1: number, bottom: number, top: number): BufferGeometry {
  const g = geo.clone();
  const pos = g.getAttribute('position');
  const span = y1 - y0 || 1;
  for (let i = 0; i < pos.count; i++) pos.setY(i, bottom + ((pos.getY(i) - y0) / span) * (top - bottom));
  pos.needsUpdate = true;
  g.computeBoundingBox();
  g.computeBoundingSphere();
  return g;
}

/** Whether the object and everything above it is shown. */
function shown(o: Object3D): boolean {
  for (let n: Object3D | null = o; n; n = n.parent) if (!n.visible) return false;
  return true;
}

/** A pad's centre on a face, for the measure tool to snap to. */
export interface PadPoint {
  at: P3;
  refdes: string;
  pad: string;
  side: 'top' | 'bottom';
}

export type Hit = { kind: 'part'; info: PartInfo3d } | { kind: 'limit'; info: LimitInfo } | { kind: 'copper'; info: CopperInfo };

/** The colours the view is drawn in, as CSS colours: one per role, the four of `TOKENS`, and those of colour by kind, the height map and the preview (inspect.ts). */
export type Colours = Record<Role, string> & {
  background: string;
  edge: string;
  select: string;
  hover: string;
  kinds: Record<KindSwatch, string>;
  margin: Record<MarginBand, string>;
  preview: string;
};

/** How the scene is looked at (inspectMode.ts): none of it is data. */
export interface Look {
  xray: boolean;
  colourBy: ColourBy;
  /** The nets lit, and how; the rest of the copper is dimmed while any is. */
  nets: Map<string, NetState>;
  /** The height preview's height in mm, or null when it is off. */
  previewMm: number | null;
}

/** How long a finding's limit pulses, once. */
const PULSE_MS = 900;

interface PartEntry {
  info: PartInfo3d;
  meshes: Mesh[];
  lines: LineSegments[];
}

type State = 'rest' | 'hover' | 'select';

/** What changes a surface's material besides its role and state. */
interface Touch {
  xray?: boolean;
  dim?: boolean;
  /** A colour in place of the role's (a body coloured by kind or by margin). */
  tint?: string | null;
}

/** What moves when the stackup comes apart: a mesh, where it was, and the layer it goes with
 * (`face:top`, `face:bottom`, a layer's name or `silk:<face>`). */
interface Mover {
  obj: Mesh;
  base: number;
  slot: string;
}

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
  private copper: { mesh: Mesh; info: CopperInfo; exposed: boolean }[] = [];
  private previews: Mesh[] = [];
  private previewEdges: LineSegments[] = [];
  private focusedLimits = new Set<string>();
  private pulseAnim = 0;
  /** The net of each pad, `path:pad`, from the 2D scene: a pad's barrel lights with its net. */
  private padNets = new Map<string, string>();
  private look: Look = { xray: false, colourBy: 'provenance', nets: new Map(), previewMm: null };
  // The tools (tools.ts): the stackup's layers as slabs, the section and its caps, the exploded stackup, the measure tool's targets.
  private substrate: Mesh[] = [];
  private holes: Mesh[] = [];
  private slabs: { mesh: Mesh; layer: StackLayer; ghost: boolean }[] = [];
  private movers: Mover[] = [];
  private lineSrc = new Map<LineSegments, Mesh>();
  private solids: Mesh[] = [];
  private pads: PadPoint[] = [];
  private holeKeys = new Set<string>();
  private silk = { top: false, bottom: false };
  private cut: { axis: Axis; at: number } | null = null;
  private plane = new Plane(new Vector3(1, 0, 0), 0);
  private caps = new Group();
  private capGeometry: PlaneGeometry | null = null;
  private capMaterials: Material[] = [];
  private closed = new WeakMap<BufferGeometry, boolean>();
  private explodeT = 0;
  private explodeTo = 0;
  private explodeAnim = 0;
  private slotList: Slot[] = [];
  private baseBox = this.box.clone();
  private drawHooks = new Set<() => void>();
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
    // A stencil buffer for the section's caps.
    const renderer = new WebGLRenderer({ canvas: this.canvas, antialias: true, stencil: true, powerPreference: 'high-performance' });
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
    this.scene.add(this.caps);

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
      this.placeCut();
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

  /** Call `f` after every frame drawn (the overlays follow the camera); returns the way to stop. */
  onDraw(f: () => void): () => void {
    this.drawHooks.add(f);
    return () => this.drawHooks.delete(f);
  }

  private render() {
    if (this.cut) for (const c of this.caps.children) if (c.userData.src) c.matrix.copy((c.userData.src as Mesh).matrixWorld);
    this.renderer.render(this.scene, this.camera());
  }

  private draw() {
    this.frame = 0;
    this.render();
    for (const f of this.drawHooks) f();
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
    this.buildPreview();
    this.buildSlabs();
    this.restyle();
    const box = new Box3().setFromObject(root);
    if (!box.isEmpty()) {
      this.box = box;
      this.baseBox = box.clone();
      box.getBoundingSphere(this.bounds);
    }
    // A scene loaded again keeps the tools as they were: the cut, the layers apart.
    this.slotList = [];
    this.applyStack();
    if (this.explodeT > 0) this.setExplodeT(this.explodeT);
    this.rebuildCaps();
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
    for (const s of this.slabs) s.mesh.geometry.dispose();
    this.slabs = [];
    this.substrate = [];
    this.holes = [];
    this.movers = [];
    this.lineSrc.clear();
    this.solids = [];
    this.holeKeys.clear();
    this.silk = { top: false, bottom: false };
    this.clearCaps();
    this.parts.clear();
    this.limits = [];
    this.pickable = [];
    this.infoOf.clear();
    this.board = null;
    this.copper = [];
    this.clearPreview();
    this.focusedLimits.clear();
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
    const laminates: Object3D[] = [];
    root.traverse((o) => {
      const info = readInfo(o.userData);
      if (info?.kind === 'board') this.board = info as BoardInfo;
      if (info?.kind === 'substrate') laminates.push(o);
    });
    // The laminate's own nodes (its masks, barrels, silkscreen) beside it instead of under it, so
    // the laminate can be hidden (a section, the layers apart) and they stay. No node of the file
    // has a transform, so nothing moves.
    for (const lam of laminates) for (const c of [...lam.children]) if (readInfo(c.userData)) lam.parent?.attach(c);
    root.traverse((o) => {
      if (!(o instanceof Mesh)) return;
      const owner = kindOf(o);
      const info = owner?.info;
      const name = (Array.isArray(o.material) ? o.material[0] : o.material)?.name ?? '';
      const part = info?.kind === 'part' ? (info as PartInfo3d) : null;
      // The silkscreen is drawn in a colour of its own: the `mark` of a body's pin-1 dot does not read on the mask in every theme.
      const role = info?.kind === 'silk' && name === 'mark' ? 'silk' : roleOf(name, part ? part.provenance : null);
      o.userData.role = role;
      if (role === 'body-unknown') planarUv(o);
      if (role && LOOKS[role].hatch) o.renderOrder = 2;
      if (role === 'limit' || role === 'limit-violated') o.renderOrder = 3;
      const lines: LineSegments[] = [];
      if (info?.kind === 'part' || info?.kind === 'substrate' || info?.kind === 'height_limit') {
        lines.push(this.edgeLines(o, role === 'body-unknown' ? 'edge-unknown' : info.kind === 'height_limit' ? (role ?? 'edge') : 'edge'));
      }
      // What the tools need: the laminate, the barrels, what moves with which layer, what the measure tool snaps to.
      const raw = ((owner?.node.userData as { netsmith?: Record<string, unknown> })?.netsmith ?? {}) as Record<string, unknown>;
      const face = raw.face === 'bottom' ? 'bottom' : 'top';
      if (info?.kind === 'substrate') this.substrate.push(o);
      if (info?.kind === 'hole') {
        this.holes.push(o);
        if (typeof raw.refdes === 'string' && typeof raw.pad === 'string') this.holeKeys.add(`${raw.refdes}.${raw.pad}`);
      }
      const slot =
        info?.kind === 'part'
          ? `face:${part!.side}`
          : info?.kind === 'height_limit'
            ? `face:${face}`
            : info?.kind === 'mask'
              ? `mask:${face}`
              : info?.kind === 'silk'
                ? `silk:${face}`
                : info?.kind === 'copper' && typeof raw.layer === 'string'
                  ? raw.layer
                  : info?.kind === 'protrusion' && typeof raw.path === 'string'
                    ? `face:${this.parts.get(raw.path)?.info.side === 'bottom' ? 'top' : 'bottom'}`
                    : null;
      if (info?.kind === 'silk') this.silk[face] = true;
      if (slot) this.movers.push({ obj: o, base: o.position.y, slot });
      if (role !== 'limit' && role !== 'limit-violated') this.solids.push(o);
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
      } else if (info?.kind === 'copper' || info?.kind === 'hole') {
        this.copper.push({ mesh: o, info: info as CopperInfo, exposed: role === 'finish' });
        this.infoOf.set(o, info);
      }
    });
  }

  /** The net of a piece of copper or a barrel: its own, or its pad's. */
  private netOf(info: CopperInfo): string | null {
    return pieceNet(info, this.padNets);
  }

  /** The nets of the pads, `path:pad` to net (the 2D scene has them). */
  setPadNets(nets: Map<string, string>) {
    this.padNets = nets;
    this.restyle();
    this.request();
  }

  /** The nets that have copper or a barrel in the scene, by name. */
  netList(): string[] {
    return [...new Set(this.copper.map((c) => this.netOf(c.info)).filter((n): n is string => n !== null))].sort();
  }

  /** What a lit net lights now (for a recipe): the layers with its copper lit, its barrels lit (vias and pads), and how many other pieces of copper are dimmed. */
  litReport() {
    return litReport(this.copper.map((c) => c.info), this.look.nets, this.padNets);
  }

  /** Edge lines for a mesh, kept on it as the mesh moves. */
  private edgeLines(o: Mesh, edgeRole: string): LineSegments {
    const line = new LineSegments(new EdgesGeometry(o.geometry, 28));
    line.matrixAutoUpdate = false;
    line.matrix.copy(o.matrixWorld);
    line.userData.edgeRole = edgeRole;
    if (edgeRole === 'edge-unknown') line.computeLineDistances();
    line.renderOrder = 4;
    this.edges.add(line);
    this.lineSrc.set(line, o);
    return line;
  }

  /** The stackup's layers as slabs: the laminate's own shape (outline, holes, cutouts) at each
   * layer's height and thickness, from the root's `layers`. They stand in for the one laminate of
   * the file in a section (each layer in its colour, as `netsmith render --section` draws it) and
   * when the stackup comes apart; a copper layer comes apart as a see-through sheet. */
  private buildSlabs() {
    const layers = this.board?.layers ?? [];
    if (!layers.length || !this.substrate.length) return;
    for (const sub of this.substrate) {
      sub.geometry.computeBoundingBox();
      const bb = sub.geometry.boundingBox!;
      const [y0, y1] = [bb.min.y, bb.max.y];
      const parent = sub.parent ?? this.model!;
      const tol = 1e-7;
      layers.forEach((layer, i) => {
        if (layer.kind === 'mask') return;
        const [bottom, top] = [layer.zBottomMm * 0.001, layer.zTopMm * 0.001];
        const inside = top <= y1 + tol && bottom >= y0 - tol;
        const add = (ghost: boolean) => {
          const mesh = new Mesh(slabOf(sub.geometry, y0, y1, bottom, top));
          mesh.position.copy(sub.position);
          mesh.quaternion.copy(sub.quaternion);
          mesh.scale.copy(sub.scale);
          mesh.userData.role = ghost ? 'copper' : slabRole(layers, i);
          mesh.userData.ghost = ghost;
          mesh.visible = false;
          parent.add(mesh);
          mesh.updateMatrixWorld(true);
          this.slabs.push({ mesh, layer, ghost });
          this.movers.push({ obj: mesh, base: mesh.position.y, slot: layer.name });
          if (!ghost) this.solids.push(mesh);
          const line = this.edgeLines(mesh, 'edge');
          line.visible = false;
        };
        if (inside) add(false);
        if (layer.kind === 'copper') add(true);
      });
    }
  }

  /** Which of the file's meshes and the slabs are shown, for the section and the exploded stackup. */
  private applyStack() {
    const apart = this.explodeT > 0 || this.explodeTo > 0;
    const slabs = this.slabs.length > 0 && (apart || this.cut !== null);
    for (const m of this.substrate) m.visible = !slabs;
    for (const s of this.slabs) s.mesh.visible = slabs && (s.ghost ? apart : !(apart && s.layer.kind === 'copper'));
    for (const h of this.holes) h.visible = !apart;
    for (const [line, src] of this.lineSrc) line.visible = shown(src);
  }

  // ---- the section ----

  /** Cut the scene across an axis at a position in mm of the scene's frame, or no more (null). */
  setSection(cut: { axis: Axis; at: number } | null) {
    const was = this.cut;
    this.cut = cut;
    this.renderer.clippingPlanes = cut ? [this.plane] : [];
    if (!was !== !cut) {
      this.applyStack();
      this.rebuildCaps();
    } else this.placeCut();
    this.request();
  }

  /** The direction from the target to the camera, in the scene's frame. */
  towardEye(): P3 {
    const d = this.direction();
    return sceneOf(new Vector3(d[0], d[1], d[2]));
  }

  /** The plane where the cut is, keeping the side away from the camera (as `netsmith render` does), and the caps on it. */
  private placeCut() {
    if (!this.cut) return;
    const u = AXIS_DIR[this.cut.axis];
    const below = keepBelow(this.towardEye(), this.cut.axis);
    this.plane.normal.copy(u).multiplyScalar(below ? -1 : 1);
    this.plane.constant = below ? this.cut.at : -this.cut.at;
    const c = this.bounds.center.clone();
    const on = c.sub(u.clone().multiplyScalar(u.dot(c))).add(u.clone().multiplyScalar(this.cut.at));
    // A hair to the kept side, so the global clipping keeps the caps themselves.
    on.add(this.plane.normal.clone().multiplyScalar(1e-3));
    for (const cap of this.caps.children) {
      if (!cap.userData.cap) continue;
      cap.position.copy(on);
      cap.quaternion.setFromUnitVectors(new Vector3(0, 0, 1), u);
      cap.updateMatrix();
    }
  }

  private clearCaps() {
    for (const m of this.capMaterials) m.dispose();
    this.capMaterials = [];
    this.capGeometry?.dispose();
    this.capGeometry = null;
    this.caps.clear();
  }

  /** The caps of the cut, one fill per colour: each closed mesh shown counts itself into the stencil
   * buffer (its back faces up, its front faces down), and a plane in its colour is drawn where the
   * count is not zero, which is where the cut goes through the solid (three.js's clipping-stencil
   * example). The layers' slabs give the stackup's bands at their true thickness. */
  private rebuildCaps() {
    this.clearCaps();
    if (!this.cut || !this.model || !this.colours) return;
    const groups = new Map<string, { meshes: Mesh[]; colour: Color; role: Role | null }>();
    this.model.traverse((o) => {
      if (!(o instanceof Mesh) || !shown(o) || o.userData.ghost) return;
      const role = o.userData.role as Role | null;
      const mat = (Array.isArray(o.material) ? o.material[0] : o.material) as MeshStandardMaterial;
      if (role ? LOOKS[role].opacity < 1 || LOOKS[role].hatch : mat.transparent) return;
      let closed = this.closed.get(o.geometry);
      if (closed === undefined) this.closed.set(o.geometry, (closed = isClosed(o.geometry)));
      if (!closed) return;
      const key = role ?? `file:${mat.name}`;
      let g = groups.get(key);
      if (!g) groups.set(key, (g = { meshes: [], colour: role ? new Color(this.colours![role]) : (mat.color?.clone() ?? new Color('gray')), role }));
      g.meshes.push(o);
    });
    const keys = [...groups.keys()].sort((a, b) => (CAP_ORDER.indexOf(a) + 1 || 99) - (CAP_ORDER.indexOf(b) + 1 || 99));
    const pass = (side: Side, op: StencilOp) =>
      new MeshBasicMaterial({ side, depthWrite: false, depthTest: false, colorWrite: false, stencilWrite: true, stencilFunc: AlwaysStencilFunc, stencilFail: op, stencilZFail: op, stencilZPass: op });
    const back = pass(BackSide, IncrementWrapStencilOp);
    const front = pass(FrontSide, DecrementWrapStencilOp);
    this.capMaterials.push(back, front);
    const size = this.bounds.radius * 6 + 10;
    this.capGeometry = new PlaneGeometry(size, size);
    let order = -1000;
    for (const key of keys) {
      const g = groups.get(key)!;
      for (const m of g.meshes) {
        for (const mat of [back, front]) {
          const c = new Mesh(m.geometry, mat);
          c.matrixAutoUpdate = false;
          c.matrix.copy(m.matrixWorld);
          c.renderOrder = order;
          c.userData.src = m;
          c.raycast = () => {};
          this.caps.add(c);
        }
      }
      const look = g.role ? LOOKS[g.role] : { metalness: 0.2, roughness: 0.7 };
      const fill = new MeshStandardMaterial({
        color: g.colour,
        metalness: look.metalness,
        roughness: look.roughness,
        side: DoubleSide,
        stencilWrite: true,
        stencilRef: 0,
        stencilFunc: NotEqualStencilFunc,
        stencilFail: ReplaceStencilOp,
        stencilZFail: ReplaceStencilOp,
        stencilZPass: ReplaceStencilOp,
      });
      this.capMaterials.push(fill);
      const cap = new Mesh(this.capGeometry, fill);
      cap.matrixAutoUpdate = false;
      cap.renderOrder = order + 1;
      cap.userData.cap = key;
      cap.raycast = () => {};
      this.caps.add(cap);
      order += 2;
    }
    this.placeCut();
  }

  /** Whether a point of the view's frame is on the side of the cut that is drawn. */
  private kept(p: Vector3): boolean {
    return !this.cut || this.plane.distanceToPoint(p) >= -1e-6;
  }

  /** The scene's box in its own frame (as the file has it, not pulled apart). */
  sceneBox(): { min: P3; max: P3 } {
    const a = sceneOf(this.baseBox.min);
    const b = sceneOf(this.baseBox.max);
    return { min: [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.min(a[2], b[2])], max: [Math.max(a[0], b[0]), Math.max(a[1], b[1]), Math.max(a[2], b[2])] };
  }

  // ---- the exploded stackup ----

  /** The layers, each with where it is now (its middle, mm in z) and how far it has moved; empty when the file has no stackup. */
  slots(): (Slot & { now: number })[] {
    return this.stackSlots().map((s) => ({ ...s, now: (s.zTop + s.zBottom) / 2 + s.offset * easeInOut(this.explodeT) }));
  }

  private stackSlots(): Slot[] {
    const b = this.board;
    if (!this.slotList.length && b?.layers.length) this.slotList = explodeSlots(b.layers, this.silk, explodeGap(b.widthMm, b.heightMm));
    return this.slotList;
  }

  /** Whether the file has the stackup the exploded view needs. */
  canExplode(): boolean {
    return this.stackSlots().length > 0;
  }

  /** Pull the stackup apart (true) or put it back, eased; at once with reduced motion. */
  setExploded(on: boolean) {
    if (!this.canExplode()) return;
    this.explodeTo = on ? 1 : 0;
    this.applyStack();
    if (this.cut) this.rebuildCaps();
    cancelAnimationFrame(this.explodeAnim);
    this.explodeAnim = 0;
    if (this.still) {
      this.setExplodeT(this.explodeTo);
      return;
    }
    const from = this.explodeT;
    const start = performance.now();
    const step = (now: number) => {
      const k = Math.min(1, (now - start) / (EXPLODE_MS * Math.max(0.2, Math.abs(this.explodeTo - from))));
      this.setExplodeT(from + (this.explodeTo - from) * k);
      this.explodeAnim = k < 1 ? requestAnimationFrame(step) : 0;
    };
    this.explodeAnim = requestAnimationFrame(step);
  }

  isExploded(): boolean {
    return this.explodeTo > 0;
  }

  /** Where the layers are, from together (0) to apart (1). */
  private setExplodeT(t: number) {
    this.explodeT = t;
    const e = easeInOut(t);
    const slots = this.stackSlots();
    const byKey = new Map(slots.map((s) => [s.key, s]));
    const layers = this.board?.layers ?? [];
    const maskName = (face: 'top' | 'bottom') => (face === 'top' ? layers.find((l) => l.kind === 'mask') : [...layers].reverse().find((l) => l.kind === 'mask'))?.name;
    const slotOf = (key: string): Slot | null | undefined =>
      key === 'face:top' ? faceSlot(slots, 'top') : key === 'face:bottom' ? faceSlot(slots, 'bottom') : key.startsWith('mask:') ? byKey.get(maskName(key === 'mask:top' ? 'top' : 'bottom') ?? '') : byKey.get(key);
    for (const m of this.movers) m.obj.position.y = m.base + (slotOf(m.slot)?.offset ?? 0) * e * 0.001;
    this.model?.updateMatrixWorld(true);
    for (const [line, src] of this.lineSrc) line.matrix.copy(src.matrixWorld);
    if (t === 0 && this.explodeTo === 0) {
      this.applyStack();
      if (this.cut) this.rebuildCaps();
    }
    if (this.model && (t === 0 || t === 1)) {
      const box = t === 0 ? this.baseBox.clone() : new Box3().setFromObject(this.model);
      if (!box.isEmpty()) this.box = box;
    }
    this.request();
  }

  // ---- the measure tool ----

  /** The pads' centres the measure tool snaps to (from the 2D scene); a pad with a plated hole is on both faces. */
  setPads(pads: PadPoint[]) {
    const t = this.board?.thicknessMm ?? 1.6;
    const out: PadPoint[] = [];
    for (const p of pads) {
      out.push(p);
      if (this.holeKeys.has(`${p.refdes}.${p.pad}`)) {
        const other = pads.find((q) => q.side !== p.side)?.at[2];
        const coppers = (this.board?.layers ?? []).filter((l) => l.kind === 'copper');
        const z = p.side === 'top' ? (coppers[coppers.length - 1]?.zBottomMm ?? other ?? -t) : (coppers[0]?.zTopMm ?? other ?? 0);
        out.push({ ...p, side: p.side === 'top' ? 'bottom' : 'top', at: [p.at[0], p.at[1], z] });
      }
    }
    this.pads = out;
  }

  /** A point of the scene's frame in the window, relative to the canvas, and whether it is in front of the camera. */
  toScreen(p: P3): { x: number; y: number; front: boolean } {
    const v = worldOf(p).project(this.camera());
    const s = this.renderer.getSize(new Vector2());
    return { x: ((v.x + 1) / 2) * s.x, y: ((1 - v.y) / 2) * s.y, front: v.z < 1 && v.z > -1 };
  }

  /** What a point of the window snaps to: a pad's centre, a corner, the board's edge, the top of a
   * body, a face of the board, or the surface under the pointer; null off the scene. */
  snap(clientX: number, clientY: number): Snap | null {
    const r = this.canvas.getBoundingClientRect();
    if (!this.model || r.width === 0) return null;
    const at = new Vector2(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1);
    const cam = this.camera();
    this.ray.setFromCamera(at, cam);
    const hit = this.ray.intersectObjects(this.solids.filter(shown), false).find((h) => this.kept(h.point));
    if (!hit || !hit.face) return null;
    const px = (p: P3) => {
      const s = this.toScreen(p);
      return Math.hypot(s.x - (clientX - r.left), s.y - (clientY - r.top));
    };
    const b = this.board;
    const W = b?.widthMm ?? 0;
    const H = b?.heightMm ?? 0;
    const T = b?.thicknessMm ?? 0;
    const owner = this.infoOf.get(hit.object);
    const part = owner?.kind === 'part' ? (owner as PartInfo3d) : null;
    const onBoard = !part;
    const found: { snap: Snap; px: number }[] = [];
    const p = sceneOf(hit.point);
    const eye = cam.position;
    const far = eye.distanceTo(hit.point) + 0.35;
    // Pads: those that are not behind what the pointer is on.
    if (this.explodeT === 0)
      for (const pad of this.pads) {
        if (eye.distanceTo(worldOf(pad.at)) > far) continue;
        found.push({ snap: { kind: 'pad', at: pad.at, label: `Pad ${pad.refdes}.${pad.pad}` }, px: px(pad.at) });
      }
    // Corners: the hit triangle's vertices (a board's only where they are on its outline's box).
    const pos = (hit.object as Mesh).geometry.getAttribute('position');
    for (const i of [hit.face.a, hit.face.b, hit.face.c]) {
      const v = sceneOf(new Vector3(pos.getX(i), pos.getY(i), pos.getZ(i)).applyMatrix4(hit.object.matrixWorld));
      if (onBoard && !(Math.min(Math.abs(v[0]), Math.abs(v[0] - W)) < 1e-3 && Math.min(Math.abs(v[1]), Math.abs(v[1] - H)) < 1e-3)) continue;
      found.push({ snap: { kind: 'corner', at: v, label: part ? `Corner of ${part.refdes}` : 'Board corner' }, px: px(v) });
    }
    const n = hit.face.normal.clone().applyMatrix3(new Matrix3().getNormalMatrix(hit.object.matrixWorld)).normalize();
    const up = n.y;
    // The board's edge: its side, or the face near the outline.
    if (onBoard && this.explodeT === 0) {
      const cl = (v: number, a: number) => Math.min(a, Math.max(0, v));
      const edges: P3[] = [
        [0, cl(p[1], H), p[2]],
        [W, cl(p[1], H), p[2]],
        [cl(p[0], W), 0, p[2]],
        [cl(p[0], W), H, p[2]],
      ];
      for (const e of edges) found.push({ snap: { kind: 'edge', at: e, label: 'Board edge' }, px: px(e) });
    }
    if (part && Math.abs(up) > 0.9 && (part.side === 'top') === up > 0 && part.heightMm !== null) {
      const z = part.side === 'top' ? part.heightMm : -T - part.heightMm;
      if (Math.abs(p[2] - z - this.liftOf(hit.object)) < 0.05) found.push({ snap: { kind: 'top', at: [p[0], p[1], z], label: `Top of ${part.refdes}` }, px: 0 });
    }
    if (onBoard && Math.abs(up) > 0.9 && this.explodeT === 0) {
      if (up > 0 && Math.abs(p[2]) < 0.06) found.push({ snap: { kind: 'face', at: [p[0], p[1], 0], label: 'Top face' }, px: 0 });
      if (up < 0 && Math.abs(p[2] + T) < 0.06) found.push({ snap: { kind: 'face', at: [p[0], p[1], -T], label: 'Bottom face' }, px: 0 });
    }
    found.push({ snap: { kind: 'surface', at: p, label: part ? `On ${part.refdes}` : 'On the board' }, px: 0 });
    return bestSnap(found);
  }

  /** How far a mesh is lifted now by the exploded stackup, in mm. */
  private liftOf(o: Object3D): number {
    const m = this.movers.find((x) => x.obj === o);
    return m ? (o.position.y - m.base) * 1000 : 0;
  }

  /** A part's box in the scene's frame, from its meshes. */
  partBox(path: string): { min: P3; max: P3 } | null {
    const p = this.parts.get(path);
    if (!p) return null;
    const box = new Box3();
    for (const m of p.meshes) box.expandByObject(m);
    if (box.isEmpty()) return null;
    const a = sceneOf(box.min);
    const b = sceneOf(box.max);
    return { min: [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.min(a[2], b[2])], max: [Math.max(a[0], b[0]), Math.max(a[1], b[1]), Math.max(a[2], b[2])] };
  }

  /** Each height limit's volume in plan, from its mesh. */
  limitPlans(): LimitPlan[] {
    return this.limits.map((l) => {
      const box = new Box3().setFromObject(l.mesh);
      const a = sceneOf(box.min);
      const b = sceneOf(box.max);
      return { info: l.info, min: [Math.min(a[0], b[0]), Math.min(a[1], b[1])], max: [Math.max(a[0], b[0]), Math.max(a[1], b[1])] };
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
    this.rebuildCaps();
    this.request();
  }

  private material(key: string, make: () => Material): Material {
    let m = this.materials.get(key);
    if (!m) this.materials.set(key, (m = make()));
    return m;
  }

  private surface(role: Role, state: State, touch: Touch = {}): Material {
    const c = this.colours!;
    const see = touch.xray ? xrayOpacity(role) : null;
    const key = `${role}:${state}:${see ?? ''}:${touch.dim ? 'dim' : ''}:${touch.tint ?? ''}`;
    return this.material(key, () => {
      const look = LOOKS[role];
      let base = new Color(touch.tint ?? c[role]);
      if (touch.dim && state === 'rest') base = base.lerp(new Color(c.background), DIM.toward);
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
      if (see !== null && state === 'rest') {
        m.transparent = true;
        m.opacity = Math.min(m.opacity, see);
        m.depthWrite = false;
      } else if (see !== null) {
        // A picked or hovered body stays readable in x-ray, a little see-through.
        m.transparent = true;
        m.opacity = Math.min(m.opacity, 0.6);
        m.depthWrite = false;
      }
      if (touch.dim && state === 'rest') {
        m.transparent = true;
        m.opacity = Math.min(m.opacity, DIM.opacity);
        m.depthWrite = false;
      }
      if ((role === 'copper' || role === 'finish') && state !== 'rest') m.emissive = new Color(state === 'select' ? c.select : c.hover).multiplyScalar(0.35);
      if (role === 'limit' || role === 'limit-violated') {
        // The volume's floor is the board's face: it stays behind the mask instead of flickering through it.
        m.polygonOffset = true;
        m.polygonOffsetFactor = 2;
        m.polygonOffsetUnits = 4;
      }
      return m;
    });
  }

  /** A copper layer of the exploded stackup: a see-through sheet in the copper's colour (the copper itself is in its own meshes, where the file has them). */
  private ghost(): Material {
    const c = this.colours!;
    return this.material('ghost:copper', () => new MeshStandardMaterial({ name: 'copper-layer', color: new Color(c.copper), metalness: 0.4, roughness: 0.5, transparent: true, opacity: 0.32, depthWrite: false, side: DoubleSide }));
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
      if (role === 'edge-preview') {
        return new LineDashedMaterial({ color: new Color(c.preview), dashSize: 0.0006, gapSize: 0.0004, transparent: true, opacity: 0.95 });
      }
      if (role === 'edge-unknown' && state === 'rest') {
        return new LineDashedMaterial({ color: new Color(colour), dashSize: 0.0007, gapSize: 0.0005, transparent: true, opacity });
      }
      return new LineBasicMaterial({ color: new Color(colour), transparent: opacity < 1, opacity });
    });
  }

  private stateOf(path: string): State {
    return this.selected.has(path) ? 'select' : this.hovered.has(path) ? 'hover' : 'rest';
  }

  /** A body's colour when the bodies are coloured by kind or by margin; null for the role's own. */
  private tintOf(part: PartInfo3d, role: Role): string | null {
    if (role !== 'body' && role !== 'body-catalog') return null;
    const c = this.colours!;
    if (this.look.colourBy === 'kind') return c.kinds[kindSwatch(part.category)];
    if (this.look.colourBy === 'margin') return marginColour(part.marginMm, c.margin);
    return null;
  }

  /** Give every mesh and line the material of its role and state. */
  private restyle() {
    if (!this.colours || !this.model) return;
    const nets = this.look.nets;
    const netsOn = nets.size > 0;
    // While a net is lit, the board and the bodies are see-through, as in x-ray, so its copper shows on every layer.
    const xray = this.look.xray || netsOn;
    this.model.traverse((o) => {
      if (!(o instanceof Mesh)) return;
      const role = o.userData.role as Role | null;
      if (!role) return;
      if (o.userData.ghost) {
        o.material = this.ghost();
        return;
      }
      const info = this.infoOf.get(o);
      if (info?.kind === 'part') {
        const part = info as PartInfo3d;
        const state = this.stateOf(part.path);
        o.material = this.surface(role, state, { xray, dim: netsOn && state === 'rest', tint: this.tintOf(part, role) });
      } else if (info?.kind === 'copper' || info?.kind === 'hole') {
        const net = this.netOf(info as CopperInfo);
        const state: State = (net !== null && nets.get(net)) || 'rest';
        o.material = this.surface(role, state, { xray, dim: netsOn && state === 'rest' });
      } else if (info?.kind === 'height_limit') {
        const l = info as LimitInfo;
        o.material = this.focusedLimits.has(`${l.name}:${l.face}`) ? this.limitFocus(role) : this.surface(role, 'rest');
      } else {
        o.material = this.surface(role, 'rest', { xray, dim: netsOn });
      }
    });
    for (const m of this.previews) m.material = this.previewMaterial();
    for (const l of this.edges.children as LineSegments[]) l.material = this.line(l.userData.edgeRole, 'rest');
    for (const [path, p] of this.parts) {
      const state = this.stateOf(path);
      if (state !== 'rest') for (const l of p.lines) l.material = this.line(l.userData.edgeRole, state);
    }
  }

  /** Change how the scene is looked at: x-ray, the colouring, the lit nets, the height preview. */
  setLook(look: Partial<Look>) {
    const was = this.look.previewMm;
    this.look = { ...this.look, ...look };
    if (this.look.previewMm !== was) this.buildPreview();
    this.restyle();
    this.request();
  }

  // ---- the height preview (ADR 0105): the parts of unknown height stood up at N mm, in the picture only ----

  private clearPreview() {
    for (const m of this.previews) {
      m.removeFromParent();
      m.geometry.dispose();
      this.infoOf.delete(m);
    }
    for (const l of this.previewEdges) {
      l.removeFromParent();
      l.geometry.dispose();
    }
    this.pickable = this.pickable.filter((m) => !m.userData.preview);
    this.previews = [];
    this.previewEdges = [];
  }

  /** Solids for the parts of unknown height, each its flat outline stood up from its face, drawn striped and see-through in the preview colour. They are added beside the outline and read by nothing but the picture. */
  private buildPreview() {
    this.clearPreview();
    const mm = this.look.previewMm;
    if (mm === null || !this.model) return;
    const t = (this.board?.thicknessMm ?? 1.6) * 0.001;
    for (const p of this.parts.values()) {
      if (p.info.provenance !== 'unknown') continue;
      for (const flat of p.meshes) {
        if (flat.userData.role !== 'body-unknown' || !flat.parent) continue;
        const pos = flat.geometry.getAttribute('position');
        const index = flat.geometry.getIndex();
        const [y0, y1] = p.info.side === 'bottom' ? [-t, -t - mm * 0.001] : [0, mm * 0.001];
        const solid = extrudeFlat(pos.array, index ? index.array : null, y0, y1);
        const geo = new BufferGeometry();
        geo.setAttribute('position', new BufferAttribute(solid, 3));
        geo.computeVertexNormals();
        const uv = new Float32Array((solid.length / 3) * 2);
        for (let i = 0; i < solid.length / 3; i++) {
          const [x, y, z] = [solid[i * 3], solid[i * 3 + 1], solid[i * 3 + 2]];
          uv[i * 2] = ((x + y) * 1000) / HATCH_MM;
          uv[i * 2 + 1] = ((z - y) * 1000) / HATCH_MM;
        }
        geo.setAttribute('uv', new BufferAttribute(uv, 2));
        const mesh = new Mesh(geo);
        mesh.userData.preview = true;
        mesh.renderOrder = 3;
        flat.parent.add(mesh);
        mesh.updateMatrixWorld(true);
        this.previews.push(mesh);
        this.pickable.push(mesh);
        this.infoOf.set(mesh, p.info);
        const line = new LineSegments(new EdgesGeometry(geo, 28));
        line.matrixAutoUpdate = false;
        line.matrix.copy(mesh.matrixWorld);
        line.userData.edgeRole = 'edge-preview';
        line.computeLineDistances();
        line.renderOrder = 4;
        this.edges.add(line);
        this.previewEdges.push(line);
      }
    }
  }

  private previewMaterial(): Material {
    const c = this.colours!;
    return this.material('preview', () => {
      const m = new MeshStandardMaterial({
        name: 'preview',
        color: new Color(c.preview),
        metalness: 0,
        roughness: 0.9,
        transparent: true,
        opacity: 0.62,
        depthWrite: false,
        side: DoubleSide,
        map: this.hatch,
        alphaTest: 0.05,
      });
      m.emissive = new Color(c.preview).multiplyScalar(0.12);
      return m;
    });
  }

  /** The fills of the section's caps that are drawn, by role (for a recipe: x-ray keeps them). */
  capList(): string[] {
    return this.caps.children.filter((c) => c.userData.cap && c.visible).map((c) => c.userData.cap as string);
  }

  /** How many parts the preview stands up (for the label and the recipes). */
  previewCount(): number {
    return new Set(this.previews.map((m) => (this.infoOf.get(m) as PartInfo3d).path)).size;
  }

  // ---- a finding of the problems dock, shown in 3D ----

  /** A limit lit for a finding: its volume brighter, pulsed once when it is first lit. */
  private limitFocus(role: Role): Material {
    const c = this.colours!;
    return this.material(`focus:${role}`, () => {
      const m = new MeshStandardMaterial({
        name: `${role}-focus`,
        color: new Color(c[role]),
        metalness: 0,
        roughness: 1,
        transparent: true,
        opacity: 0.42,
        depthWrite: false,
        side: DoubleSide,
        polygonOffset: true,
        polygonOffsetFactor: 2,
        polygonOffsetUnits: 4,
      });
      m.emissive = new Color(c[role]).multiplyScalar(0.3);
      return m;
    });
  }

  /** Fly to a finding's parts and limits and light them: the parts through the selection (the dock picks them), the limits here. The violated limit's volume pulses once; with reduced motion it does not. */
  focusFinding(paths: string[], limitKeys: string[]) {
    this.focusedLimits = new Set(limitKeys);
    this.restyle();
    const box = new Box3();
    // The parts, where it names some (a limit's volume may be half the board); else the limit.
    for (const path of paths) for (const m of this.parts.get(path)?.meshes ?? []) box.expandByObject(m);
    if (box.isEmpty()) for (const l of this.limits) if (this.focusedLimits.has(`${l.info.name}:${l.info.face}`)) box.expandByObject(l.mesh);
    if (!box.isEmpty()) {
      box.expandByScalar(Math.max(0, 10 - box.getSize(new Vector3()).length() / 4));
      const dir = this.direction();
      const f = this.framing(box, dir);
      this.moveTo(f.target, dir, f.dist, f.zoom, true);
    }
    cancelAnimationFrame(this.pulseAnim);
    const violated = this.limits.some((l) => l.info.violated && this.focusedLimits.has(`${l.info.name}:${l.info.face}`));
    if (!violated || this.still) {
      this.request();
      return;
    }
    const mats = [this.limitFocus('limit-violated')] as MeshStandardMaterial[];
    const start = performance.now();
    const step = (now: number) => {
      const k = Math.min(1, (now - start) / PULSE_MS);
      const swell = Math.sin(Math.PI * k);
      for (const m of mats) m.opacity = 0.42 + 0.4 * swell;
      this.request();
      if (k < 1) this.pulseAnim = requestAnimationFrame(step);
    };
    this.pulseAnim = requestAnimationFrame(step);
  }

  /** The limits a finding lit, as `name:face`. */
  focusedList(): string[] {
    return [...this.focusedLimits];
  }

  clearFocus() {
    if (!this.focusedLimits.size) return;
    cancelAnimationFrame(this.pulseAnim);
    this.focusedLimits.clear();
    this.restyle();
    this.request();
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
    // The copper is picked where it is bare (a pad, the gold), and anywhere while it is seen through the board.
    const seeThrough = this.look.xray || this.look.nets.size > 0;
    const copper = this.copper.filter((c) => this.netOf(c.info) !== null && (c.exposed || seeThrough)).map((c) => c.mesh);
    const hits = this.ray.intersectObjects([...this.pickable, ...copper], false);
    let limit: LimitInfo | null = null;
    for (const h of hits) {
      if (!this.kept(h.point) || !shown(h.object)) continue;
      const info = this.infoOf.get(h.object);
      if (info?.kind === 'part') return { kind: 'part', info: info as PartInfo3d };
      if (info?.kind === 'copper' || info?.kind === 'hole') {
        const net = this.netOf(info as CopperInfo);
        if (net !== null) return { kind: 'copper', info: { ...(info as CopperInfo), net } };
      }
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
    this.anim = 0;
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
      this.anim = k < 1 ? requestAnimationFrame(step) : 0;
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

  // ---- the camera as an agent sets and reads it (`/api/view`, docs/BOARD3D.md), in the scene's frame ----

  /** Look at the whole board from a direction of the scene's frame (from the target to the eye), as
   * `netsmith render --view` does. A view straight down or up keeps the board's top edge up. */
  lookFrom(towardEye: P3, animate = true) {
    const w = worldOf(towardEye);
    let dir: Vec3 = [w.x, w.y, w.z];
    const l = Math.hypot(...dir) || 1;
    dir = [dir[0] / l, dir[1] / l, dir[2] / l];
    if (Math.abs(dir[1]) > 0.9999) dir = presetDirection(dir[1] > 0 ? 'top' : 'bottom');
    const f = this.framing(this.box, dir);
    this.moveTo(f.target, dir, f.dist, f.zoom, animate);
  }

  /** Put the eye at `position` looking at `target` (scene frame, mm). An orthographic camera shows
   * at the target what a perspective one there would. */
  place(position: P3, target: P3, animate = true) {
    const p = worldOf(position);
    const t = worldOf(target);
    const d = p.clone().sub(t);
    const dist = Math.max(d.length(), 0.01);
    d.normalize();
    let dir: Vec3 = [d.x, d.y, d.z];
    if (Math.abs(dir[1]) > 0.9999) dir = presetDirection(dir[1] > 0 ? 'top' : 'bottom');
    this.moveTo(t, dir, dist, this.orthoBase / halfHeightAt(dist, FOV), animate);
  }

  /** Where the eye is, what it looks at and the screen's up, in the scene's frame. For an
   * orthographic camera the eye is where a perspective one would show the same at the target. */
  cameraPlace(): { position: P3; target: P3; up: P3 } {
    const t = this.controls.target.clone();
    const dist = this.orthoOn ? distanceFor(this.ortho.top / this.ortho.zoom, FOV) : this.persp.position.distanceTo(t);
    const d = this.direction();
    const eye = new Vector3(t.x + d[0] * dist, t.y + d[1] * dist, t.z + d[2] * dist);
    const cam = this.camera();
    cam.updateMatrixWorld();
    const up = new Vector3().setFromMatrixColumn(cam.matrixWorld, 1).normalize();
    const round = (v: P3): P3 => v.map((x) => Math.round(x * 1000) / 1000 + 0) as P3;
    return { position: round(sceneOf(eye)), target: round(sceneOf(t)), up: round(sceneOf(up)) };
  }

  /** Light height limits by name (both faces of a name), with no camera move; none clears them. */
  lightLimits(names: string[]) {
    const want = new Set(names);
    cancelAnimationFrame(this.pulseAnim);
    this.focusedLimits = new Set(this.limits.filter((l) => want.has(l.info.name)).map((l) => `${l.info.name}:${l.info.face}`));
    this.restyle();
    this.request();
  }

  /** A camera move or the stackup moving is under way. */
  busy(): boolean {
    return this.anim !== 0 || this.explodeAnim !== 0;
  }

  /** Draw now, and what follows a frame (the overlays), whether or not the window is drawing frames. */
  drawNow() {
    cancelAnimationFrame(this.frame);
    this.draw();
  }

  /** A PNG of the canvas as it is drawn now: drawn and copied in the same task, so the drawing
   * buffer (which is not kept between frames) is still there. */
  picture(): Promise<Blob> {
    this.render();
    return new Promise((done, fail) => this.canvas.toBlob((b) => (b ? done(b) : fail(new Error('the canvas gave no picture'))), 'image/png'));
  }

  /** Draw now and count what is on screen (for a recipe: the view is not blank). The pixels are
   * read in the same task as the drawing, so no buffer needs keeping. */
  sample(): { width: number; height: number; drawn: number; colours: number } {
    this.render();
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
    this.render();
    const px = new Uint8Array(4);
    gl.readPixels(x, y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
    return `#${((1 << 24) | (px[0] << 16) | (px[1] << 8) | px[2]).toString(16).slice(1)}`;
  }

  dispose() {
    cancelAnimationFrame(this.frame);
    cancelAnimationFrame(this.anim);
    cancelAnimationFrame(this.pulseAnim);
    cancelAnimationFrame(this.explodeAnim);
    this.drawHooks.clear();
    this.controls.dispose();
    this.clearModel();
    for (const m of this.materials.values()) m.dispose();
    this.hatch.dispose();
    for (const d of this.disposables) d.dispose();
    this.renderer.dispose();
    this.canvas.remove();
  }
}
