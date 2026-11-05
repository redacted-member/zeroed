// The three.js objects of the tool the animated preview draws: the cutter,
// shank and (when it is checked) holder as shaded solids turned from the
// tool's profile, and a light outline of them drawn over everything, so the
// tool shows where the material hides it.

import type { BufferGeometry } from 'three';
import {
  EdgesGeometry,
  Group,
  LatheGeometry,
  LineBasicMaterial,
  LineSegments,
  Mesh,
  MeshStandardMaterial,
  Vector2,
  type Scene,
} from 'three';

import { toolShape, type ProfilePoint, type ToolPart } from './toolMarker';

/** What the tool is turned from: its outline, tip first, and the shank and
 * holder the checks use (see toolShape). */
export interface ToolParts {
  outline: ReadonlyArray<readonly [number, number]>;
  shank?: ToolPart | null;
  holder?: ToolPart | null;
}

/** Steps round the axis. */
const AROUND = 40;
/** Edges between faces meeting at more than this, degrees, are outlined. */
const EDGE_ANGLE = 25;

function lathe(points: readonly ProfilePoint[]): LatheGeometry {
  return new LatheGeometry(
    points.map(([r, h]) => new Vector2(r, h)),
    AROUND,
  );
}

export class ToolModel {
  private readonly group = new Group();
  private readonly cutterMaterial = new MeshStandardMaterial({
    color: 0xe8edf2,
    metalness: 0.55,
    roughness: 0.3,
  });
  private readonly shankMaterial = new MeshStandardMaterial({
    color: 0x9aa3ad,
    metalness: 0.6,
    roughness: 0.35,
  });
  private readonly holderMaterial = new MeshStandardMaterial({
    color: 0x5d6670,
    metalness: 0.5,
    roughness: 0.45,
    transparent: true,
    opacity: 0.8,
  });
  /** The outline shows through the material, faint, over everything. */
  private readonly edgeMaterial = new LineBasicMaterial({
    color: 0xffffff,
    depthTest: false,
    transparent: true,
    opacity: 0.55,
  });
  /** The parts the shown geometry was turned from. */
  private parts: ToolParts | null = null;

  constructor(private readonly scene: Scene) {
    this.group.visible = false;
    this.group.renderOrder = 10;
    scene.add(this.group);
  }

  /** The tool turned from `parts` at scene point `at`, or none. */
  set(parts: ToolParts | null, at: readonly [number, number, number] | null): void {
    const same =
      parts === this.parts ||
      (parts !== null &&
        this.parts !== null &&
        parts.outline === this.parts.outline &&
        parts.shank === this.parts.shank &&
        parts.holder === this.parts.holder);
    if (!same) {
      this.rebuild(parts);
    }
    const shown = at !== null && this.group.children.length > 0;
    this.group.visible = shown;
    if (at !== null) {
      this.group.position.set(at[0], at[1], at[2]);
    }
  }

  /** Whether the tool is drawn, and where its tip is in the scene. */
  state(): {
    shown: boolean;
    at: [number, number, number];
    radius: number;
    height: number;
    holder: boolean;
  } {
    const box = this.group.children[0] as Mesh<BufferGeometry> | undefined;
    box?.geometry.computeBoundingBox();
    const bounds = box?.geometry.boundingBox;
    return {
      shown: this.group.visible,
      at: [this.group.position.x, this.group.position.y, this.group.position.z],
      radius: bounds ? bounds.max.x : 0,
      height: bounds ? bounds.max.y : 0,
      holder: this.group.children.length > 4,
    };
  }

  private rebuild(parts: ToolParts | null): void {
    this.clear();
    this.parts = parts;
    const shape = parts === null ? null : toolShape(parts.outline, parts);
    if (shape === null) {
      return;
    }
    const cutter = lathe(shape.cutter);
    const shank = lathe(shape.shank);
    const cutterMesh = new Mesh(cutter, this.cutterMaterial);
    const shankMesh = new Mesh(shank, this.shankMaterial);
    const cutterEdges = new LineSegments(new EdgesGeometry(cutter, EDGE_ANGLE), this.edgeMaterial);
    const shankEdges = new LineSegments(new EdgesGeometry(shank, EDGE_ANGLE), this.edgeMaterial);
    cutterEdges.renderOrder = 11;
    shankEdges.renderOrder = 11;
    this.group.add(cutterMesh, shankMesh, cutterEdges, shankEdges);
    if (shape.holder !== null) {
      const holder = lathe(shape.holder);
      const holderEdges = new LineSegments(
        new EdgesGeometry(holder, EDGE_ANGLE),
        this.edgeMaterial,
      );
      holderEdges.renderOrder = 11;
      this.group.add(new Mesh(holder, this.holderMaterial), holderEdges);
    }
  }

  private clear(): void {
    for (const child of [...this.group.children]) {
      this.group.remove(child);
      if (child instanceof Mesh || child instanceof LineSegments) {
        (child.geometry as BufferGeometry).dispose();
      }
    }
  }

  dispose(): void {
    this.clear();
    this.scene.remove(this.group);
    this.cutterMaterial.dispose();
    this.shankMaterial.dispose();
    this.holderMaterial.dispose();
    this.edgeMaterial.dispose();
  }
}
