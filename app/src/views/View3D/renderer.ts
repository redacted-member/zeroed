// three.js renderer for the model view. It draws the material block, the bed
// grid, the lights and, on request, toolpaths and vectors over the model.
// Job space maps to the scene as x -> x, y -> -z, z -> y, so that the
// material top sits at scene y zero. The camera maths lives in camera3d.ts.

import {
  BoxGeometry,
  BufferAttribute,
  BufferGeometry,
  Float32BufferAttribute,
  Color,
  DoubleSide,
  DirectionalLight,
  EdgesGeometry,
  GridHelper,
  HemisphereLight,
  LineBasicMaterial,
  LineLoop,
  LineSegments,
  Mesh,
  MeshBasicMaterial,
  MeshStandardMaterial,
  OrthographicCamera,
  PlaneGeometry,
  PerspectiveCamera,
  Raycaster,
  RingGeometry,
  Scene,
  Vector2,
  Vector3,
  WebGLRenderer,
  type Camera,
  type Texture,
} from 'three';

import type { Heightfield } from '../../bridge/cam';
import type { RenderMode } from '../../bridge/prefs';
import type { JobSetup } from '../../bridge/types';
import type { ScenePalette } from '../../lib/theme';
import type { PatchedCells } from '../../state/previewStore';
import { isRotary, rotaryOf } from '../../lib/rotary';
import { jobBounds } from '../View2D/camera';
import type { PaletteColours } from '../../bridge/componentTree';
import type { MaterialLook } from './appearance';
import { vertexColours, type LinearRgb } from './componentColours';
import {
  FINISH_ATTRIBUTE,
  finishUniforms,
  patchFinishMaterial,
  vertexFinish,
} from './componentFinish';
import { gpuFits, GpuSurface, rectUpload, type GpuLimits, type SurfacePaint } from './gpuSurface';
import { ownerColours } from './ownerPalette';
import {
  cameraBasis,
  cameraPosition,
  clampPhi,
  fitView,
  FOV_DEG,
  ISO_PHI,
  ISO_THETA,
  orthoHalfSize,
  sceneBoxOf,
  type Box3,
  type OrbitAngles,
  type OrbitView,
  type ProjectionKind,
} from './camera3d';
import { heightfieldMesh } from './heightfieldMesh';
import { contourHoleWalls, holeWalls } from './holeWalls';
import { AssemblyLayer, type ScenePiece } from './assemblyLayer';
import { FurnitureLayer } from './furnitureLayer';
import type { DimensionLine } from './furnitureSeams';
import type { PreviewPanel, Vec3 } from '../../bridge/furniture';
import type { FitMesh } from './fitOverlay';
import type { ScenePlane } from './sectionPlane';
import type { Mat4 } from './pieceGizmo';
import { ImportLayer } from './importLayer';
import type { ImportScene } from './importPreview';
import type { ToolpathLines } from './overlayLines';
import { pickSurface } from './pickSurface';
import { SceneExtras } from './sceneExtras';
import { ToolModel, type ToolParts } from './toolModel';
import { IssueMarkers, type IssueMarker } from './issueMarkers';
import { JOB_ATTRIBUTE, flatJobCoords, jobUniforms, patchJobMaterial } from './jobShader';
import {
  MARK_ATTRIBUTE,
  markAttribute,
  markedColours,
  vertexBurn,
  vertexCodes,
  type VertexGrid,
} from './checkMarks';
import { setSliceTint, sliceTintUniforms } from './sliceTint';
import { wrappedHeightfield, wrappedStock, type WrappedMesh } from './wrappedMesh';

/** A point or direction as three numbers. */
type Vec3Tuple = readonly [number, number, number];

const MIN_RADIUS = 5;
const MAX_RADIUS = 40000;
const ORBIT_PER_PIXEL = 0.007;
const BRUSH_COLOUR = 0xff6a3d;
/** Least time between rebuilds of the hole walls while a preview animates, ms. */
const HOLE_WALL_GAP_MS = 250;
/** Strength of the rim light. */
const RIM_INTENSITY = 1.1;
/** How far above the horizon the rim light sits, as a fraction of its direction. */
const RIM_LIFT = 0.45;

/**
 * Where the rim light shines from for a camera at `camera` looking at
 * `target`: behind the model, opposite the camera, a little above it.
 */
export function rimDirection(camera: Vector3, target: Vector3): Vector3 {
  const away = camera.clone().sub(target);
  away.y = 0;
  if (away.lengthSq() < 1e-12) {
    return new Vector3(0, 1, 0);
  }
  away.normalize();
  return new Vector3(-away.x, RIM_LIFT, -away.z).normalize();
}
/** Width of the brush outline as a fraction of its radius. */
const BRUSH_RING = 0.08;
const WIRE_COLOUR = 0x1b1d21;
const WIRE_OPACITY = 0.15;
const EDGE_COLOUR = 0x2a2d33;
const CUTTING_COLOUR = 0x2f7df6;

/** A flat arrow from the middle to the rim of a brush of `radius`,
 * pointing along scene +X. */
function arrowGeometry(radius: number): BufferGeometry {
  const w = radius * 0.06;
  const head = radius * 0.25;
  const tip = radius * 0.95;
  const neck = tip - head;
  // Shaft (two triangles) and head, in the XZ plane.
  const points = [
    [0, -w],
    [neck, -w],
    [neck, w],
    [0, -w],
    [neck, w],
    [0, w],
    [neck, -head * 0.6],
    [tip, 0],
    [neck, head * 0.6],
  ];
  const geometry = new BufferGeometry();
  geometry.setAttribute(
    'position',
    new Float32BufferAttribute(
      points.flatMap(([x, z]) => [x ?? 0, 0, z ?? 0]),
      3,
    ),
  );
  return geometry;
}

/** A point on the shown surface: in job space, and where it is in the scene. */
export interface SurfacePick {
  x: number;
  y: number;
  scene: Vector3;
}

/** What the view shows, from the preferences. */
export interface SceneVisibility {
  model: boolean;
  block: boolean;
  toolpaths: boolean;
  vectors: boolean;
}

/** Colours of the overlay lines. */
export interface LineColours {
  cut: string;
  rapid: string;
  vector: string;
}

/** Geometry for a wrapped rotary mesh. */
function wrappedGeometry(mesh: WrappedMesh): BufferGeometry {
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new BufferAttribute(mesh.positions, 3));
  geometry.setAttribute(JOB_ATTRIBUTE, new BufferAttribute(mesh.job, 3));
  geometry.setIndex(new BufferAttribute(mesh.indices, 1));
  geometry.computeVertexNormals();
  return geometry;
}

