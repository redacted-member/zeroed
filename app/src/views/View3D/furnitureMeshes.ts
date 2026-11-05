// three.js pieces for furniture previews: a panel's mesh as geometry, a
// group of panels in furniture space (Z up), and library thumbnails drawn
// offscreen with one shared renderer. Loaded only with the 3D view, the
// library dialog and the seam popover.

import {
  AmbientLight,
  Box3,
  BufferAttribute,
  BufferGeometry,
  Color,
  DirectionalLight,
  EdgesGeometry,
  Group,
  LineBasicMaterial,
  LineSegments,
  Mesh,
  MeshStandardMaterial,
  PerspectiveCamera,
  Scene,
  Sphere,
  Vector3,
  WebGLRenderer,
} from 'three';

import type { PanelMesh, PreviewPanel } from '../../bridge/furniture';

/** Panel colors by material: the carcass light, the next ones darker. */
const MATERIAL_COLORS = [0xd9b98c, 0xb88d5c, 0x8f6a45, 0xc9a77a];
const EDGE_COLOR = 0x3b2a1a;

/** A panel mesh (furniture space, mm) as geometry. */
export function panelGeometry(mesh: PanelMesh): BufferGeometry {
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new BufferAttribute(new Float32Array(mesh.positions), 3));
  if (mesh.normals.length === mesh.positions.length) {
    geometry.setAttribute('normal', new BufferAttribute(new Float32Array(mesh.normals), 3));
  }
  geometry.setIndex(new BufferAttribute(new Uint32Array(mesh.indices), 1));
  if (mesh.normals.length !== mesh.positions.length) {
    geometry.computeVertexNormals();
  }
  geometry.computeBoundingBox();
  return geometry;
}

/** The color of each material key, in the order they first appear. */
export function materialColors(panels: readonly PreviewPanel[]): Map<string, Color> {
  const colors = new Map<string, Color>();
  for (const p of panels) {
    if (!colors.has(p.material)) {
      const hex = MATERIAL_COLORS[colors.size % MATERIAL_COLORS.length] ?? MATERIAL_COLORS[0];
      colors.set(p.material, new Color(hex));
    }
  }
  return colors;
}

/** The panels as solids with dark edges, in furniture space. */
export function panelsGroup(panels: readonly PreviewPanel[]): Group {
  const group = new Group();
  const colors = materialColors(panels);
  const edgeMaterial = new LineBasicMaterial({ color: EDGE_COLOR });
  for (const panel of panels) {
    const geometry = panelGeometry(panel.mesh);
    const material = new MeshStandardMaterial({
      color: colors.get(panel.material) ?? new Color(MATERIAL_COLORS[0]),
      roughness: 0.8,
      metalness: 0,
    });
    const mesh = new Mesh(geometry, material);
    mesh.name = panel.key;
    group.add(mesh, new LineSegments(new EdgesGeometry(geometry, 30), edgeMaterial));
  }
  return group;
}

/** Frees a group's geometry and materials. */
export function disposeGroup(group: Group): void {
  group.traverse((object) => {
    if (object instanceof Mesh || object instanceof LineSegments) {
      object.geometry.dispose();
      const material = object.material as MeshStandardMaterial | LineBasicMaterial;
      material.dispose();
    }
  });
}

/** A camera from the front left, above, Z up, showing all of `sphere`. */
export function placeCamera(
  camera: PerspectiveCamera,
  sphere: Sphere,
  azimuth: number,
  elevation: number,
): void {
  const distance = (sphere.radius / Math.sin((camera.fov * Math.PI) / 360)) * 1.05;
  camera.up.set(0, 0, 1);
  camera.position.set(
    sphere.center.x + distance * Math.cos(elevation) * Math.cos(azimuth),
    sphere.center.y + distance * Math.cos(elevation) * Math.sin(azimuth),
    sphere.center.z + distance * Math.sin(elevation),
  );
  camera.near = Math.max(distance / 100, 0.1);
  camera.far = distance * 10;
  camera.lookAt(sphere.center);
  camera.updateProjectionMatrix();
}

let shared: { renderer: WebGLRenderer; canvas: HTMLCanvasElement } | null = null;

/**
 * A picture of the panels seen from the front left, as a PNG data URL of
 * `size` CSS pixels square, or null when WebGL is not there.
 */
export function renderThumbnail(panels: readonly PreviewPanel[], size: number): string | null {
  if (panels.length === 0) {
    return null;
  }
  try {
    if (shared === null) {
      const canvas = document.createElement('canvas');
      const renderer = new WebGLRenderer({
        canvas,
        antialias: true,
        alpha: true,
        preserveDrawingBuffer: true,
      });
      shared = { renderer, canvas };
    }
    const { renderer, canvas } = shared;
    const ratio = Math.min(2, window.devicePixelRatio || 1);
    renderer.setPixelRatio(ratio);
    renderer.setSize(size, size, false);
    const scene = new Scene();
    scene.add(new AmbientLight(0xffffff, 1.2));
    const sun = new DirectionalLight(0xffffff, 1.6);
    sun.position.set(-1, -2, 3);
    scene.add(sun);
    const group = panelsGroup(panels);
    scene.add(group);
    const sphere = new Box3().setFromObject(group).getBoundingSphere(new Sphere(new Vector3(), 1));
    const camera = new PerspectiveCamera(30, 1, 1, 100000);
    placeCamera(camera, sphere, -Math.PI * 0.62, Math.PI / 7);
    renderer.setClearColor(0x000000, 0);
    renderer.render(scene, camera);
    const url = canvas.toDataURL('image/png');
    disposeGroup(group);
    return url;
  } catch {
    return null;
  }
}
