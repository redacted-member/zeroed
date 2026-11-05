// The model or preview surface drawn on the GPU: a fixed grid (see
// surfaceGrid.ts) lifted in the vertex shader by the heightfield, uploaded
// as the kernel sent it: u16 codes as an R16UI texture the shader decodes,
// or floats as an R32F one. A changed heightfield of the same size only
// re-uploads the texture (just the changed cells during the preview
// animation), which is what keeps the preview animation cheap. Component
// colours and machined colours are a palette texture and a texture of one
// palette entry per cell.

import type {
  MeshBasicMaterial,
  MeshStandardMaterial,
  PixelFormat,
  Texture,
  WebGLRenderer,
} from 'three';
import {
  BufferAttribute,
  BufferGeometry,
  Box2,
  DataTexture,
  DoubleSide,
  FloatType,
  LinearFilter,
  LinearMipmapLinearFilter,
  Mesh,
  RedFormat,
  RedIntegerFormat,
  RGBAFormat,
  UnsignedByteType,
  UnsignedShortType,
  Vector2,
  Vector3,
  type IUniform,
} from 'three';

import type { Heightfield } from '../../bridge/cam';
import type { Heights } from '../../bridge/heights';
import type { PaletteColours } from '../../bridge/componentTree';
import { finishPalette, finishUniforms, type FinishUniforms } from './componentFinish';
import type { OwnerPalette } from './ownerPalette';
import { gridKey, surfaceGrid, GPU_MAX_SIDE, type GridShape } from './surfaceGrid';
import type { SliceTintUniforms } from './sliceTint';
import { PAINT, patchSurfaceMaterial } from './surfaceShader';
import { clipRect, type CellRect } from './textureRect';

/** What the top surface is painted with. */
export type SurfacePaint =
  | { kind: 'none' }
  | { kind: 'cells'; colours: PaletteColours }
  | { kind: 'owners'; palette: OwnerPalette };

/** What the GPU path needs from the WebGL context. */
export interface GpuLimits {
  maxTextureSize: number;
  vertexTextures: number;
}

/** Height textures the vertex shader reads: the top and the underside, each
 * as codes and as floats. */
const VERTEX_TEXTURES = 4;

/** Whether the GPU surface can draw `field` with these limits. */
export function gpuFits(field: Pick<Heightfield, 'width' | 'height'>, limits: GpuLimits): boolean {
  return (
    limits.vertexTextures >= VERTEX_TEXTURES &&
    field.width >= 2 &&
    field.height >= 2 &&
    field.width <= limits.maxTextureSize &&
    field.height <= limits.maxTextureSize
  );
}

/** Textures whose whole data is still waiting to be uploaded. */
const fullUploadPending = new WeakSet<DataTexture>();

/** Marks `texture` for a whole upload and notes when it has happened. */
function uploadWhole(texture: DataTexture): DataTexture {
  texture.clearUpdateRanges();
  texture.needsUpdate = true;
  fullUploadPending.add(texture);
  texture.onUpdate = () => {
    fullUploadPending.delete(texture);
  };
  return texture;
}

function floatTexture(data: Float32Array, width: number, height: number): DataTexture {
  const texture = new DataTexture(data, width, height, RedFormat, FloatType);
  return uploadWhole(texture);
}

function byteTexture(data: Uint8Array, width: number, height: number): DataTexture {
  const texture = new DataTexture(data, width, height, RGBAFormat, UnsignedByteType);
  return uploadWhole(texture);
}

/** A laser preview's char per cell, 0 to 255, read as 0 to 1. Mipmapped, so
 * a burnt line narrower than a pixel still shows, as a paler line, when the
 * view is zoomed out. */
function burnTexture(data: Uint8Array, width: number, height: number): DataTexture {
  const texture = new DataTexture(data, width, height, RedFormat, UnsignedByteType);
  texture.minFilter = LinearMipmapLinearFilter;
  texture.magFilter = LinearFilter;
  texture.generateMipmaps = true;
  return uploadWhole(texture);
}

/** u16 height codes, decoded by the shader. */
function codeTexture(data: Uint16Array, width: number, height: number): DataTexture {
  const texture = new DataTexture(data, width, height, RedIntegerFormat, UnsignedShortType);
  texture.internalFormat = 'R16UI';
  return uploadWhole(texture);
}