function lineGeometry(positions: Float32Array): BufferGeometry {
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new BufferAttribute(positions, 3));
  return geometry;
}

/** Draws the model view. No React inside the draw path. */
export class View3DRenderer {
  private readonly renderer: WebGLRenderer;
  private readonly scene = new Scene();
  private readonly perspective = new PerspectiveCamera(FOV_DEG, 1, 1, MAX_RADIUS * 4);
  private readonly ortho = new OrthographicCamera(-1, 1, 1, -1, -MAX_RADIUS * 4, MAX_RADIUS * 4);
  private projection: ProjectionKind = 'perspective';
  private readonly material = new MeshStandardMaterial({
    roughness: 0.85,
    metalness: 0.02,
    polygonOffset: true,
    polygonOffsetFactor: 1,
    polygonOffsetUnits: 1,
  });
  private readonly cutMaterial = new MeshStandardMaterial({
    roughness: 0.8,
    metalness: 0.02,
    side: DoubleSide,
    polygonOffset: true,
    polygonOffsetFactor: 1,
    polygonOffsetUnits: 1,
  });
  /** The model surface when components have colours: vertex colours on white. */
  private readonly paintedMaterial = new MeshStandardMaterial({
    color: 0xffffff,
    vertexColors: true,
    roughness: 0.8,
    metalness: 0.02,
    side: DoubleSide,
    polygonOffset: true,
    polygonOffsetFactor: 1,
    polygonOffsetUnits: 1,
  });
  /** Component or machined colours drawn on the model surface. */
  private surfacePaint: SurfacePaint = { kind: 'none' };
  /** The surface drawn on the GPU from height textures, when it can be. */
  private readonly gpu: GpuSurface;
  /** Texture and lithophane of the plain meshes (block, CPU and rotary surfaces). */
  private readonly jobLook = jobUniforms();
  /** The Slice Model form's picked slice, on every model surface. */
  private readonly sliceTint = sliceTintUniforms();
  private readonly gpuLimits: GpuLimits;
  /** The shown surface is the GPU one rather than a CPU built mesh. */
  private cutOnGpu = false;
  /** The top surface samples of the model mesh, for recolouring it. */
  private surface: { xs: number[]; ys: number[]; vertices: number } | null = null;
  /** The vertex grid of the CPU built surface (flat or wrapped), for the
   * preview check's colors. */
  private vertexGrid: VertexGrid | null = null;
  /** The preview check's code per field cell, or null. */
  private checkCodes: Uint8Array | null = null;
  /** The material colour in linear RGB, for uncoloured parts of the model. */
  private lookColour: LinearRgb = [1, 1, 1];
  private readonly wireMaterial = new MeshBasicMaterial({
    color: WIRE_COLOUR,
    wireframe: true,
    transparent: true,
    opacity: WIRE_OPACITY,
  });
  private readonly edgeMaterial = new LineBasicMaterial({ color: EDGE_COLOUR });
  private readonly cutLineMaterial = new LineBasicMaterial({ color: 0x1d4ed8 });
  private readonly rapidLineMaterial = new LineBasicMaterial({ color: 0xd62828 });
  private readonly vectorMaterial = new LineBasicMaterial({ color: 0x1c1c1c });
  /** The tool drawn by an animated preview. */
  private readonly tool = new ToolModel(this.scene);
  private readonly issues = new IssueMarkers(this.scene);
  private block: Mesh<BufferGeometry, MeshStandardMaterial> | null = null;
  private blockWire: Mesh<BufferGeometry, MeshBasicMaterial> | null = null;
  /** Outline of the flat material block, shown around the model. */
  private blockEdges: LineSegments<EdgesGeometry, LineBasicMaterial> | null = null;
  /** The job last shown; a rotary job is drawn wrapped round its axis. */
  private job: JobSetup | null = null;
  private cut: Mesh<BufferGeometry, MeshStandardMaterial> | null = null;
  private cutWire: Mesh<BufferGeometry, MeshBasicMaterial> | null = null;
  private cutLines: LineSegments<BufferGeometry, LineBasicMaterial> | null = null;
  private rapidLines: LineSegments<BufferGeometry, LineBasicMaterial> | null = null;
  private vectorLines: LineSegments<BufferGeometry, LineBasicMaterial> | null = null;
  private grid: GridHelper | null = null;
  private field: Heightfield | null = null;
  private readonly raycaster = new Raycaster();
  private readonly importLayer = new ImportLayer(this.scene);
  private readonly assemblyLayer = new AssemblyLayer(this.scene, this.cutMaterial.color);
  /** The box round the Assembly View's pieces while it shows, scene space. */
  private assemblyBox: Box3 | null = null;
  private readonly furnitureLayer = new FurnitureLayer(this.scene);
  /** The box round the Furniture Designer's preview while it shows, scene space. */
  private furnitureBox: Box3 | null = null;
  /** Walls round the cells cut through, over either surface path. */
  private holeWalls: Mesh<BufferGeometry, MeshStandardMaterial> | null = null;
  private holeWallsAt = 0;
  private brush: Mesh<RingGeometry, MeshBasicMaterial> | null = null;
  private brushRadius = 0;
  /** The custom brush's direction, an arrow lying in the brush ring. */
  private brushArrow: Mesh<BufferGeometry, MeshBasicMaterial> | null = null;
  private outline: LineLoop<BufferGeometry, LineBasicMaterial> | null = null;
  /** The Replace Below plane. */
  private cuttingPlane: Mesh<PlaneGeometry, MeshBasicMaterial> | null = null;
  private readonly cuttingMaterial = new MeshBasicMaterial({
    color: CUTTING_COLOUR,
    side: DoubleSide,
    transparent: true,
    opacity: 0.35,
    depthWrite: false,
  });
  private readonly outlineMaterial = new LineBasicMaterial({
    color: BRUSH_COLOUR,
    depthTest: false,
    transparent: true,
  });
  private readonly brushMaterial = new MeshBasicMaterial({
    color: BRUSH_COLOUR,
    side: DoubleSide,
    depthTest: false,
    transparent: true,
  });
  private gridColour = '#5c636c';
  private renderMode: RenderMode = 'shaded';
  private visibility: SceneVisibility = {
    model: true,
    block: true,
    toolpaths: true,
    vectors: false,
  };
  /** Scene box of the job, what zoom to fit frames. */
  private sceneBox: Box3 = { min: [0, -1, -1], max: [1, 0, 0] };
  /** Where the grid sits: under the job, fixed while the camera moves. */
  private home = new Vector3();
  private readonly target = new Vector3();
  private theta = ISO_THETA;
  private phi = ISO_PHI;
  private radius = 900;
  private aspect = 1;
  private viewportWidth = 1;
  private viewportHeight = 1;
  /** Origin arrows, modeling plane, extra move groups and lighting modes. */
  readonly extras: SceneExtras;
  /** The rim light, placed behind the model from the camera's side each frame. */
  private readonly rim = new DirectionalLight(0xfff3e4, RIM_INTENSITY);

