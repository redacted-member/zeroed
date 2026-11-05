// Extra things the 3D view draws on request from the View and Toolpaths
// menus: plunge, retract and join moves in their own colours, arrows at the
// job origin, the see through modeling plane, and the lighting modes (light
// follows the viewer, shadow shading). The renderer owns one and calls it
// before each frame.

import {
  ArrowHelper,
  BufferAttribute,
  BufferGeometry,
  Color,
  DoubleSide,
  LineBasicMaterial,
  LineSegments,
  Mesh,
  MeshBasicMaterial,
  PlaneGeometry,
  Vector3,
  type Camera,
  type DirectionalLight,
  type HemisphereLight,
  type Scene,
} from 'three';

import { SHADOW_DEPTH } from '../../lib/shadowDepth';
import type { ToolpathLines } from './overlayLines';

/** The lights the renderer set up, which the lighting modes adjust. */
export interface SceneLights {
  sky: HemisphereLight;
  sun: DirectionalLight;
  fill: DirectionalLight;
}

/** Light strengths for normal shading and for shadow shading. */
export const LIGHTING = {
  normal: { sky: 2.1, sun: 1.9, fill: 0.7 },
  shadow: { sky: 0.8, sun: 3.0, fill: 0.2 },
} as const;

/** The deepest shadow shading Ctrl + NumPad+ reaches. */
const DEEPEST = { sky: 0.3, sun: 3.4, fill: 0.05 } as const;

/** Light strengths for shadow shading at `depth`. */
export function shadowLighting(depth: number): { sky: number; sun: number; fill: number } {
  const [from, to, t] =
    depth <= 0.5
      ? [LIGHTING.normal, LIGHTING.shadow, depth / 0.5]
      : [LIGHTING.shadow, DEEPEST, (depth - 0.5) / 0.5];
  const mix = (a: number, b: number) => a + (b - a) * t;
  return { sky: mix(from.sky, to.sky), sun: mix(from.sun, to.sun), fill: mix(from.fill, to.fill) };
}

/** Where the key light sits relative to its target: up and to the top left. */
const SUN_HOME = new Vector3(-0.4, 1, 0.6);
/** A light that follows the viewer sits a little above the camera. */
const FOLLOW_LIFT = 0.25;

const ORIGIN_X = 0xd62828;
const ORIGIN_Y = 0x2e9e44;
const ORIGIN_Z = 0x1f6fd6;
const PLANE_COLOUR = 0x5b8def;
const PLANE_OPACITY = 0.22;

/** Colours of the move groups the renderer does not draw itself. */
export interface MoveColours {
  plunge: string;
  retract: string;
  join: string;
}

/** What the markers need to know about the job, in scene units. */
export interface MarkerPlacement {
  /** Scene height of job Z zero. */
  originY: number;
  /** Scene height of the model's zero plane. */
  planeY: number;
  /** Job XY box: minX, minY, maxX, maxY. */
  bounds: readonly [number, number, number, number];
}

export interface ExtrasOptions {
  drawOrigin: boolean;
  drawModelingPlane: boolean;
  lightFollows: boolean;
  shadowShading: boolean;
  /** How deep the shadow shading is; the standard depth when absent. */
  shadowDepth?: number;
}

/** Direction of the key light for a camera at `camera` looking at `target`. */
export function followDirection(camera: Vector3, target: Vector3): Vector3 {
  const direction = camera.clone().sub(target);
  if (direction.lengthSq() < 1e-12) {
    return SUN_HOME.clone();
  }
  direction.normalize();
  direction.y += FOLLOW_LIFT;
  return direction.normalize();
}

export class SceneExtras {
  private readonly scene: Scene;
  private readonly lights: SceneLights;
  private readonly materials = {
    plunge: new LineBasicMaterial({ color: 0x00a8c6 }),
    retract: new LineBasicMaterial({ color: 0x2e9e44 }),
    join: new LineBasicMaterial({ color: 0x8a3ffc }),
  };
  private readonly lines: Record<keyof MoveColours, LineSegments | null> = {
    plunge: null,
    retract: null,
    join: null,
  };
  private readonly planeMaterial = new MeshBasicMaterial({
    color: PLANE_COLOUR,
    transparent: true,
    opacity: PLANE_OPACITY,
    side: DoubleSide,
    depthWrite: false,
  });
  private plane: Mesh<PlaneGeometry, MeshBasicMaterial> | null = null;
  private arrows: ArrowHelper[] = [];
  private options: ExtrasOptions = {
    drawOrigin: false,
    drawModelingPlane: false,
    lightFollows: false,
    shadowShading: false,
  };
  private toolpathsVisible = true;
  /** The job itself shows (not the Assembly View's pieces). */
  private jobShown = true;
  /** A toolpath preview shows the cut material in place of the model. */
  private previewShown = false;