/** Palette entries per cell: one byte each for small palettes, two for
 * large ones and for preview owners. */
function indexTexture(data: Uint8Array | Uint16Array, width: number, height: number): DataTexture {
  const wide = data instanceof Uint16Array;
  const texture = new DataTexture(
    data,
    width,
    height,
    RedIntegerFormat,
    wide ? UnsignedShortType : UnsignedByteType,
  );
  texture.internalFormat = wide ? 'R16UI' : 'R8UI';
  return uploadWhole(texture);
}

/** Uploads one rectangle of a texture's current data. */
export type RectUpload = (texture: DataTexture, rect: CellRect) => void;

/** Cells that changed since the last upload, and how to upload them alone. */
interface PartialUpload {
  rects: readonly CellRect[];
  upload: RectUpload;
}

/** A rectangle upload through `renderer`, one texSubImage2D call. */
export function rectUpload(renderer: WebGLRenderer): RectUpload {
  return (texture, rect) => {
    const { data, width, height } = texture.image;
    // A texture never uploaded itself, so three.js copies straight from its
    // data into the target instead of blitting between framebuffers.
    const source = new DataTexture(
      data,
      width,
      height,
      texture.format as PixelFormat,
      texture.type,
    );
    const min = new Vector2(rect.x0, rect.y0);
    const max = new Vector2(rect.x0 + rect.width, rect.y0 + rect.height);
    renderer.copyTextureToTexture(source, texture, new Box2(min, max), min);
  };
}

/**
 * Points a texture uniform at `data`, re-uploading into the same texture
 * when the size is unchanged and making a new one otherwise. When only the
 * `partial` cells of the data already uploaded differ, just they go.
 */
function retarget<T extends Float32Array | Uint8Array | Uint16Array>(
  uniform: IUniform<DataTexture>,
  data: T,
  width: number,
  height: number,
  make: (data: T, width: number, height: number) => DataTexture,
  placeholder: DataTexture,
  partial: PartialUpload | null = null,
): void {
  const current = uniform.value;
  const same =
    current.image.width === width &&
    current.image.height === height &&
    (current.image.data as T | null)?.constructor === data.constructor;
  if (current !== placeholder && same) {
    if (partial !== null && current.image.data === data && !fullUploadPending.has(current)) {
      for (const changed of partial.rects) {
        const rect = clipRect(changed, width, height);
        if (rect !== null) {
          partial.upload(current, rect);
        }
      }
      return;
    }
    current.image.data = data;
    uploadWhole(current);
    return;
  }
  if (current !== placeholder) {
    current.dispose();
  }
  uniform.value = make(data, width, height);
}

function release(uniform: IUniform<DataTexture>, placeholder: DataTexture): void {
  if (uniform.value !== placeholder) {
    uniform.value.dispose();
    uniform.value = placeholder;
  }
}

/** The uniforms one height grid (the top or the underside) is read through. */
interface HeightUniforms {
  floats: IUniform<DataTexture>;
  codes: IUniform<DataTexture>;
  /** (coded, base, step) */
  code: IUniform<Vector3>;
}