  constructor(canvas: HTMLCanvasElement) {
    // The stencil buffer caps the Assembly View's section.
    this.renderer = new WebGLRenderer({ canvas, antialias: true, alpha: true, stencil: true });
    this.renderer.localClippingEnabled = true;
    this.renderer.setPixelRatio(window.devicePixelRatio);
    const capabilities = this.renderer.capabilities;
    this.gpuLimits = {
      maxTextureSize: capabilities.maxTextureSize,
      vertexTextures: capabilities.maxVertexTextures,
    };
    this.gpu = new GpuSurface(
      this.cutMaterial,
      this.wireMaterial,
      rectUpload(this.renderer),
      this.sliceTint,
    );
    // After the GPU surface took its own copy of the cut material.
    patchJobMaterial(this.material, this.jobLook, this.sliceTint);
    patchJobMaterial(this.cutMaterial, this.jobLook, this.sliceTint);
    patchFinishMaterial(this.paintedMaterial, finishUniforms(), this.sliceTint);

    // A warm key, a cool fill and a soft rim from behind the model that
    // follows the camera, so wood reads as a warm material with an edge
    // instead of a flat tint.
    const sky = new HemisphereLight(0xfff8ee, 0x50525a, 2.1);
    const sun = new DirectionalLight(0xfff1dc, 1.9);
    sun.position.set(-0.4, 1, 0.6);
    const fill = new DirectionalLight(0xdfe9ff, 0.7);
    fill.position.set(0.8, 0.5, -0.7);
    this.scene.add(sky, sun, fill, this.rim);
    this.extras = new SceneExtras(this.scene, { sky, sun, fill });
  }

  /** Faceted shading instead of smooth normals. */
  setFlatShading(flat: boolean): void {
    for (const material of this.surfaceMaterials()) {
      if (material.flatShading !== flat) {
        material.flatShading = flat;
        material.needsUpdate = true;
      }
    }
  }

  /** The materials that follow the look, shading and render mode. */
  private surfaceMaterials(): MeshStandardMaterial[] {
    return [this.material, this.cutMaterial, this.paintedMaterial, this.gpu.material];
  }

  setPalette(palette: ScenePalette): void {
    this.gridColour = palette.grid;
    if (this.block !== null) {
      this.rebuildGrid(this.gridSpan());
    }
  }

  /** Colour and finish of the material and model. */
  setLook(look: MaterialLook): void {
    for (const material of this.surfaceMaterials()) {
      // The painted material stays white under its vertex colours, which
      // already hold the material colour where no component colour is.
      if (material !== this.paintedMaterial) {
        material.color = new Color(look.colour);
      }
      material.roughness = look.roughness;
      material.metalness = look.metalness;
    }
    this.assemblyLayer.setColour(new Color(look.colour));
    const linear = new Color(look.colour);
    this.lookColour = [linear.r, linear.g, linear.b];
    this.paint();
  }

  /** The grain of the job's material (a finish slot), 0 for a plain colour. */
  setJobFinish(slot: number): void {
    this.gpu.setJobFinish(slot);
    this.jobLook.carveJobFinish.value = slot;
  }

  /**
   * Lithophane shading of the previewed material at `strength` from 0 to
   * 1, or null for none. Drawn on the GPU surface only.
   */
  setLithophane(strength: number | null): void {
    this.gpu.setLithophane(strength);
    this.jobLook.carveJobLitho.value = strength === null ? -1 : Math.min(Math.max(strength, 0), 1);
  }

  /** A material texture picture over the material, or null. */
  setMaterialTexture(texture: Texture | null): void {
    this.gpu.setMaterialTexture(texture);
    this.jobLook.carveJobMap.value = texture;
    this.jobLook.carveJobMapOn.value = texture !== null;
  }

  /** Component or machined colours to draw on the model. */
  setPaint(paint: SurfacePaint): void {
    this.surfacePaint = paint;
    this.paint();
  }

  /** The colours per model cell the CPU mesh paints, or null for none. */
  private cellColours(): PaletteColours | null {
    const paint = this.surfacePaint;
    if (paint.kind === 'cells') {
      return paint.colours;
    }
    if (paint.kind === 'owners' && this.field !== null) {
      return ownerColours(this.field, paint.palette);
    }
    return null;
  }

  /** Puts the colours on the model surface, or takes them off. */
  private paint(): void {
    const cut = this.cut;
    if (cut === null) {
      return;
    }
    if (this.cutOnGpu) {
      this.gpu.setPaint(this.surfacePaint);
      return;
    }
    const field = this.field;
    const surface = this.surface;
    const cells = field === null || surface === null ? null : this.cellColours();
    const colours =
      field === null || surface === null || cells === null
        ? null
        : vertexColours(field, cells, surface.xs, surface.ys, surface.vertices, this.lookColour);
    const grid = this.vertexGrid;
    const codes = this.checkCodes;
    const marks =
      field === null || grid === null || codes === null
        ? null
        : vertexCodes(codes, field.width, field.height, grid);
    // Where a laser burnt, the material chars as on the GPU surface.
    const burn = field?.burn;
    const chars =
      field === null || grid === null || burn === undefined
        ? null
        : vertexBurn(burn, field.width, field.height, grid);
    if (colours === null || field === null || surface === null || cells === null) {
      cut.geometry.deleteAttribute('color');
      cut.geometry.deleteAttribute(FINISH_ATTRIBUTE);
      // The check's colors and the char over the material, its texture and grain.
      if (marks === null && chars === null) {
        cut.geometry.deleteAttribute(MARK_ATTRIBUTE);
      } else {
        const count = cut.geometry.getAttribute('position').count;
        cut.geometry.setAttribute(
          MARK_ATTRIBUTE,
          new BufferAttribute(markAttribute(marks, count, chars), 4),
        );
      }
      cut.material = this.cutMaterial;
      return;
    }
    cut.geometry.deleteAttribute(MARK_ATTRIBUTE);
    cut.geometry.setAttribute(
      'color',
      new BufferAttribute(
        marks === null && chars === null ? colours : markedColours(colours, marks, chars),
        3,
      ),
    );
    // Components with a preview material show its grain and finish.
    const finish = vertexFinish(field, cells, surface.xs, surface.ys, surface.vertices);
    if (finish === null) {
      cut.geometry.deleteAttribute(FINISH_ATTRIBUTE);
    } else {
      cut.geometry.setAttribute(FINISH_ATTRIBUTE, new BufferAttribute(finish, 3));
    }
    cut.material = this.paintedMaterial;
  }