  constructor(scene: Scene, lights: SceneLights) {
    this.scene = scene;
    this.lights = lights;
  }

  setOptions(options: ExtrasOptions): void {
    this.options = { ...options };
    const level = options.shadowShading
      ? shadowLighting(options.shadowDepth ?? SHADOW_DEPTH.initial)
      : LIGHTING.normal;
    this.lights.sky.intensity = level.sky;
    this.lights.sun.intensity = level.sun;
    this.lights.fill.intensity = level.fill;
    if (!options.lightFollows) {
      this.lights.sun.position.copy(SUN_HOME);
    }
    this.updateVisibility();
  }

  setColours(colours: MoveColours): void {
    this.materials.plunge.color = new Color(colours.plunge);
    this.materials.retract.color = new Color(colours.retract);
    this.materials.join.color = new Color(colours.join);
  }

  /** Plunge, retract and join segments to draw, or null for none. */
  setMoveLines(lines: ToolpathLines | null): void {
    for (const group of ['plunge', 'retract', 'join'] as const) {
      const old = this.lines[group];
      if (old !== null) {
        this.scene.remove(old);
        old.geometry.dispose();
        this.lines[group] = null;
      }
      const positions = lines?.[group];
      if (positions === undefined || positions.length === 0) {
        continue;
      }
      const geometry = new BufferGeometry();
      geometry.setAttribute('position', new BufferAttribute(positions, 3));
      const segments = new LineSegments(geometry, this.materials[group]);
      this.scene.add(segments);
      this.lines[group] = segments;
    }
    this.updateVisibility();
  }

  /** Whether toolpaths show at all (the 3D view's own toggle). */
  setToolpathsVisible(visible: boolean): void {
    this.toolpathsVisible = visible;
    this.updateVisibility();
  }

  /**
   * Whether the job shows: the modeling plane belongs to the job's
   * material, so it hides while the Assembly View shows pieces instead.
   */
  setJobShown(shown: boolean): void {
    this.jobShown = shown;
    this.updateVisibility();
  }

  /**
   * Whether a toolpath preview shows: the modeling plane would lie over
   * the cut material and tint it, so it hides while the preview is up.
   */
  setPreviewShown(shown: boolean): void {
    this.previewShown = shown;
    this.updateVisibility();
  }

  /** Places the origin arrows and the modeling plane for a job. */
  setPlacement(placement: MarkerPlacement): void {
    this.removeMarkers();
    const [minX, minY, maxX, maxY] = placement.bounds;
    const width = Math.max(maxX - minX, 1e-3);
    const height = Math.max(maxY - minY, 1e-3);
    const size = Math.max(width, height) * 0.15;
    const at = new Vector3(0, placement.originY, 0);
    const head = size * 0.18;
    this.arrows = [
      new ArrowHelper(new Vector3(1, 0, 0), at, size, ORIGIN_X, head, head * 0.6),
      new ArrowHelper(new Vector3(0, 0, -1), at, size, ORIGIN_Y, head, head * 0.6),
      new ArrowHelper(new Vector3(0, 1, 0), at, size, ORIGIN_Z, head, head * 0.6),
    ];
    for (const arrow of this.arrows) {
      arrow.renderOrder = 2;
      this.scene.add(arrow);
    }
    const geometry = new PlaneGeometry(width, height);
    geometry.rotateX(-Math.PI / 2);
    this.plane = new Mesh(geometry, this.planeMaterial);
    this.plane.position.set((minX + maxX) / 2, placement.planeY, -(minY + maxY) / 2);
    this.plane.renderOrder = 1;
    this.scene.add(this.plane);
    this.updateVisibility();
  }

  /** Called before each frame: points the key light along the view when it follows. */
  beforeDraw(camera: Camera, target: Vector3): void {
    if (this.options.lightFollows) {
      this.lights.sun.position.copy(followDirection(camera.position, target));
    }
  }

  private updateVisibility(): void {
    for (const segments of Object.values(this.lines)) {
      if (segments !== null) {
        segments.visible = this.toolpathsVisible;
      }
    }
    for (const arrow of this.arrows) {
      arrow.visible = this.options.drawOrigin;
    }
    if (this.plane !== null) {
      this.plane.visible = this.options.drawModelingPlane && this.jobShown && !this.previewShown;
    }
  }

  private removeMarkers(): void {
    for (const arrow of this.arrows) {
      this.scene.remove(arrow);
      arrow.dispose();
    }
    this.arrows = [];
    if (this.plane !== null) {
      this.scene.remove(this.plane);
      this.plane.geometry.dispose();
      this.plane = null;
    }
  }

  dispose(): void {
    this.setMoveLines(null);
    this.removeMarkers();
    this.planeMaterial.dispose();
    for (const material of Object.values(this.materials)) {
      material.dispose();
    }
  }
}
