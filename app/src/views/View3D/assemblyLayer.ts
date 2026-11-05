// three.js objects for the Assembly View: one mesh per piece, built from
// its part's simulation relief and placed by its pose. The picked piece is
// tinted; while a handle is dragged the other pieces turn pale; while a
// mate waits for a face, the face under the pointer is lit; Verify Fit's
// joint faces show colored by how they fit, the pieces pale meanwhile so
// faces inside a slot show. A component being placed shows as a see
// through ghost that picks never hit. Owned by the renderer, which hides the job's
// material and relief while it shows.

import {
  Box3,
  BufferAttribute,
  BufferGeometry,
  DoubleSide,
  Group,
  Mesh,
  MeshBasicMaterial,
  MeshStandardMaterial,
  type Raycaster,
  type Color,
  type Scene,
} from 'three';

import type { Heightfield } from '../../bridge/cam';
import type { FitMesh } from './fitOverlay';
import { heightfieldMesh } from './heightfieldMesh';
import { MeasureMarks } from './measureMarks';
import { SectionCaps } from './sectionCaps';
import { keptBySection, type ScenePlane } from './sectionPlane';
import { holeWalls } from './holeWalls';
import type { Mat4 } from './pieceGizmo';

/** Most vertices along a side of one piece's surface: pieces are many. */
const PIECE_SIDE = 400;
/** The picked piece's tint. */
const PICKED_COLOUR = 0xd9826b;
/** How much of a ghosted piece shows. */
const GHOST_OPACITY = 0.25;
/** The ghost of a component being placed: its colour and how much of it shows. */
const PLACING_COLOUR = 0x3d8bfd;
const PLACING_OPACITY = 0.45;
/** The lit face's colour and how much of it shows. */
const FACE_COLOUR = 0x3d8bfd;
const FACE_OPACITY = 0.6;
/** How much of a joint face shows: the picked joint's, and every joint's under Show Fit. */
const FIT_STRONG_OPACITY = 0.9;
const FIT_OPACITY = 0.6;

/** A piece as the layer draws it. */
export interface ScenePiece {
  /** The piece's key (`sheet:part`). */
  key: string;
  field: Heightfield;
  /** Relief mesh to scene, column major. */
  matrix: Mat4;
}

interface Built {
  field: Heightfield;
  geometry: BufferGeometry;
  mesh: Mesh<BufferGeometry, MeshStandardMaterial>;
}

/** The relief's top, walls, base and hole walls as one geometry. */
function pieceGeometry(field: Heightfield): BufferGeometry {
  const surface = heightfieldMesh(field, PIECE_SIDE);
  const walls = holeWalls(field, PIECE_SIDE);
  const count = surface.positions.length / 3;
  const positions = new Float32Array(surface.positions.length + walls.positions.length);
  positions.set(surface.positions, 0);
  positions.set(walls.positions, surface.positions.length);
  const indices = new Uint32Array(surface.indices.length + walls.indices.length);
  indices.set(surface.indices, 0);
  for (let i = 0; i < walls.indices.length; i += 1) {
    indices[surface.indices.length + i] = (walls.indices[i] ?? 0) + count;
  }
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new BufferAttribute(positions, 3));
  geometry.setIndex(new BufferAttribute(indices, 1));
  geometry.computeVertexNormals();
  return geometry;
}

export class AssemblyLayer {
  private readonly group = new Group();
  private readonly built = new Map<string, Built>();
  private readonly material: MeshStandardMaterial;
  private readonly picked: MeshStandardMaterial;
  private readonly ghost: MeshStandardMaterial;
  private readonly faceMaterial: MeshBasicMaterial;
  private face: Mesh<BufferGeometry, MeshBasicMaterial> | null = null;
  private readonly placingMaterial: MeshStandardMaterial;
  private placing: { field: Heightfield; mesh: Mesh<BufferGeometry, MeshStandardMaterial> } | null =
    null;
  private readonly fitGroup = new Group();
  private readonly fitMaterials = new Map<string, MeshBasicMaterial>();
  private readonly caps = new SectionCaps(this.group);
  private readonly marks = new MeasureMarks(this.group);
  private section: ScenePlane | null = null;
  private shown = false;
  private selected: string | null = null;
  private ghosting = false;
  /** Joint faces show, so the pieces pale to let them through. */
  private fitShowing = false;