  setLineColours(colours: LineColours): void {
    this.cutLineMaterial.color = new Color(colours.cut);
    this.rapidLineMaterial.color = new Color(colours.rapid);
    this.vectorMaterial.color = new Color(colours.vector);
  }

  /** Filled surfaces, their triangle edges, or both. */
  setRenderMode(mode: RenderMode): void {
    this.renderMode = mode;
    for (const material of this.surfaceMaterials()) {
      const wire = mode === 'wireframe';
      if (material.wireframe !== wire) {
        material.wireframe = wire;
        material.needsUpdate = true;
      }
    }
    this.updateVisibility();
  }

  setVisibility(visibility: SceneVisibility): void {
    this.visibility = { ...visibility };
    this.updateVisibility();
  }

  setProjection(projection: ProjectionKind): void {
    this.projection = projection;
  }

  /** Rebuilds the material block for a job and frames it. */
  setJob(job: JobSetup): void {
    const bounds = jobBounds(job);
    const centreX = (bounds.minX + bounds.maxX) / 2;
    const centreY = (bounds.minY + bounds.maxY) / 2;

    this.removeBlock();
    this.job = job;
    this.jobLook.carveJobSize.value.set(job.width, job.height);
    if (isRotary(job)) {
      this.setRotaryJob(job, bounds);
      return;
    }
    const geometry = new BoxGeometry(job.width, job.thickness, job.height);
    geometry.setAttribute(
      JOB_ATTRIBUTE,
      new BufferAttribute(
        flatJobCoords(geometry.getAttribute('position').array, bounds, 0, -job.thickness, [
          centreX,
          -job.thickness / 2,
          -centreY,
        ]),
        3,
      ),
    );
    const block = new Mesh(geometry, this.material);
    block.position.set(centreX, -job.thickness / 2, -centreY);
    this.scene.add(block);
    this.block = block;
    this.blockWire = new Mesh(geometry, this.wireMaterial);
    this.blockWire.position.copy(block.position);
    this.scene.add(this.blockWire);
    this.blockEdges = new LineSegments(new EdgesGeometry(geometry), this.edgeMaterial);
    this.blockEdges.position.copy(block.position);
    this.scene.add(this.blockEdges);

    this.sceneBox = {
      min: [bounds.minX, -job.thickness, -bounds.maxY],
      max: [bounds.maxX, 0, -bounds.minY],
    };
    this.home.set(centreX, -job.thickness / 2, -centreY);
    this.target.copy(this.home);
    this.radius = Math.max(job.width, job.height, job.thickness) * 2.2;
    this.rebuildGrid(Math.max(job.width, job.height) * 2);
    this.updateVisibility();
  }

  /**
   * A rotary blank: the unwrapped job wrapped round its axis, which runs
   * along scene x or scene z at one radius below the scene origin.
   */
  private setRotaryJob(job: JobSetup, bounds: ReturnType<typeof jobBounds>): void {
    const rotary = rotaryOf(job);
    const geometry = wrappedGeometry(wrappedStock(job, bounds));
    const block = new Mesh(geometry, this.cutMaterial);
    this.scene.add(block);
    this.block = block;
    this.blockWire = new Mesh(geometry, this.wireMaterial);
    this.scene.add(this.blockWire);
    const radius = rotary.diameter / 2;
    // A square blank reaches its corners, further out than the radius.
    const reach = rotary.blank === 'square' ? radius * Math.SQRT2 : radius;
    if (rotary.axis === 'x') {
      this.home.set((bounds.minX + bounds.maxX) / 2, -radius, 0);
      this.sceneBox = {
        min: [bounds.minX, -radius - reach, -reach],
        max: [bounds.maxX, -radius + reach, reach],
      };
    } else {
      this.home.set(0, -radius, -(bounds.minY + bounds.maxY) / 2);
      this.sceneBox = {
        min: [-reach, -radius - reach, -bounds.maxY],
        max: [reach, -radius + reach, -bounds.minY],
      };
    }
    this.target.copy(this.home);
    this.radius = Math.max(rotary.length, rotary.diameter) * 2.2;
    this.rebuildGrid(Math.max(rotary.length, rotary.diameter) * 2);
    this.updateVisibility();
  }

  private removeBlock(): void {
    if (this.block !== null) {
      this.scene.remove(this.block);
      this.block.geometry.dispose();
      this.block = null;
    }
    if (this.blockWire !== null) {
      // Shares the block's geometry, already disposed.
      this.scene.remove(this.blockWire);
      this.blockWire = null;
    }
    if (this.blockEdges !== null) {
      this.scene.remove(this.blockEdges);
      this.blockEdges.geometry.dispose();
      this.blockEdges = null;
    }
  }

  /**
   * Shows the previewed material in place of the plain block, or brings the
   * block back when `field` is null.
   */
  setHeightfield(field: Heightfield | null, patched: PatchedCells | null = null): void {
    // A field patched from the one shown only re-uploads the changed cells.
    const changed =
      patched !== null && patched.field === field && patched.base === this.field
        ? patched.rects
        : null;
    this.field = field;
    const empty = field === null || field.width === 0 || field.height === 0;
    const job = this.job;
    const rotary = job !== null && isRotary(job);
    const onGpu = field !== null && !empty && !rotary && gpuFits(field, this.gpuLimits);
    this.clearCut(onGpu);
    if (field === null || empty || rotary) {
      this.setHoleWalls(null, false);
    } else if (changed === null || performance.now() - this.holeWallsAt > HOLE_WALL_GAP_MS) {
      // During an animated preview the walls follow a few times a second.
      this.setHoleWalls(field, onGpu);
    }
    if (field === null || empty) {
      this.updateVisibility();
      return;
    }
    if (onGpu) {
      // Same sized fields, such as each step of an animated preview, only
      // re-upload the height texture.
      this.gpu.setField(field, changed);
      if (!this.cutOnGpu) {
        this.scene.add(this.gpu.mesh, this.gpu.wire);
        this.cutOnGpu = true;
      }
      this.cut = this.gpu.mesh;
      this.cutWire = this.gpu.wire;
      this.paint();
      this.updateVisibility();
      return;
    }
    let geometry: BufferGeometry;
    if (rotary) {
      const wrapped = wrappedHeightfield(job, field);
      geometry = wrappedGeometry(wrapped);
      this.vertexGrid = wrapped.grid ?? null;
    } else {
      const mesh = heightfieldMesh(field);
      geometry = new BufferGeometry();
      geometry.setAttribute('position', new BufferAttribute(mesh.positions, 3));
      geometry.setIndex(new BufferAttribute(mesh.indices, 1));
      geometry.computeVertexNormals();
      const box = {
        minX: field.originX,
        minY: field.originY,
        maxX: field.originX + field.width * field.cellSize,
        maxY: field.originY + field.height * field.cellSize,
      };
      geometry.setAttribute(
        JOB_ATTRIBUTE,
        new BufferAttribute(flatJobCoords(mesh.positions, box, 0, field.bottom - field.top), 3),
      );
      this.surface = { xs: mesh.xs, ys: mesh.ys, vertices: mesh.positions.length / 3 };
      this.vertexGrid = { rows: mesh.ys, columns: mesh.xs, rowsAreY: true };
    }
    this.cut = new Mesh(geometry, this.cutMaterial);
    this.paint();
    this.scene.add(this.cut);
    this.cutWire = new Mesh(geometry, this.wireMaterial);
    this.scene.add(this.cutWire);
    this.updateVisibility();
  }