export class GpuSurface {
  readonly material: MeshStandardMaterial;
  readonly wireMaterial: MeshBasicMaterial;
  readonly mesh: Mesh<BufferGeometry, MeshStandardMaterial>;
  readonly wire: Mesh<BufferGeometry, MeshBasicMaterial>;
  private readonly blank = {
    float: floatTexture(new Float32Array(1), 1, 1),
    code: codeTexture(new Uint16Array(1), 1, 1),
    bytes: byteTexture(new Uint8Array(4), 1, 1),
    index: indexTexture(new Uint16Array(1), 1, 1),
    check: indexTexture(new Uint8Array(1), 1, 1),
    burn: burnTexture(new Uint8Array(1), 1, 1),
  };
  private readonly uniforms: {
    carveHeights: IUniform<DataTexture>;
    carveFloors: IUniform<DataTexture>;
    carveTopCodes: IUniform<DataTexture>;
    carveFloorCodes: IUniform<DataTexture>;
    carveTopCode: IUniform<Vector3>;
    carveFloorCode: IUniform<Vector3>;
    carveTop: IUniform<number>;
    carveFloor: IUniform<number>;
    carveHasFloor: IUniform<boolean>;
    carveStep: IUniform<number>;
    carveCellSize: IUniform<number>;
    carvePaint: IUniform<number>;
    carveIndex: IUniform<DataTexture>;
    carvePalette: IUniform<DataTexture>;
    carveLitho: IUniform<number>;
    carveMaterialMap: IUniform<Texture>;
    carveMaterialOn: IUniform<boolean>;
    carveFinishPalette: IUniform<DataTexture>;
    carveFinishOn: IUniform<boolean>;
    carveJobFinish: IUniform<number>;
    carveCheck: IUniform<DataTexture>;
    carveCheckOn: IUniform<boolean>;
    carveBurn: IUniform<DataTexture>;
    carveBurnOn: IUniform<boolean>;
  } & FinishUniforms;
  private key = '';
  /** The component colours in the index texture. */
  private lastColours: PaletteColours | null = null;
  /** The finish palette made for the component colours last shown. */
  private finishOf: { colours: PaletteColours; finish: Uint8Array | null } | null = null;
  private field: Heightfield | null = null;
  private paint: SurfacePaint = { kind: 'none' };
  /** A check code per cell to paint, or null. */
  private check: Uint8Array | null = null;

  constructor(
    look: MeshStandardMaterial,
    wire: MeshBasicMaterial,
    private readonly upload: RectUpload,
    tint?: SliceTintUniforms,
  ) {
    this.uniforms = {
      carveHeights: { value: this.blank.float },
      carveFloors: { value: this.blank.float },
      carveTopCodes: { value: this.blank.code },
      carveFloorCodes: { value: this.blank.code },
      carveTopCode: { value: new Vector3() },
      carveFloorCode: { value: new Vector3() },
      carveTop: { value: 0 },
      carveFloor: { value: 0 },
      carveHasFloor: { value: false },
      carveStep: { value: 1 },
      carveCellSize: { value: 1 },
      carvePaint: { value: PAINT.none },
      carveIndex: { value: this.blank.index },
      carvePalette: { value: this.blank.bytes },
      carveLitho: { value: -1 },
      carveMaterialMap: { value: this.blank.bytes },
      carveMaterialOn: { value: false },
      carveFinishPalette: { value: this.blank.bytes },
      carveFinishOn: { value: false },
      carveJobFinish: { value: 0 },
      carveCheck: { value: this.blank.check },
      carveCheckOn: { value: false },
      carveBurn: { value: this.blank.burn },
      carveBurnOn: { value: false },
      ...finishUniforms(),
    };
    this.material = patchSurfaceMaterial(look.clone(), this.uniforms, true, tint);
    this.material.side = DoubleSide;
    this.wireMaterial = patchSurfaceMaterial(wire.clone(), this.uniforms, false);
    const geometry = new BufferGeometry();
    this.mesh = new Mesh(geometry, this.material);
    this.wire = new Mesh(geometry, this.wireMaterial);
    // The grid's own positions are flat; the shader moves them.
    this.mesh.frustumCulled = false;
    this.wire.frustumCulled = false;
  }

  /**
   * Shows `field`, rebuilding the grid only when its size or place changed.
   * `changed` says only those cells differ from the field shown before, in
   * the same arrays, so only their rows are uploaded.
   */
  setField(field: Heightfield, changed: readonly CellRect[] | null = null): void {
    const shape: GridShape = {
      width: field.width,
      height: field.height,
      cellSize: field.cellSize,
      originX: field.originX,
      originY: field.originY,
      underside: field.floor !== undefined,
    };
    const key = gridKey(shape);
    const sameGrid = key === this.key;
    if (!sameGrid) {
      this.key = key;
      this.rebuild(shape);
    }
    const u = this.uniforms;
    u.carveTop.value = field.top;
    u.carveFloor.value = field.bottom - field.top;
    u.carveCellSize.value = field.cellSize;
    const partial =
      sameGrid && this.field !== null && changed !== null
        ? { rects: changed, upload: this.upload }
        : null;
    const top = { floats: u.carveHeights, codes: u.carveTopCodes, code: u.carveTopCode };
    const under = { floats: u.carveFloors, codes: u.carveFloorCodes, code: u.carveFloorCode };
    this.setHeights(top, field.z, field.width, field.height, partial);
    // A flat underside needs no texture: the shader puts it at the bottom.
    if (field.floor !== undefined && field.floor !== 'flat') {
      this.setHeights(under, field.floor, field.width, field.height, partial);
      u.carveHasFloor.value = true;
    } else {
      this.releaseHeights(under);
      u.carveHasFloor.value = false;
    }
    this.field = field;
    this.applyPaint(partial);
    this.applyCheck();
    this.applyBurn();
  }

