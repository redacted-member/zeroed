// The Assembly View's section in three.js: the pieces clipped by one plane
// and the cut faces capped. Each piece's back faces behind the plane count
// up in the stencil buffer and its front faces count down, so where the
// plane passes through material the count is left above zero; a quad on
// the plane draws only there, which caps every cut face at once.

import {
  AlwaysStencilFunc,
  BackSide,
  DecrementWrapStencilOp,
  DoubleSide,
  FrontSide,
  Group,
  IncrementWrapStencilOp,
  Mesh,
  MeshBasicMaterial,
  MeshStandardMaterial,
  NotEqualStencilFunc,
  Plane,
  PlaneGeometry,
  Quaternion,
  ReplaceStencilOp,
  Vector3,
  type BufferGeometry,
  type Material,
} from 'three';

import type { ScenePlane } from './sectionPlane';

/** The cap's colour: a darker cut face than the material's surface. */
const CAP_COLOUR = 0x8a5a2b;
/** Stencil passes draw after the solid pieces, the cap after them. */
const STENCIL_ORDER = 5;
const CAP_ORDER = 6;

function stencilMaterial(
  side: typeof FrontSide | typeof BackSide,
  plane: Plane,
): MeshBasicMaterial {
  const op = side === BackSide ? IncrementWrapStencilOp : DecrementWrapStencilOp;
  return new MeshBasicMaterial({
    side,
    clippingPlanes: [plane],
    colorWrite: false,
    depthWrite: false,
    depthTest: false,
    stencilWrite: true,
    stencilFunc: AlwaysStencilFunc,
    stencilFail: op,
    stencilZFail: op,
    stencilZPass: op,
  });
}

export class SectionCaps {
  private readonly group = new Group();
  private readonly plane = new Plane();
  private readonly back = stencilMaterial(BackSide, this.plane);
  private readonly front = stencilMaterial(FrontSide, this.plane);
  private readonly capMaterial = new MeshStandardMaterial({
    color: CAP_COLOUR,
    roughness: 0.9,
    metalness: 0,
    side: DoubleSide,
    stencilWrite: true,
    stencilRef: 0,
    stencilFunc: NotEqualStencilFunc,
    stencilFail: ReplaceStencilOp,
    stencilZFail: ReplaceStencilOp,
    stencilZPass: ReplaceStencilOp,
  });
  private readonly cap = new Mesh(new PlaneGeometry(1, 1), this.capMaterial);
  private on = false;

  constructor(parent: Group) {
    this.group.matrixAutoUpdate = false;
    this.cap.renderOrder = CAP_ORDER;
    parent.add(this.group);
  }

  /** Whether a section is showing. */
  get active(): boolean {
    return this.on;
  }

  /** The clipping planes the pieces' materials take: the section's, or none. */
  planes(): Plane[] {
    return this.on ? [this.plane] : [];
  }

  /**
   * Cuts `meshes` (the pieces) by `plane`, capped with a quad `size` across
   * round `centre` (scene space), or takes the section away when null.
   * Clips `materials` too.
   */
  set(
    plane: ScenePlane | null,
    meshes: readonly Mesh<BufferGeometry, Material>[],
    centre: [number, number, number],
    size: number,
    materials: readonly Material[],
  ): void {
    this.on = plane !== null;
    for (const child of [...this.group.children]) {
      this.group.remove(child);
    }
    for (const material of materials) {
      material.clippingPlanes = this.planes();
      material.needsUpdate = true;
    }
    if (plane === null) {
      return;
    }
    this.plane.normal.set(plane.normal[0], plane.normal[1], plane.normal[2]);
    this.plane.constant = plane.constant;
    for (const mesh of meshes) {
      for (const material of [this.back, this.front]) {
        const pass = new Mesh(mesh.geometry, material);
        pass.matrixAutoUpdate = false;
        pass.matrix.copy(mesh.matrix);
        pass.renderOrder = STENCIL_ORDER;
        this.group.add(pass);
      }
    }
    // The cap lies on the plane, facing along its normal, over the pieces.
    const normal = new Vector3(plane.normal[0], plane.normal[1], plane.normal[2]).normalize();
    const at = new Vector3(centre[0], centre[1], centre[2]);
    at.addScaledVector(normal, -(normal.dot(at) + plane.constant));
    this.cap.position.copy(at);
    this.cap.quaternion.copy(new Quaternion().setFromUnitVectors(new Vector3(0, 0, 1), normal));
    this.cap.scale.set(size, size, 1);
    this.cap.updateMatrix();
    this.cap.matrixAutoUpdate = false;
    this.group.add(this.cap);
  }

  dispose(): void {
    this.back.dispose();
    this.front.dispose();
    this.capMaterial.dispose();
    this.cap.geometry.dispose();
  }
}