  /** Builds the walls round `field`'s through cuts, or takes them away. */
  private setHoleWalls(field: Heightfield | null, onGpu: boolean): void {
    if (this.holeWalls !== null) {
      this.scene.remove(this.holeWalls);
      this.holeWalls.geometry.dispose();
      this.holeWalls = null;
    }
    this.holeWallsAt = performance.now();
    if (field === null) {
      return;
    }
    // The GPU surface ends at the half way contour of its sampled blocks;
    // the CPU mesh leaves out whole quads round the cut cells.
    const walls = onGpu ? contourHoleWalls(field) : holeWalls(field);
    if (walls.indices.length === 0) {
      return;
    }
    const geometry = new BufferGeometry();
    geometry.setAttribute('position', new BufferAttribute(walls.positions, 3));
    geometry.setIndex(new BufferAttribute(walls.indices, 1));
    if (walls.normals !== undefined) {
      geometry.setAttribute('normal', new BufferAttribute(walls.normals, 3));
    } else {
      geometry.computeVertexNormals();
    }
    this.holeWalls = new Mesh(geometry, this.cutMaterial);
    this.scene.add(this.holeWalls);
  }

  /**
   * Takes the shown surface away before another is shown. The GPU surface
   * stays in the scene when the next one is drawn on the GPU too.
   */
  private clearCut(keepGpu: boolean): void {
    if (this.cut !== null && !this.cutOnGpu) {
      this.scene.remove(this.cut);
      this.cut.geometry.dispose();
    }
    if (this.cutWire !== null && !this.cutOnGpu) {
      this.scene.remove(this.cutWire);
    }
    this.cut = null;
    this.cutWire = null;
    this.surface = null;
    this.vertexGrid = null;
    if (this.cutOnGpu && !keepGpu) {
      this.scene.remove(this.gpu.mesh, this.gpu.wire);
      this.gpu.clear();
      this.cutOnGpu = false;
    }
  }

  /** The model import preview, or null to show the job again. */
  setImportPreview(data: ImportScene | null): void {
    this.importLayer.set(data);
    this.updateVisibility();
  }

  /** Toolpath moves to draw, or null for none. */
  /**
   * Shows the Assembly View's pieces in place of the job, framed by `box`
   * (scene space), or takes them away when null.
   */
  setAssembly(pieces: readonly ScenePiece[] | null, box: Box3 | null): void {
    this.assemblyLayer.set(pieces);
    this.assemblyBox = pieces === null ? null : box;
    this.updateVisibility();
  }

  /** The box the view frames on: the pieces and the furniture preview, else the job. */
  private framingBox(): Box3 {
    const a = this.assemblyBox;
    const b = this.furnitureBox;
    if (a === null || b === null) {
      return a ?? b ?? this.sceneBox;
    }
    return {
      min: [
        Math.min(a.min[0], b.min[0]),
        Math.min(a.min[1], b.min[1]),
        Math.min(a.min[2], b.min[2]),
      ],
      max: [
        Math.max(a.max[0], b.max[0]),
        Math.max(a.max[1], b.max[1]),
        Math.max(a.max[2], b.max[2]),
      ],
    };
  }

  /**
   * Shows the Furniture Designer's preview panels (job space) framed by
   * `box` (job space), or none.
   */
  setFurniturePreview(
    panels: readonly PreviewPanel[] | null,
    box: { min: Vec3; max: Vec3 } | null,
    origin: Vec3 = [0, 0, 0],
  ): void {
    this.furnitureLayer.setPanels(panels, origin);
    this.furnitureBox =
      box === null
        ? null
        : {
            min: [box.min[0], box.min[2], -box.max[1]],
            max: [box.max[0], box.max[2], -box.min[1]],
          };
  }

  /** Draws the preview's overall dimension lines (job space), or none. */
  setFurnitureDimensions(lines: readonly DimensionLine[]): void {
    this.furnitureLayer.setDimensions(lines);
  }

  /** Lights a seam of the design (job space), or none. */
  setFurnitureSeam(seam: { from: Vec3; to: Vec3 } | null, radius: number, picked: boolean): void {
    this.furnitureLayer.setSeam(seam, radius, picked);
  }

  /** Tints the picked piece; `ghosting` pales the others. */
  setAssemblyHighlight(selected: string | null, ghosting: boolean): void {
    this.assemblyLayer.setHighlight(selected, ghosting);
  }

  /** Shows the ghost of a component being placed, or none. */
  setAssemblyGhost(piece: Pick<ScenePiece, 'field' | 'matrix'> | null): void {
    this.assemblyLayer.setPlacing(piece);
  }

  /**
   * Lights a face of a piece: triangles in the piece's own frame and the
   * matrix to the scene, or null to light none.
   */
  setAssemblyFace(triangles: Float32Array | null, matrix: Mat4 | null): void {
    this.assemblyLayer.setFace(triangles, matrix);
  }

  /** Shows Verify Fit's joint faces (empty for none). */
  setAssemblyFit(meshes: readonly FitMesh[]): void {
    this.assemblyLayer.setFit(meshes);
  }

  /** Cuts the Assembly View's pieces by a plane (scene space), or none. */
  setAssemblySection(plane: ScenePlane | null): void {
    this.assemblyLayer.setSection(plane);
  }

  /** Shows a measurement between two job points with its label, or none. */
  setAssemblyMeasure(
    ends: { a: [number, number, number]; b: [number, number, number]; text: string } | null,
  ): void {
    const scene = (p: [number, number, number]): [number, number, number] => [p[0], p[2], -p[1]];
    this.assemblyLayer.setMeasure(
      ends === null ? null : { a: scene(ends.a), b: scene(ends.b), text: ends.text },
    );
  }

