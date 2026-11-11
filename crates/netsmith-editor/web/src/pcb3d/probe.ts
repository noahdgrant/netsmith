// SPIKE (#430 P0, not for merge): the three.js a board viewer needs, loaded on demand, rendering a test scene. Opened by `?probe3d`.
import {
  AmbientLight,
  BoxGeometry,
  BufferGeometry,
  Color,
  DirectionalLight,
  Float32BufferAttribute,
  InstancedMesh,
  Matrix4,
  Mesh,
  MeshPhysicalMaterial,
  MeshStandardMaterial,
  OrthographicCamera,
  PerspectiveCamera,
  Plane,
  PMREMGenerator,
  Raycaster,
  Scene,
  Vector2,
  Vector3,
  WebGLRenderer,
} from 'three';
import { RoomEnvironment } from 'three/examples/jsm/environments/RoomEnvironment.js';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';

export interface ProbeResult {
  ok: boolean;
  error?: string;
  webgl?: string;
  glVersion?: string;
  vendor?: string;
  renderer?: string;
  maxTextureSize?: number;
  frameMsAvg?: number;
  frameMsMax?: number;
  frameMsMedian?: number;
  firstFrameMs?: number;
  rafMsAvg?: number;
  nonBackgroundPixels?: number;
  totalPixels?: number;
  glbMeshes?: number;
  raycastHits?: number;
}

/** A GLB of one unit cube (positions and indices), built by hand so no exporter is in the chunk. */
function cubeGlb(): ArrayBuffer {
  const p = [-1, -1, -1, 1, -1, -1, 1, 1, -1, -1, 1, -1, -1, -1, 1, 1, -1, 1, 1, 1, 1, -1, 1, 1].map((v) => v * 0.5);
  const idx = [0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 2, 3, 7, 2, 7, 6, 1, 2, 6, 1, 6, 5, 3, 0, 4, 3, 4, 7];
  const posBytes = p.length * 4;
  const idxBytes = idx.length * 2;
  const binLen = Math.ceil((posBytes + idxBytes) / 4) * 4;
  const bin = new ArrayBuffer(binLen);
  new Float32Array(bin, 0, p.length).set(p);
  new Uint16Array(bin, posBytes, idx.length).set(idx);
  const json = {
    asset: { version: '2.0' },
    scene: 0,
    scenes: [{ nodes: [0] }],
    nodes: [{ mesh: 0, name: 'chip' }],
    meshes: [{ primitives: [{ attributes: { POSITION: 0 }, indices: 1, material: 0 }] }],
    materials: [{ pbrMetallicRoughness: { baseColorFactor: [0.08, 0.08, 0.09, 1], metallicFactor: 0, roughnessFactor: 0.6 } }],
    accessors: [
      { bufferView: 0, componentType: 5126, count: 8, type: 'VEC3', min: [-0.5, -0.5, -0.5], max: [0.5, 0.5, 0.5] },
      { bufferView: 1, componentType: 5123, count: idx.length, type: 'SCALAR' },
    ],
    bufferViews: [
      { buffer: 0, byteOffset: 0, byteLength: posBytes },
      { buffer: 0, byteOffset: posBytes, byteLength: idxBytes },
    ],
    buffers: [{ byteLength: binLen }],
  };
  let text = JSON.stringify(json);
  while (text.length % 4) text += ' ';
  const jsonBytes = new TextEncoder().encode(text);
  const total = 12 + 8 + jsonBytes.length + 8 + binLen;
  const out = new ArrayBuffer(total);
  const dv = new DataView(out);
  dv.setUint32(0, 0x46546c67, true);
  dv.setUint32(4, 2, true);
  dv.setUint32(8, total, true);
  dv.setUint32(12, jsonBytes.length, true);
  dv.setUint32(16, 0x4e4f534a, true);
  new Uint8Array(out, 20, jsonBytes.length).set(jsonBytes);
  const o = 20 + jsonBytes.length;
  dv.setUint32(o, binLen, true);
  dv.setUint32(o + 4, 0x004e4942, true);
  new Uint8Array(out, o + 8, binLen).set(new Uint8Array(bin));
  return out;
}