  constructor(
    private readonly scene: Scene,
    colour: Color,
  ) {
    const common = { roughness: 0.8, metalness: 0.02, side: DoubleSide } as const;
    this.material = new MeshStandardMaterial({ ...common, color: colour.clone() });
    this.picked = new MeshStandardMaterial({ ...common, color: PICKED_COLOUR });
    this.ghost = new MeshStandardMaterial({
      ...common,
      color: colour.clone(),
      transparent: true,
      opacity: GHOST_OPACITY,
      depthWrite: false,
    });
    this.placingMaterial = new MeshStandardMaterial({
      ...common,
      color: PLACING_COLOUR,
      transparent: true,
      opacity: PLACING_OPACITY,
      depthWrite: false,
    });
    this.faceMaterial = new MeshBasicMaterial({
      color: FACE_COLOUR,
      transparent: true,
      opacity: FACE_OPACITY,
      side: DoubleSide,
      depthWrite: false,
      polygonOffset: true,
      polygonOffsetFactor: -2,
      polygonOffsetUnits: -2,
    });
    this.group.matrixAutoUpdate = false;
    this.fitGroup.matrixAutoUpdate = false;
    this.group.add(this.fitGroup);
  }

  /** Whether the Assembly View is showing. */
  get active(): boolean {
    return this.shown;
  }

  /** The material colour the pieces take (the job's look). */
  setColour(colour: Color): void {
    this.material.color.copy(colour);
    this.ghost.color.copy(colour);
  }

  /** Shows `pieces`, or takes the layer away when null. */
  set(pieces: readonly ScenePiece[] | null): void {
    if (pieces === null) {
      this.clear();
      return;
    }
    if (!this.shown) {
      this.scene.add(this.group);
      this.shown = true;
    }
    const wanted = new Set(pieces.map((p) => p.key));
    for (const [key, built] of this.built) {
      if (!wanted.has(key)) {
        this.drop(key, built);
      }
    }
    for (const piece of pieces) {
      let built = this.built.get(piece.key);
      if (built !== undefined && built.field !== piece.field) {
        this.drop(piece.key, built);
        built = undefined;
      }
      if (built === undefined) {
        const geometry = pieceGeometry(piece.field);
        const mesh = new Mesh(geometry, this.material);
        mesh.matrixAutoUpdate = false;
        mesh.userData['piece'] = piece.key;
        this.group.add(mesh);
        built = { field: piece.field, geometry, mesh };
        this.built.set(piece.key, built);
      }
      built.mesh.matrix.fromArray(piece.matrix);
      built.mesh.matrixWorldNeedsUpdate = true;
    }
    this.paint();
    if (this.section !== null) {
      this.cut();
    }
  }

  /** Cuts the pieces by a plane (scene space) and caps the cut faces; null for none. */
  setSection(plane: ScenePlane | null): void {
    this.section = plane;
    this.cut();
  }

  private cut(): void {
    const meshes = [...this.built.values()].map((b) => b.mesh);
    this.group.updateMatrixWorld(true);
    const box = new Box3();
    for (const mesh of meshes) {
      box.expandByObject(mesh);
    }
    const centre = box.isEmpty()
      ? ([0, 0, 0] as [number, number, number])
      : ([
          (box.min.x + box.max.x) / 2,
          (box.min.y + box.max.y) / 2,
          (box.min.z + box.max.z) / 2,
        ] as [number, number, number]);
    const size = box.isEmpty() ? 1 : box.max.distanceTo(box.min) * 2;
    this.caps.set(this.section, meshes, centre, size, [
      this.material,
      this.picked,
      this.ghost,
      this.faceMaterial,
      ...this.fitMaterials.values(),
    ]);
  }

  /** Shows a measurement between two scene points with its label; null for none. */
  setMeasure(
    ends: { a: [number, number, number]; b: [number, number, number]; text: string } | null,
  ): void {
    this.marks.set(this.shown ? ends : null);
  }

  /** Tints the picked piece and, while `ghosting`, pales the others. */
  setHighlight(selected: string | null, ghosting: boolean): void {
    this.selected = selected;
    this.ghosting = ghosting;
    this.paint();
  }

  private paint(): void {
    const pale = this.ghosting || this.fitShowing;
    for (const [key, built] of this.built) {
      built.mesh.material =
        key === this.selected && !this.fitShowing ? this.picked : pale ? this.ghost : this.material;
      // Pale pieces draw after the solid ones, so they do not hide them.
      built.mesh.renderOrder = built.mesh.material === this.ghost ? 1 : 0;
    }
  }