  /** The view framing a box in job space from the current angles, the
   * material top at job Z `surfaceZ`. */
  framedOn(box: Box3, surfaceZ = 0): OrbitView {
    return fitView(sceneBoxOf(box, surfaceZ), this.view(), this.aspect, this.projection);
  }

  /**
   * The Assembly View piece under a canvas point (normalised device
   * coordinates) and the point hit on it in job space, or null.
   */
  pickPiece(ndcX: number, ndcY: number): { key: string; point: Vec3Tuple } | null {
    this.aimRay(ndcX, ndcY);
    const hit = this.assemblyLayer.pick(this.raycaster);
    if (hit === null) {
      return null;
    }
    const [x, y, z] = hit.point;
    // Scene (x, z, -y) back to job space.
    return { key: hit.key, point: [x, -z, y] };
  }

  setToolpaths(lines: ToolpathLines | null): void {
    this.cutLines = this.replaceLines(this.cutLines, lines?.cut ?? null, this.cutLineMaterial);
    this.rapidLines = this.replaceLines(
      this.rapidLines,
      lines?.rapid ?? null,
      this.rapidLineMaterial,
    );
    this.extras.setMoveLines(lines);
    this.updateVisibility();
  }

  /**
   * The preview tool turned from its outline ([radius, height above the
   * tip] pairs, tip first) with its shank and holder, the tip at scene point
   * `at`, leaning `lean` radians about scene x and z on a wrapped view, or
   * null to hide it.
   */
  setTool(
    parts: ToolParts | null,
    at: readonly [number, number, number] | null,
    lean?: readonly [number, number],
  ): void {
    this.tool.set(parts, at, lean);
  }

  /** Paints the preview check's codes (CHECK_CODE per cell) on the
   * previewed surface, or none: on the GPU surface, or as vertex marks on
   * the CPU built one (big fields, rotary jobs). */
  setCheck(codes: Uint8Array | null): void {
    if (codes === this.checkCodes) {
      return;
    }
    this.checkCodes = codes;
    this.gpu.setCheck(codes);
    if (!this.cutOnGpu) {
      this.paint();
    }
  }

  /** How many vertices of the CPU built surface carry a check mark over
   * the material, for checks. */
  checkMarkCount(): number {
    const marks = this.cutOnGpu ? undefined : this.cut?.geometry.getAttribute(MARK_ATTRIBUTE);
    if (marks === undefined) {
      return 0;
    }
    let count = 0;
    for (let v = 0; v < marks.count; v += 1) {
      if (marks.getW(v) < 1) {
        count += 1;
      }
    }
    return count;
  }

  /** Marks where the preview checks found issues, `picked` larger. */
  setIssueMarkers(markers: readonly IssueMarker[], picked: number | null, radius: number): void {
    this.issues.set(markers, picked, radius);
  }

  /** How many issue markers show, for checks. */
  issueMarkerCount(): number {
    return this.issues.count();
  }

  /** Whether the preview tool shows, where and how big, for checks. */
  toolState(): ReturnType<ToolModel['state']> {
    return this.tool.state();
  }

  /** Vector segments to draw, or null for none. */
  setVectors(segments: Float32Array | null): void {
    this.vectorLines = this.replaceLines(this.vectorLines, segments, this.vectorMaterial);
    this.updateVisibility();
  }

  private replaceLines(
    old: LineSegments<BufferGeometry, LineBasicMaterial> | null,
    positions: Float32Array | null,
    material: LineBasicMaterial,
  ): LineSegments<BufferGeometry, LineBasicMaterial> | null {
    if (old !== null) {
      this.scene.remove(old);
      old.geometry.dispose();
    }
    if (positions === null || positions.length === 0) {
      return null;
    }
    const lines = new LineSegments(lineGeometry(positions), material);
    this.scene.add(lines);
    return lines;
  }

  /** Applies the visibility toggles and render mode to every object. */
  private updateVisibility(): void {
    const assembly = this.assemblyLayer.active;
    const { model, block } = this.visibility;
    // The Assembly View shows pieces, not the job's toolpaths and vectors.
    const toolpaths = this.visibility.toolpaths && !assembly;
    const vectors = this.visibility.vectors && !assembly;
    // The import preview and the pieces stand in for the material and the relief.
    const previewing = this.importLayer.active || assembly;
    const surface = model && this.cut !== null && !previewing;
    const shaded = this.renderMode !== 'wireframe';
    const overlayWire = this.renderMode === 'shadedWire';
    // The model mesh already carries the block's walls, so the plain block
    // only shows where there is no model to show.
    const plainBlock = block && !surface && !previewing;
    if (this.block !== null) {
      this.block.visible = plainBlock;
    }
    if (this.blockWire !== null) {
      this.blockWire.visible = plainBlock && overlayWire;
    }
    if (this.blockEdges !== null) {
      this.blockEdges.visible = block && (surface || !shaded);
    }
    if (this.cut !== null) {
      this.cut.visible = surface;
    }
    if (this.holeWalls !== null) {
      this.holeWalls.visible = surface && shaded;
    }
    if (this.cutWire !== null) {
      this.cutWire.visible = surface && overlayWire;
    }
    if (this.cutLines !== null) {
      this.cutLines.visible = toolpaths;
    }
    if (this.rapidLines !== null) {
      this.rapidLines.visible = toolpaths;
    }
    if (this.vectorLines !== null) {
      this.vectorLines.visible = vectors;
    }
    this.extras.setToolpathsVisible(toolpaths);
    this.extras.setJobShown(!assembly);
  }

  /**
   * The surface under a canvas point given in normalised device coordinates
   * (-1 to 1 across and up), or null when nothing is there.
   */
  pick(ndcX: number, ndcY: number): SurfacePick | null {
    // Picking reads the flat heightfield; a wrapped view has no flat surface.
    if (this.field === null || (this.job !== null && isRotary(this.job))) {
      return null;
    }
    this.aimRay(ndcX, ndcY);
    const { origin, direction } = this.raycaster.ray;
    const hit = pickSurface(this.field, origin, direction);
    if (hit === null) {
      return null;
    }
    return { x: hit.x, y: -hit.z, scene: new Vector3(hit.x, hit.y, hit.z) };
  }

  /**
   * Where a canvas point (normalised device coordinates) meets the
   * horizontal plane at scene height `sceneY`, in job X and Y, or null when
   * the ray runs parallel to it or away from it.
   */
  pickPlane(ndcX: number, ndcY: number, sceneY: number): SurfacePick | null {
    this.aimRay(ndcX, ndcY);
    const { origin, direction } = this.raycaster.ray;
    if (Math.abs(direction.y) < 1e-9) {
      return null;
    }
    const t = (sceneY - origin.y) / direction.y;
    if (t <= 0) {
      return null;
    }
    const at = origin.clone().addScaledVector(direction, t);
    return { x: at.x, y: -at.z, scene: at };
  }