  /** Uploads a laser preview's char per cell, or turns it off when the
   * field has none. The whole of it goes up each time, so its mipmaps are
   * made again. */
  private applyBurn(): void {
    const u = this.uniforms;
    const field = this.field;
    const burn = field?.burn;
    if (field === null || burn === undefined || burn.length !== field.width * field.height) {
      u.carveBurnOn.value = false;
      release(u.carveBurn, this.blank.burn);
      return;
    }
    retarget(u.carveBurn, burn, field.width, field.height, burnTexture, this.blank.burn);
    u.carveBurnOn.value = true;
  }

  /** Paints a check code per cell (see CHECK_CODE), or none. */
  setCheck(codes: Uint8Array | null): void {
    if (codes === this.check) {
      return;
    }
    this.check = codes;
    this.applyCheck();
  }

  /** Uploads the check codes when they fit the field shown, else turns them off. */
  private applyCheck(): void {
    const u = this.uniforms;
    const field = this.field;
    const codes = this.check;
    if (field === null || codes === null || codes.length !== field.width * field.height) {
      u.carveCheckOn.value = false;
      release(u.carveCheck, this.blank.check);
      return;
    }
    if (u.carveCheck.value.image.data !== codes) {
      retarget(u.carveCheck, codes, field.width, field.height, indexTexture, this.blank.check);
    }
    u.carveCheckOn.value = true;
  }

  /** Uploads `heights` to the codes or the floats texture, letting go of
   * the other. */
  private setHeights(
    target: HeightUniforms,
    heights: Heights,
    width: number,
    height: number,
    partial: PartialUpload | null,
  ): void {
    if (heights.code === null) {
      release(target.codes, this.blank.code);
      retarget(target.floats, heights.data, width, height, floatTexture, this.blank.float, partial);
      target.code.value.set(0, 0, 0);
      return;
    }
    release(target.floats, this.blank.float);
    retarget(target.codes, heights.data, width, height, codeTexture, this.blank.code, partial);
    target.code.value.set(1, heights.code.base, heights.code.step);
  }

  private releaseHeights(target: HeightUniforms): void {
    release(target.floats, this.blank.float);
    release(target.codes, this.blank.code);
    target.code.value.set(0, 0, 0);
  }

  /**
   * A picture laid over the material, stretched over the job, or null for
   * the plain material colour. The caller owns the texture.
   */
  setMaterialTexture(texture: Texture | null): void {
    this.uniforms.carveMaterialMap.value = texture ?? this.blank.bytes;
    this.uniforms.carveMaterialOn.value = texture !== null;
  }

  /** The grain of the job's material (a finish slot), 0 for a plain colour. */
  setJobFinish(slot: number): void {
    this.uniforms.carveJobFinish.value = slot;
  }

  /** Lithophane shading at `strength` from 0 to 1, or null for none. */
  setLithophane(strength: number | null): void {
    this.uniforms.carveLitho.value = strength === null ? -1 : Math.min(Math.max(strength, 0), 1);
  }

  setPaint(paint: SurfacePaint): void {
    if (paint === this.paint) {
      return;
    }
    this.paint = paint;
    this.applyPaint(null);
  }

