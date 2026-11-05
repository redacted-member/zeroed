// three.js objects for the model import preview: the coloured model and a
// see-through plane. Owned by the renderer, which hides the material and the
// relief while the preview shows.

import {
  BufferAttribute,
  BufferGeometry,
  DoubleSide,
  Mesh,
  LineBasicMaterial,
  LineSegments,
  MeshBasicMaterial,
  MeshStandardMaterial,
  type Scene,
} from 'three';

import type { ImportScene } from './importPreview';

const PLANE_COLOUR = 0x3b82f6;
const CYLINDER_COLOUR = 0x2563eb;
const AXIS_COLOUR = 0xdc2626;
const PLANE_OPACITY = 0.28;
/** Mid grey, readable on the light and the dark backgrounds. */
const OUTLINE_COLOUR = 0x8f96a0;

export class ImportLayer {
  private readonly modelMaterial = new MeshStandardMaterial({
    vertexColors: true,
    side: DoubleSide,
    roughness: 0.75,
    metalness: 0,
  });
  private readonly outlineMaterial = new LineBasicMaterial({ color: OUTLINE_COLOUR });
  private readonly cylinderMaterial = new LineBasicMaterial({ color: CYLINDER_COLOUR });
  private readonly axisMaterial = new LineBasicMaterial({ color: AXIS_COLOUR });
  private guides: LineSegments<BufferGeometry, LineBasicMaterial>[] = [];
  private readonly planeMaterial = new MeshBasicMaterial({
    color: PLANE_COLOUR,
    transparent: true,
    opacity: PLANE_OPACITY,
    side: DoubleSide,
    depthWrite: false,
  });
  private model: Mesh<BufferGeometry, MeshStandardMaterial> | null = null;
  private plane: Mesh<BufferGeometry, MeshBasicMaterial> | null = null;
  private outline: LineSegments<BufferGeometry, LineBasicMaterial> | null = null;

  constructor(private readonly scene: Scene) {}

  /** Whether a preview is showing. */
  get active(): boolean {
    return this.model !== null;
  }

  /** Shows `data`, or nothing when null. */
  set(data: ImportScene | null): void {
    this.remove();
    if (data === null) {
      return;
    }
    this.modelMaterial.wireframe = data.wireframe === true;
    const geometry = new BufferGeometry();
    geometry.setAttribute('position', new BufferAttribute(data.positions, 3));
    geometry.setAttribute('color', new BufferAttribute(data.colours, 3));
    geometry.computeVertexNormals();
    this.model = new Mesh(geometry, this.modelMaterial);
    this.scene.add(this.model);
    if (data.plane !== null) {
      const planeGeometry = new BufferGeometry();
      planeGeometry.setAttribute('position', new BufferAttribute(data.plane, 3));
      this.plane = new Mesh(planeGeometry, this.planeMaterial);
      this.plane.renderOrder = 1;
      this.scene.add(this.plane);
    }
    if (data.outline !== null) {
      const outlineGeometry = new BufferGeometry();
      outlineGeometry.setAttribute('position', new BufferAttribute(data.outline, 3));
      this.outline = new LineSegments(outlineGeometry, this.outlineMaterial);
      this.scene.add(this.outline);
    }
    if (data.guides !== undefined) {
      for (const [points, material] of [
        [data.guides.cylinder, this.cylinderMaterial],
        [data.guides.axis, this.axisMaterial],
      ] as const) {
        const geometry = new BufferGeometry();
        geometry.setAttribute('position', new BufferAttribute(points, 3));
        const lines = new LineSegments(geometry, material);
        this.guides.push(lines);
        this.scene.add(lines);
      }
    }
  }

  private remove(): void {
    for (const mesh of [this.model, this.plane, this.outline]) {
      if (mesh !== null) {
        this.scene.remove(mesh);
        mesh.geometry.dispose();
      }
    }
    this.model = null;
    this.plane = null;
    this.outline = null;
    for (const lines of this.guides) {
      this.scene.remove(lines);
      lines.geometry.dispose();
    }
    this.guides = [];
  }

  dispose(): void {
    this.remove();
    this.modelMaterial.dispose();
    this.planeMaterial.dispose();
    this.outlineMaterial.dispose();
    this.cylinderMaterial.dispose();
    this.axisMaterial.dispose();
  }
}