  /**
   * Where job point (`x`, `y`) at scene height `sceneY` shows on the
   * canvas, in CSS pixels from its top left, or null when it is behind the
   * camera.
   */
  project(x: number, y: number, sceneY: number): { x: number; y: number } | null {
    const camera = this.placeCamera();
    camera.updateMatrixWorld();
    const at = new Vector3(x, sceneY, -y).project(camera);
    if (!Number.isFinite(at.x) || !Number.isFinite(at.y) || at.z < -1 || at.z > 1) {
      return null;
    }
    return {
      x: ((at.x + 1) / 2) * this.viewportWidth,
      y: ((1 - at.y) / 2) * this.viewportHeight,
    };
  }

  /**
   * The ray under a canvas point (normalised device coordinates) in job
   * space (X, Y, Z up): where it starts and its unit direction.
   */
  jobRay(ndcX: number, ndcY: number): { origin: Vec3Tuple; direction: Vec3Tuple } {
    this.aimRay(ndcX, ndcY);
    const { origin, direction } = this.raycaster.ray;
    return {
      origin: [origin.x, -origin.z, origin.y],
      direction: [direction.x, -direction.z, direction.y],
    };
  }

  /** The camera's right and up directions in job space. */
  jobCameraAxes(): { right: Vec3Tuple; up: Vec3Tuple } {
    const camera = this.placeCamera();
    camera.updateMatrixWorld();
    const right = new Vector3().setFromMatrixColumn(camera.matrixWorld, 0);
    const up = new Vector3().setFromMatrixColumn(camera.matrixWorld, 1);
    return { right: [right.x, -right.z, right.y], up: [up.x, -up.z, up.y] };
  }

  /** Where a job point shows on the canvas, CSS pixels, or null when behind the camera. */
  projectJob(point: Vec3Tuple): { x: number; y: number } | null {
    return this.project(point[0], point[1], point[2]);
  }

  private aimRay(ndcX: number, ndcY: number): void {
    const camera = this.placeCamera();
    camera.updateMatrixWorld();
    this.raycaster.setFromCamera(new Vector2(ndcX, ndcY), camera);
    if (this.projection === 'orthographic') {
      // The orthographic ray starts on the far behind near plane; bring it
      // to just outside the job so surface picking marches a short way.
      const { origin, direction } = this.raycaster.ray;
      const box = this.framingBox();
      const [minX, minY, minZ] = box.min;
      const [maxX, maxY, maxZ] = box.max;
      const centre = new Vector3((minX + maxX) / 2, (minY + maxY) / 2, (minZ + maxZ) / 2);
      const reach =
        Math.hypot(maxX - minX, maxY - minY, maxZ - minZ) + centre.distanceTo(this.target);
      const along = this.target.clone().sub(origin).dot(direction);
      origin.addScaledVector(direction, along - reach);
    }
  }

  /**
   * Draws a rectangle in job X and Y (minX, minY, maxX, maxY) at scene
   * height `sceneY`, such as where a dragged component will land; null
   * hides it.
   */
  setOutline(rect: readonly [number, number, number, number] | null, sceneY: number): void {
    if (this.outline !== null) {
      this.scene.remove(this.outline);
      this.outline.geometry.dispose();
      this.outline = null;
    }
    if (rect === null) {
      return;
    }
    const [minX, minY, maxX, maxY] = rect;
    const corners = new Float32Array([
      minX,
      sceneY,
      -minY,
      maxX,
      sceneY,
      -minY,
      maxX,
      sceneY,
      -maxY,
      minX,
      sceneY,
      -maxY,
    ]);
    this.outline = new LineLoop(lineGeometry(corners), this.outlineMaterial);
    this.outline.renderOrder = 1;
    this.scene.add(this.outline);
  }

  /**
   * Shows a see through plane over `rect` (job minX, minY, maxX, maxY) at
   * scene height `sceneY`, such as the Replace Below height; null hides it.
   */
  setCuttingPlane(rect: readonly [number, number, number, number] | null, sceneY: number): void {
    if (this.cuttingPlane !== null) {
      this.scene.remove(this.cuttingPlane);
      this.cuttingPlane.geometry.dispose();
      this.cuttingPlane = null;
    }
    if (rect === null) {
      return;
    }
    const [minX, minY, maxX, maxY] = rect;
    const geometry = new PlaneGeometry(Math.max(maxX - minX, 1e-3), Math.max(maxY - minY, 1e-3));
    geometry.rotateX(-Math.PI / 2);
    this.cuttingPlane = new Mesh(geometry, this.cuttingMaterial);
    this.cuttingPlane.position.set((minX + maxX) / 2, sceneY, -(minY + maxY) / 2);
    this.cuttingPlane.renderOrder = 2;
    this.scene.add(this.cuttingPlane);
  }

  /** Colours the Slice Model slice from scene height `low` to `high` red
   * and the model above it green; null shows the model as it is. */
  setSliceBand(band: { low: number; high: number } | null): void {
    setSliceTint(this.sliceTint, band);
  }

  /** Shows the brush outline lying on the surface, or hides it for null;
   * `arrow` (radians counter clockwise from +X) points a custom brush. */
  setBrush(at: SurfacePick | null, radius: number, arrow: number | null = null): void {
    if (at === null || radius <= 0) {
      if (this.brush !== null) {
        this.brush.visible = false;
      }
      if (this.brushArrow !== null) {
        this.brushArrow.visible = false;
      }
      return;
    }
    if (this.brush === null || this.brushRadius !== radius) {
      if (this.brush !== null) {
        this.scene.remove(this.brush);
        this.brush.geometry.dispose();
      }
      if (this.brushArrow !== null) {
        this.scene.remove(this.brushArrow);
        this.brushArrow.geometry.dispose();
      }
      this.brushArrow = new Mesh(arrowGeometry(radius), this.brushMaterial);
      this.brushArrow.renderOrder = 1;
      this.scene.add(this.brushArrow);
      const ring = new RingGeometry(radius * (1 - BRUSH_RING), radius, 64);
      ring.rotateX(-Math.PI / 2);
      this.brush = new Mesh(ring, this.brushMaterial);
      this.brush.renderOrder = 1;
      this.brushRadius = radius;
      this.scene.add(this.brush);
    }
    this.brush.visible = true;
    this.brush.position.copy(at.scene);
    if (this.brushArrow !== null) {
      this.brushArrow.visible = arrow !== null;
      this.brushArrow.position.copy(at.scene);
      // Job Y is scene -Z, so a turn about +Y is counter clockwise in job XY.
      this.brushArrow.rotation.y = arrow ?? 0;
    }
  }