  /** Shows the ghost of a component being placed where it would land; null for none. */
  setPlacing(piece: Pick<ScenePiece, 'field' | 'matrix'> | null): void {
    if (this.placing !== null && (piece === null || this.placing.field !== piece.field)) {
      this.group.remove(this.placing.mesh);
      this.placing.mesh.geometry.dispose();
      this.placing = null;
    }
    if (piece === null || !this.shown) {
      return;
    }
    if (this.placing === null) {
      const mesh = new Mesh(pieceGeometry(piece.field), this.placingMaterial);
      mesh.matrixAutoUpdate = false;
      mesh.renderOrder = 2;
      this.group.add(mesh);
      this.placing = { field: piece.field, mesh };
    }
    this.placing.mesh.matrix.fromArray(piece.matrix);
    this.placing.mesh.matrixWorldNeedsUpdate = true;
  }

  /** Lights a face: triangles in a piece's own frame and its matrix to the scene; null for none. */
  setFace(triangles: Float32Array | null, matrix: Mat4 | null): void {
    if (this.face !== null) {
      this.group.remove(this.face);
      this.face.geometry.dispose();
      this.face = null;
    }
    if (triangles === null || matrix === null || triangles.length === 0 || !this.shown) {
      return;
    }
    const geometry = new BufferGeometry();
    geometry.setAttribute('position', new BufferAttribute(triangles, 3));
    const mesh = new Mesh(geometry, this.faceMaterial);
    mesh.matrixAutoUpdate = false;
    mesh.matrix.fromArray(matrix);
    mesh.renderOrder = 2;
    this.face = mesh;
    this.group.add(mesh);
  }

  /**
   * Shows joint faces colored by how they fit; the picked joint's (strong)
   * show through everything. An empty list takes them away.
   */
  setFit(meshes: readonly FitMesh[]): void {
    for (const child of [...this.fitGroup.children]) {
      this.fitGroup.remove(child);
      if (child instanceof Mesh) {
        (child.geometry as BufferGeometry).dispose();
      }
    }
    for (const fit of meshes) {
      const geometry = new BufferGeometry();
      geometry.setAttribute('position', new BufferAttribute(fit.triangles, 3));
      const mesh = new Mesh(geometry, this.fitMaterial(fit.colour, fit.strong));
      mesh.matrixAutoUpdate = false;
      mesh.matrix.fromArray(fit.matrix);
      mesh.renderOrder = fit.strong ? 4 : 3;
      this.fitGroup.add(mesh);
    }
    this.fitShowing = meshes.length > 0;
    this.paint();
  }

  private fitMaterial(colour: number, strong: boolean): MeshBasicMaterial {
    const key = `${colour}:${strong}`;
    let material = this.fitMaterials.get(key);
    if (material === undefined) {
      material = new MeshBasicMaterial({
        color: colour,
        transparent: true,
        opacity: strong ? FIT_STRONG_OPACITY : FIT_OPACITY,
        side: DoubleSide,
        depthWrite: false,
        depthTest: !strong,
        polygonOffset: true,
        polygonOffsetFactor: -3,
        polygonOffsetUnits: -3,
        clippingPlanes: this.caps.planes(),
      });
      this.fitMaterials.set(key, material);
    }
    return material;
  }

  /**
   * The piece under a ray and where the ray meets it (scene space), or
   * null. Material the section cuts away is not there to pick: the ray
   * goes on to what the cut leaves.
   */
  pick(raycaster: Raycaster): { key: string; point: [number, number, number] } | null {
    if (!this.shown) {
      return null;
    }
    this.group.updateMatrixWorld(true);
    const meshes = [...this.built.values()].map((b) => b.mesh);
    for (const hit of raycaster.intersectObjects(meshes, false)) {
      const key: unknown = hit.object.userData['piece'];
      const point: [number, number, number] = [hit.point.x, hit.point.y, hit.point.z];
      if (typeof key === 'string' && keptBySection(this.section, point)) {
        return { key, point };
      }
    }
    return null;
  }

  private drop(key: string, built: Built): void {
    this.group.remove(built.mesh);
    built.geometry.dispose();
    this.built.delete(key);
  }

  private clear(): void {
    this.setFace(null, null);
    this.setPlacing(null);
    this.setFit([]);
    this.marks.set(null);
    if (this.section !== null) {
      this.section = null;
      this.cut();
    }
    for (const [key, built] of this.built) {
      this.drop(key, built);
    }
    if (this.shown) {
      this.scene.remove(this.group);
      this.shown = false;
    }
  }

  dispose(): void {
    this.clear();
    this.material.dispose();
    this.picked.dispose();
    this.ghost.dispose();
    this.faceMaterial.dispose();
    this.placingMaterial.dispose();
    for (const material of this.fitMaterials.values()) {
      material.dispose();
    }
    this.caps.dispose();
    this.marks.dispose();
  }
}
