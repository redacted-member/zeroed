// three.js objects for the Furniture Designer in the Assembly View: the
// fast preview's panels (furniture space turned into the scene's Y up),
// the overall dimension lines, and the seam under the pointer lit as a
// thin bar. Owned by the renderer, drawn over the pieces.

import {
  BufferAttribute,
  BufferGeometry,
  CylinderGeometry,
  Group,
  LineBasicMaterial,
  LineSegments,
  Matrix4,
  Mesh,
  MeshBasicMaterial,
  Quaternion,
  Vector3,
  type Scene,
} from 'three';

import type { PreviewPanel, Vec3 } from '../../bridge/furniture';
import { disposeGroup, panelsGroup } from './furnitureMeshes';
import type { DimensionLine } from './furnitureSeams';

/** Furniture (job) space X, Y, Z up to the scene's X, Z, -Y. */
const TO_SCENE = new Matrix4().set(1, 0, 0, 0, 0, 0, 1, 0, 0, -1, 0, 0, 0, 0, 0, 1);

const DIMENSION_COLOR = 0x3d8bfd;
const SEAM_COLOR = 0xff8a3d;
const SEAM_PICKED_COLOR = 0x3d8bfd;

function scenePoint(p: Vec3): Vector3 {
  return new Vector3(p[0], p[2], -p[1]);
}

export class FurnitureLayer {
  private readonly root = new Group();
  private panels: Group | null = null;
  private dims: LineSegments<BufferGeometry, LineBasicMaterial> | null = null;
  private readonly dimMaterial = new LineBasicMaterial({
    color: DIMENSION_COLOR,
    depthTest: false,
  });
  private seam: Mesh<CylinderGeometry, MeshBasicMaterial> | null = null;
  private readonly seamMaterial = new MeshBasicMaterial({ color: SEAM_COLOR, depthTest: false });

  constructor(private readonly scene: Scene) {
    this.root.renderOrder = 2;
    this.scene.add(this.root);
  }

  /** Shows the preview's panels, or none. */
  setPanels(panels: readonly PreviewPanel[] | null, origin: Vec3 = [0, 0, 0]): void {
    if (this.panels !== null) {
      this.root.remove(this.panels);
      disposeGroup(this.panels);
      this.panels = null;
    }
    if (panels === null || panels.length === 0) {
      return;
    }
    const group = panelsGroup(panels);
    group.matrixAutoUpdate = false;
    // Furniture space sits at the design's origin in the Assembly View.
    group.matrix
      .copy(TO_SCENE)
      .multiply(new Matrix4().makeTranslation(origin[0], origin[1], origin[2]));
    this.root.add(group);
    this.panels = group;
  }

  /** Draws the dimension lines and their extension lines (job space), or none. */
  setDimensions(lines: readonly DimensionLine[]): void {
    if (this.dims !== null) {
      this.root.remove(this.dims);
      this.dims.geometry.dispose();
      this.dims = null;
    }
    if (lines.length === 0) {
      return;
    }
    const points: number[] = [];
    const push = (a: Vec3, b: Vec3) => {
      const p = scenePoint(a);
      const q = scenePoint(b);
      points.push(p.x, p.y, p.z, q.x, q.y, q.z);
    };
    for (const line of lines) {
      push(line.from, line.to);
      for (const [a, b] of line.extensions) {
        push(a, b);
      }
    }
    const geometry = new BufferGeometry();
    geometry.setAttribute('position', new BufferAttribute(new Float32Array(points), 3));
    this.dims = new LineSegments(geometry, this.dimMaterial);
    this.dims.renderOrder = 3;
    this.root.add(this.dims);
  }

  /** Lights a seam (job space ends) as a bar `radius` mm thick, or none; `picked` for the one a popover is open on. */
  setSeam(seam: { from: Vec3; to: Vec3 } | null, radius: number, picked: boolean): void {
    if (this.seam !== null) {
      this.root.remove(this.seam);
      this.seam.geometry.dispose();
      this.seam = null;
    }
    if (seam === null) {
      return;
    }
    const a = scenePoint(seam.from);
    const b = scenePoint(seam.to);
    const length = a.distanceTo(b);
    if (length < 1e-6) {
      return;
    }
    this.seamMaterial.color.setHex(picked ? SEAM_PICKED_COLOR : SEAM_COLOR);
    const bar = new Mesh(new CylinderGeometry(radius, radius, length, 8, 1), this.seamMaterial);
    bar.position.copy(a).add(b).multiplyScalar(0.5);
    bar.quaternion.copy(
      new Quaternion().setFromUnitVectors(new Vector3(0, 1, 0), b.clone().sub(a).normalize()),
    );
    bar.renderOrder = 4;
    this.root.add(bar);
    this.seam = bar;
  }

  /** Whether anything shows. */
  get active(): boolean {
    return this.panels !== null || this.dims !== null || this.seam !== null;
  }

  dispose(): void {
    this.setPanels(null);
    this.setDimensions([]);
    this.setSeam(null, 1, false);
    this.dimMaterial.dispose();
    this.seamMaterial.dispose();
    this.scene.remove(this.root);
  }
}