  private gridSpan(): number {
    const [minX, , minZ] = this.sceneBox.min;
    const [maxX, , maxZ] = this.sceneBox.max;
    return Math.max(maxX - minX, maxZ - minZ) * 2;
  }

  private rebuildGrid(span: number): void {
    if (this.grid !== null) {
      this.scene.remove(this.grid);
      this.grid.geometry.dispose();
      this.grid.material.dispose();
      this.grid = null;
    }
    const colour = new Color(this.gridColour);
    const grid = new GridHelper(span, 20, colour, colour);
    // Just under the block. It is drawn first and leaves the depth buffer
    // alone, so the block and a model surface lying on the material bottom
    // (a model taller than the material) always cover it: from far away the
    // small gap is below the depth buffer's precision.
    grid.position.set(this.home.x, this.home.y * 2 - Math.max(0.5, span * 0.001), this.home.z);
    grid.material.depthWrite = false;
    grid.renderOrder = -1;
    this.scene.add(grid);
    this.grid = grid;
  }

  /** Rotates the camera about the target. Deltas are pixels. */
  orbit(deltaX: number, deltaY: number): void {
    this.theta -= deltaX * ORBIT_PER_PIXEL;
    this.phi = clampPhi(this.phi - deltaY * ORBIT_PER_PIXEL);
  }

  /** Turns the camera about the vertical axis by `radians`. */
  spin(radians: number): void {
    this.theta += radians;
  }

  /** Slides the target across the view plane. Deltas are pixels. */
  /** Scene displacement a pan of `deltaX`, `deltaY` pixels stands for at
   * the target, without moving the view. */
  panDelta(deltaX: number, deltaY: number): [number, number, number] {
    const { halfHeight } = orthoHalfSize(this.radius, this.aspect);
    const scale = (halfHeight * 2) / this.viewportHeight;
    const { right, up } = cameraBasis(this.theta, this.phi);
    return [
      (deltaX * right[0] - deltaY * up[0]) * scale,
      (deltaX * right[1] - deltaY * up[1]) * scale,
      (deltaX * right[2] - deltaY * up[2]) * scale,
    ];
  }

  pan(deltaX: number, deltaY: number): void {
    const { halfHeight } = orthoHalfSize(this.radius, this.aspect);
    // Scene units per pixel at the target, the same for both projections.
    const scale = (halfHeight * 2) / this.viewportHeight;
    const { right, up } = cameraBasis(this.theta, this.phi);
    this.target.x += (-deltaX * right[0] + deltaY * up[0]) * scale;
    this.target.y += (-deltaX * right[1] + deltaY * up[1]) * scale;
    this.target.z += (-deltaX * right[2] + deltaY * up[2]) * scale;
  }

  /** Moves the camera towards or away from the target. */
  dolly(factor: number): void {
    this.radius = Math.min(MAX_RADIUS, Math.max(MIN_RADIUS, this.radius * factor));
  }

  /** The camera as it is now. */
  view(): OrbitView {
    return {
      theta: this.theta,
      phi: this.phi,
      radius: this.radius,
      target: [this.target.x, this.target.y, this.target.z],
    };
  }

  setView(view: OrbitView): void {
    this.theta = view.theta;
    this.phi = clampPhi(view.phi);
    this.radius = Math.min(MAX_RADIUS, Math.max(MIN_RADIUS, view.radius));
    this.target.set(view.target[0], view.target[1], view.target[2]);
  }

  /** The view framing the whole job from the given angles. */
  fitted(angles: OrbitAngles): OrbitView {
    return fitView(this.framingBox(), angles, this.aspect, this.projection);
  }

  resize(width: number, height: number): void {
    if (width === 0 || height === 0) {
      return;
    }
    this.renderer.setPixelRatio(window.devicePixelRatio);
    this.renderer.setSize(width, height, false);
    this.aspect = width / height;
    this.viewportWidth = width;
    this.viewportHeight = height;
    this.perspective.aspect = this.aspect;
    this.perspective.updateProjectionMatrix();
  }

  draw(): void {
    const camera = this.placeCamera();
    this.rim.position.copy(rimDirection(camera.position, this.target));
    this.extras.beforeDraw(camera, this.target);
    this.renderer.render(this.scene, camera);
  }

  /** Draws a frame and answers the canvas while it still holds it, for a picture. */
  snapshot(): HTMLCanvasElement {
    this.draw();
    return this.renderer.domElement;
  }

  /** Puts the active camera where the orbit says and answers it. */
  private placeCamera(): Camera {
    const view = this.view();
    const [x, y, z] = cameraPosition(view);
    const { up } = cameraBasis(view.theta, view.phi);
    const camera = this.projection === 'orthographic' ? this.ortho : this.perspective;
    camera.position.set(x, y, z);
    camera.up.set(up[0], up[1], up[2]);
    camera.lookAt(this.target);
    if (camera === this.ortho) {
      const { halfWidth, halfHeight } = orthoHalfSize(this.radius, this.aspect);
      this.ortho.left = -halfWidth;
      this.ortho.right = halfWidth;
      this.ortho.top = halfHeight;
      this.ortho.bottom = -halfHeight;
      this.ortho.updateProjectionMatrix();
    }
    return camera;
  }

  dispose(): void {
    this.extras.dispose();
    this.removeBlock();
    this.importLayer.dispose();
    this.assemblyLayer.dispose();
    this.furnitureLayer.dispose();
    this.tool.dispose();
    this.issues.dispose();
    if (this.grid !== null) {
      this.grid.geometry.dispose();
    }
    if (this.cut !== null && !this.cutOnGpu) {
      this.cut.geometry.dispose();
    }
    this.gpu.dispose();
    for (const lines of [this.cutLines, this.rapidLines, this.vectorLines]) {
      lines?.geometry.dispose();
    }
    if (this.brush !== null) {
      this.brush.geometry.dispose();
    }
    this.brushArrow?.geometry.dispose();
    this.holeWalls?.geometry.dispose();
    if (this.outline !== null) {
      this.outline.geometry.dispose();
    }
    this.cuttingPlane?.geometry.dispose();
    for (const material of [
      this.cuttingMaterial,
      this.brushMaterial,
      this.outlineMaterial,
      this.material,
      this.cutMaterial,
      this.paintedMaterial,
      this.wireMaterial,
      this.edgeMaterial,
      this.cutLineMaterial,
      this.rapidLineMaterial,
      this.vectorMaterial,
    ]) {
      material.dispose();
    }
    this.renderer.dispose();
  }
}