export async function runProbe(host: HTMLElement): Promise<ProbeResult> {
  host.innerHTML = '';
  const canvas = document.createElement('canvas');
  canvas.id = 'probe3d';
  canvas.style.cssText = 'width:800px;height:600px;display:block';
  host.appendChild(canvas);
  const r: ProbeResult = { ok: false };
  let renderer: WebGLRenderer;
  try {
    renderer = new WebGLRenderer({ canvas, antialias: true, preserveDrawingBuffer: true });
  } catch (e) {
    r.error = `WebGLRenderer: ${(e as Error).message}`;
    return r;
  }
  renderer.setSize(800, 600, false);
  renderer.localClippingEnabled = true;
  const gl = renderer.getContext();
  r.webgl = typeof WebGL2RenderingContext !== 'undefined' && gl instanceof WebGL2RenderingContext ? 'webgl2' : 'webgl1';
  r.glVersion = gl.getParameter(gl.VERSION) as string;
  const dbg = gl.getExtension('WEBGL_debug_renderer_info');
  r.vendor = dbg ? (gl.getParameter(dbg.UNMASKED_VENDOR_WEBGL) as string) : (gl.getParameter(gl.VENDOR) as string);
  r.renderer = dbg ? (gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL) as string) : (gl.getParameter(gl.RENDERER) as string);
  r.maxTextureSize = gl.getParameter(gl.MAX_TEXTURE_SIZE) as number;

  const bg = new Color(0x202428);
  const scene = new Scene();
  scene.background = bg;
  const pmrem = new PMREMGenerator(renderer);
  scene.environment = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;
  scene.add(new AmbientLight(0xffffff, 0.2));
  const sun = new DirectionalLight(0xffffff, 1.5);
  sun.position.set(30, 60, 40);
  scene.add(sun);

  // The board: a green slab with copper-ish physical material on top.
  const board = new Mesh(new BoxGeometry(60, 1.6, 40), new MeshStandardMaterial({ color: 0x1f6b35, roughness: 0.5 }));
  scene.add(board);
  const parts = new InstancedMesh(new BoxGeometry(3, 1.2, 2), new MeshPhysicalMaterial({ color: 0x9a9da3, metalness: 0.9, roughness: 0.3, clearcoat: 0.5 }), 12);
  const m = new Matrix4();
  for (let i = 0; i < 12; i++) parts.setMatrixAt(i, m.makeTranslation(-22 + (i % 6) * 9, 1.4, -10 + Math.floor(i / 6) * 20));
  scene.add(parts);
  // A hand-made BufferGeometry (a pad).
  const pad = new BufferGeometry();
  pad.setAttribute('position', new Float32BufferAttribute([-2, 0.81, -2, 2, 0.81, -2, 2, 0.81, 2, -2, 0.81, -2, 2, 0.81, 2, -2, 0.81, 2], 3));
  pad.computeVertexNormals();
  scene.add(new Mesh(pad, new MeshStandardMaterial({ color: 0xc8a050, metalness: 1, roughness: 0.35, side: 2 })));

  // The GLB (a chip body), scaled and placed on the board; clipped by a plane like a section view.
  const gltf = await new GLTFLoader().parseAsync(cubeGlb(), '');
  r.glbMeshes = 0;
  gltf.scene.traverse((o) => {
    if ((o as Mesh).isMesh) {
      r.glbMeshes!++;
      const mat = (o as Mesh).material as MeshStandardMaterial;
      mat.clippingPlanes = [new Plane(new Vector3(-1, 0, 0), 4)];
    }
  });
  gltf.scene.scale.set(10, 2, 10);
  gltf.scene.position.set(0, 1.8, 0);
  scene.add(gltf.scene);

  const camera = new PerspectiveCamera(40, 800 / 600, 0.1, 1000);
  camera.position.set(50, 45, 60);
  camera.lookAt(0, 0, 0);
  const ortho = new OrthographicCamera(-40, 40, 30, -30, 0.1, 1000);
  ortho.position.set(0, 100, 0);
  ortho.lookAt(0, 0, 0);
  const controls = new OrbitControls(camera, canvas);
  controls.update();

  const ray = new Raycaster();
  ray.setFromCamera(new Vector2(0, 0), camera);
  r.raycastHits = ray.intersectObjects(scene.children, true).length;

  renderer.render(scene, ortho);
  // 60 frames, each waited for with a one-pixel readPixels (gl.finish does not block in Chromium) so the GPU work is counted.
  const one = new Uint8Array(4);
  const times: number[] = [];
  for (let i = 0; i < 60; i++) {
    camera.position.applyAxisAngle(new Vector3(0, 1, 0), 0.01);
    camera.lookAt(0, 0, 0);
    const t0 = performance.now();
    renderer.render(scene, camera);
    gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, one);
    times.push(performance.now() - t0);
  }
  r.frameMsAvg = +(times.reduce((a, b) => a + b, 0) / times.length).toFixed(2);
  r.frameMsMax = +Math.max(...times).toFixed(2);
  r.firstFrameMs = +times[0].toFixed(2);
  r.frameMsMedian = +[...times].sort((a, b) => a - b)[30].toFixed(2);
  // 30 frames through requestAnimationFrame, as an interactive view runs.
  const raf: number[] = [];
  await new Promise<void>((done) => {
    let last = performance.now();
    const tick = () => {
      renderer.render(scene, camera);
      const now = performance.now();
      raf.push(now - last);
      last = now;
      if (raf.length < 30) requestAnimationFrame(tick);
      else done();
    };
    requestAnimationFrame(tick);
  });
  r.rafMsAvg = +(raf.slice(1).reduce((a, b) => a + b, 0) / (raf.length - 1)).toFixed(2);

  const px = new Uint8Array(800 * 600 * 4);
  gl.readPixels(0, 0, 800, 600, gl.RGBA, gl.UNSIGNED_BYTE, px);
  const hex = bg.getHex(); // sRGB, as the canvas holds it
  const [br, bgc, bb] = [(hex >> 16) & 255, (hex >> 8) & 255, hex & 255];
  let differ = 0;
  for (let i = 0; i < px.length; i += 4) {
    if (Math.abs(px[i] - br) + Math.abs(px[i + 1] - bgc) + Math.abs(px[i + 2] - bb) > 24) differ++;
  }
  r.nonBackgroundPixels = differ;
  r.totalPixels = 800 * 600;
  r.ok = differ > 1000;
  return r;
}