  /**
   * Uploads what the paint needs: the palette and the entry of each cell.
   * Component colours already uploaded are kept, and only the `changed`
   * owners go up when the field was patched.
   */
  private applyPaint(changed: PartialUpload | null): void {
    const u = this.uniforms;
    const field = this.field;
    const paint = this.paint;
    if (field === null) {
      return;
    }
    const cells = field.width * field.height;
    let palette: Uint8Array | null = null;
    let finish: Uint8Array | null = null;
    if (paint.kind === 'cells') {
      const { colours } = paint;
      if (
        colours.width === field.width &&
        colours.height === field.height &&
        colours.index.length >= cells
      ) {
        if (this.lastColours !== colours || u.carveIndex.value === this.blank.index) {
          retarget(
            u.carveIndex,
            colours.index,
            field.width,
            field.height,
            indexTexture,
            this.blank.index,
          );
          this.lastColours = colours;
        }
        palette = colours.palette;
        if (this.finishOf?.colours !== colours) {
          this.finishOf = { colours, finish: finishPalette(colours) };
        }
        finish = this.finishOf.finish;
      }
    } else if (paint.kind === 'owners' && field.owner.length === cells) {
      this.lastColours = null;
      retarget(
        u.carveIndex,
        field.owner,
        field.width,
        field.height,
        indexTexture,
        this.blank.index,
        changed,
      );
      palette = paint.palette.rgba;
    }
    if (palette === null || palette.length < 4) {
      finish = null;
    }
    this.applyFinish(finish);
    if (palette === null || palette.length < 4) {
      this.lastColours = null;
      release(u.carveIndex, this.blank.index);
      release(u.carvePalette, this.blank.bytes);
      u.carvePaint.value = PAINT.none;
      return;
    }
    if (u.carvePalette.value.image.data !== palette) {
      release(u.carvePalette, this.blank.bytes);
      u.carvePalette.value = byteTexture(palette, Math.floor(palette.length / 4), 1);
    }
    // An owner palette with a colour at entry 0 paints the uncut surface.
    u.carvePaint.value =
      paint.kind === 'owners' && (palette[3] ?? 0) > 0 ? PAINT.all : PAINT.palette;
  }

  /** Uploads the finish of each palette entry, or turns finishes off. */
  private applyFinish(finish: Uint8Array | null): void {
    const u = this.uniforms;
    if (finish === null) {
      release(u.carveFinishPalette, this.blank.bytes);
      u.carveFinishOn.value = false;
      return;
    }
    if (u.carveFinishPalette.value.image.data !== finish) {
      release(u.carveFinishPalette, this.blank.bytes);
      u.carveFinishPalette.value = byteTexture(finish, Math.floor(finish.length / 4), 1);
    }
    u.carveFinishOn.value = true;
  }

  private rebuild(shape: GridShape): void {
    const grid = surfaceGrid(shape, GPU_MAX_SIDE);
    const geometry = new BufferGeometry();
    geometry.setAttribute('position', new BufferAttribute(grid.positions, 3));
    geometry.setAttribute('normal', new BufferAttribute(grid.normals, 3));
    geometry.setAttribute('carveCell', new BufferAttribute(grid.cells, 2));
    geometry.setAttribute('carveRole', new BufferAttribute(grid.roles, 1));
    geometry.setIndex(new BufferAttribute(grid.indices, 1));
    this.mesh.geometry.dispose();
    this.mesh.geometry = geometry;
    this.wire.geometry = geometry;
    this.uniforms.carveStep.value = grid.step;
  }

  /** Lets go of the textures and grid while no field is shown. */
  clear(): void {
    const u = this.uniforms;
    this.releaseHeights({ floats: u.carveHeights, codes: u.carveTopCodes, code: u.carveTopCode });
    this.releaseHeights({
      floats: u.carveFloors,
      codes: u.carveFloorCodes,
      code: u.carveFloorCode,
    });
    release(u.carveIndex, this.blank.index);
    release(u.carvePalette, this.blank.bytes);
    release(u.carveFinishPalette, this.blank.bytes);
    release(u.carveCheck, this.blank.check);
    release(u.carveBurn, this.blank.burn);
    u.carveFinishOn.value = false;
    u.carveCheckOn.value = false;
    u.carveBurnOn.value = false;
    this.mesh.geometry.dispose();
    const empty = new BufferGeometry();
    this.mesh.geometry = empty;
    this.wire.geometry = empty;
    this.key = '';
    this.field = null;
    this.lastColours = null;
  }

  dispose(): void {
    this.clear();
    this.mesh.geometry.dispose();
    this.material.dispose();
    this.wireMaterial.dispose();
    this.blank.float.dispose();
    this.blank.code.dispose();
    this.blank.bytes.dispose();
    this.blank.index.dispose();
    this.blank.check.dispose();
    this.blank.burn.dispose();
  }
}
