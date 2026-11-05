import type { JSX } from 'react';

import { strings } from '../../../lib/formStrings';
import { Dim, EndMill, K, Label, PathHead, VBit, type Picture } from '../kit';

// Toolpath form pictures: side views (material slab, tool, depths) and top
// views (vectors in blue, toolpaths in purple). viewBox 64 x 48.

const t = strings.helpers.toolpaths;

/** Cutting Depths: start depth D and cut depth C below the surface. */
export const cuttingDepths: Picture = {
  id: 'cutting-depths',
  alt: t.cuttingDepths,
  draw: () => (
    <>
      <polygon className={K.material} points="4,12 17,12 17,36 33,36 33,12 46,12 46,44 4,44" />
      <line className={K.faint} x1={4} x2={46} y1={19} y2={19} />
      <EndMill width={14} x={25} y={36} />
      <Dim x1={52} x2={52} y1={12} y2={19} />
      <Dim x1={52} x2={52} y1={19} y2={36} />
      <Label x={59} y={18}>
        D
      </Label>
      <Label x={59} y={31}>
        C
      </Label>
    </>
  ),
};

function vgroove(flat: boolean): Picture {
  return {
    id: flat ? 'vgroove-flat-depth' : 'vgroove-depth',
    alt: flat ? t.vgrooveFlatDepth : t.vgrooveDepth,
    draw: () => (
      <>
        <polygon
          className={K.material}
          points={
            flat
              ? '4,12 12,12 24,32 30,32 42,12 46,12 46,44 4,44'
              : '4,12 12,12 27,37 42,12 46,12 46,44 4,44'
          }
        />
        <line className={K.faint} x1={4} x2={46} y1={17} y2={17} />
        <VBit half={9} height={15} x={flat ? 24 : 27} y={flat ? 32 : 37} />
        <Dim x1={52} x2={52} y1={12} y2={17} />
        <Label x={59} y={17}>
          D
        </Label>
        {flat ? (
          <>
            <Dim x1={52} x2={52} y1={17} y2={32} />
            <Label x={59} y={29}>
              F
            </Label>
          </>
        ) : null}
      </>
    ),
  };
}

/** V-Groove and Sketch: the groove, flat bottomed when a flat depth is set. */
export const vgrooveDepth = vgroove(false);
export const vgrooveFlatDepth = vgroove(true);

export function vgrooveDepthPicture(flat: boolean): Picture {
  return flat ? vgrooveFlatDepth : vgrooveDepth;
}

export type ProfileSide = 'outside' | 'inside' | 'on';
export type Direction = 'climb' | 'conventional';

/** The toolpath loop of each side, and the tool sitting on its left edge. */
const SIDE_LOOP: Record<ProfileSide, { x: number; y: number; w: number; h: number; r: number }> = {
  outside: { x: 7, y: 5, w: 50, h: 38, r: 7 },
  inside: { x: 21, y: 17, w: 22, h: 14, r: 1 },
  on: { x: 14, y: 11, w: 36, h: 26, r: 1 },
};

/**
 * Machine Vectors: a square vector with the tool cutting outside, inside or
 * on it. The arrow on the top edge shows the direction of travel: climb is
 * clockwise outside and counterclockwise inside with a clockwise spindle.
 */
export function machineVectorsPicture(side: ProfileSide, direction: Direction): Picture {
  const loop = SIDE_LOOP[side];
  const clockwise = (direction === 'climb') === (side !== 'inside');
  const top = loop.y;
  const mid = loop.x + loop.w / 2;
  const toolX = loop.x;
  const toolY = loop.y + loop.h / 2;
  return {
    id: `machine-vectors-${side}-${direction}`,
    alt: t.machineVectors(side, direction),
    draw: () => (
      <>
        <rect className={K.vector} height={26} width={36} x={14} y={11} />
        <rect className={K.path} height={loop.h} rx={loop.r} width={loop.w} x={loop.x} y={loop.y} />
        <PathHead
          x1={clockwise ? mid - 6 : mid + 6}
          x2={clockwise ? mid + 2 : mid - 2}
          y1={top}
          y2={top}
        />
        <circle className={K.tool} cx={toolX} cy={toolY} r={5} />
        <circle className={K.dot} cx={toolX} cy={toolY} r={1} />
      </>
    ),
  };
}

/** Separate Last Pass: the wall left at allowance A for the final pass. */
export const lastPass: Picture = {
  id: 'last-pass',
  alt: t.lastPass,
  draw: () => (
    <>
      <rect className={K.material} height={34} width={28} x={32} y={10} />
      <rect className={K.side} height={34} width={8} x={24} y={10} />
      <EndMill width={14} x={16} y={42} />
      <line className={K.faint} x1={24} x2={24} y1={6} y2={46} />
      <Dim x1={24} x2={32} y1={6} y2={6} />
      <Label x={40} y={8}>
        A
      </Label>
    </>
  ),
};

function tabs(triangular: boolean): Picture {
  return {
    id: triangular ? 'tabs-3d' : 'tabs-2d',
    alt: triangular ? t.tabs3d : t.tabs2d,
    draw: () => (
      <>
        <rect className={K.material} height={26} width={18} x={4} y={14} />
        <rect className={K.material} height={26} width={18} x={42} y={14} />
        <polygon
          className={K.material}
          points={triangular ? '22,40 27,31 37,31 42,40' : '22,40 22,31 42,31 42,40'}
        />
        <line className={K.line} x1={2} x2={62} y1={40} y2={40} />
        <Dim x1={22} x2={42} y1={26} y2={26} />
        <Dim x1={32} x2={32} y1={31} y2={40} />
      </>
    ),
  };
}

/** Tabs: a bridge left in the cut, flat or with sloped ends (3D tabs). */
export const tabs2d = tabs(false);
export const tabs3d = tabs(true);

export function tabsPicture(triangular: boolean): Picture {
  return triangular ? tabs3d : tabs2d;
}

export type RampKind = 'smooth' | 'zigZag' | 'spiral';

const RAMP_PATHS: Record<RampKind, string> = {
  smooth: 'M6 12 L34 34 L58 34',
  zigZag: 'M6 12 L30 19 L8 26 L32 34 L58 34',
  spiral: 'M8 12 Q32 20 56 15 Q32 10 8 22 Q32 30 56 25 Q32 20 8 32 Q24 34 58 34',
};

/** Ramp page: how the tool goes down into the material along the cut. */
export function rampPicture(kind: RampKind): Picture {
  return {
    id: `ramp-${kind}`,
    alt: t.ramp[kind],
    draw: () => (
      <>
        <rect className={K.material} height={32} width={56} x={4} y={12} />
        <path className={K.path} d={RAMP_PATHS[kind]} />
        <PathHead x1={50} x2={58} y1={34} y2={34} />
      </>
    ),
  };
}

export type LeadKind = 'none' | 'straight' | 'arc';

/** Leads page: the path onto the vector, none, a straight line or an arc. */
export function leadPicture(kind: LeadKind): Picture {
  const lead =
    kind === 'straight'
      ? 'M8 8 L22 30 L58 30'
      : kind === 'arc'
        ? 'M8 16 A14 14 0 0 0 22 30 L58 30'
        : 'M22 30 L58 30';
  return {
    id: `lead-${kind}`,
    alt: t.lead[kind],
    draw: () => (
      <>
        <line className={K.vector} x1={22} x2={62} y1={36} y2={36} />
        <path className={K.path} d={lead} />
        <PathHead x1={50} x2={58} y1={30} y2={30} />
        <circle
          className={K.dot}
          cx={kind === 'none' ? 22 : 8}
          cy={kind === 'none' ? 30 : kind === 'arc' ? 16 : 8}
          r={1.8}
        />
      </>
    ),
  };
}

/** Corners page: an outside corner cut round, or sharp. */
export function cornersPicture(sharp: boolean): Picture {
  return {
    id: sharp ? 'corners-sharp' : 'corners-round',
    alt: sharp ? t.cornersSharp : t.cornersRound,
    draw: () => (
      <>
        <path className={K.vector} d="M16 46 L16 16 L62 16" />
        <path
          className={K.path}
          d={sharp ? 'M8 46 L8 8 L62 8' : 'M8 46 L8 16 A8 8 0 0 1 16 8 L62 8'}
        />
        <PathHead x1={54} x2={62} y1={8} y2={8} />
      </>
    ),
  };
}

export type ClearanceStrategy = 'offset' | 'raster';

/** Pocket clearance: offset loops, or raster lines across the pocket. */
export function clearancePicture(strategy: ClearanceStrategy): Picture {
  return {
    id: `clearance-${strategy}`,
    alt: t.clearance[strategy],
    draw: () => (
      <>
        <rect className={K.vector} height={36} rx={6} width={52} x={6} y={6} />
        {strategy === 'offset' ? (
          <>
            <rect className={K.path} height={28} rx={4} width={44} x={10} y={10} />
            <rect className={K.path} height={20} rx={2} width={36} x={14} y={14} />
            <rect className={K.path} height={12} width={28} x={18} y={18} />
            <line className={K.path} x1={22} x2={42} y1={24} y2={24} />
          </>
        ) : (
          <path className={K.path} d="M11 11 H53 V17 H11 V23 H53 V29 H11 V35 H53" />
        )}
      </>
    ),
  };
}

/** Peck drilling: the drill goes down in steps and lifts out between them. */
export function peckPicture(peck: boolean): Picture {
  return {
    id: peck ? 'drill-peck' : 'drill-straight',
    alt: peck ? t.drillPeck : t.drillStraight,
    draw: () => (
      <>
        <polygon className={K.material} points="4,14 22,14 22,40 30,40 30,14 46,14 46,44 4,44" />
        <rect className={K.tool} height={22} width={8} x={22} y={2} />
        {peck ? (
          <path className={K.path} d="M54 6 V22 M58 22 V12 M58 12 V30 M62 30 V18 M62 18 V40" />
        ) : (
          <line className={K.path} x1={56} x2={56} y1={6} y2={38} />
        )}
        {peck ? (
          <PathHead x1={62} x2={62} y1={34} y2={40} />
        ) : (
          <PathHead x1={56} x2={56} y1={32} y2={40} />
        )}
      </>
    ),
  };
}

export type PrismSide = 'outside' | 'inside';

/** Prism: a ridge carved from the vector edge up to its center line. */
export function prismPicture(side: PrismSide): Picture {
  return {
    id: `prism-${side}`,
    alt: t.prism[side],
    draw: () => (
      <>
        <polygon
          className={K.material}
          points={
            side === 'outside'
              ? '4,44 4,32 10,32 32,10 54,32 60,32 60,44'
              : '4,44 4,10 10,10 32,32 54,10 60,10 60,44'
          }
        />
        <line className={K.faint} x1={4} x2={60} y1={10} y2={10} />
        <Dim x1={60} x2={60} y1={side === 'outside' ? 10 : 10} y2={32} />
      </>
    ),
  };
}

export type EngraveMode = 'outline' | 'fill';

/** Quick Engrave: a V groove on the outline, or the shape filled with grooves. */
export function engravePicture(mode: EngraveMode): Picture {
  return {
    id: `engrave-${mode}`,
    alt: t.engrave[mode],
    draw: () => (
      <>
        <rect className={K.vector} height={28} rx={4} width={44} x={10} y={10} />
        {mode === 'outline' ? (
          <rect className={K.path} height={24} rx={2} width={40} x={12} y={12} />
        ) : (
          <path className={K.path} d="M13 14 H51 M13 19 H51 M13 24 H51 M13 29 H51 M13 34 H51" />
        )}
      </>
    ),
  };
}

/** Chamfer: an edge cut at the tool angle, with its width and depth. */
export const chamfer: Picture = {
  id: 'chamfer',
  alt: t.chamfer,
  draw: () => (
    <>
      <polygon className={K.material} points="4,14 36,14 50,28 50,44 4,44" />
      <line className={K.faint} x1={36} x2={58} y1={14} y2={14} />
      <line className={K.faint} x1={50} x2={50} y1={8} y2={14} />
      <Dim x1={36} x2={50} y1={8} y2={8} />
      <Dim x1={56} x2={56} y1={14} y2={28} />
      <path className={K.dim} d="M42 14 A6 6 0 0 1 40.2 18.2" />
    </>
  ),
};

export type ThreadType = 'internal' | 'external';

const THREAD_LEFT = 'M24 12 L20 16 L24 20 L20 24 L24 28 L20 32 L24 36';
const THREAD_RIGHT = 'M40 12 L44 16 L40 20 L44 24 L40 28 L44 32 L40 36';

/** Thread Milling: a threaded hole, or a threaded boss. */
export function threadPicture(type: ThreadType): Picture {
  return {
    id: `thread-${type}`,
    alt: t.thread[type],
    draw: () =>
      type === 'internal' ? (
        <>
          <rect className={K.material} height={32} width={56} x={4} y={12} />
          <rect className={K.cut} height={26} width={20} x={22} y={10} />
          <path className={K.path} d={THREAD_LEFT} />
          <path className={K.path} d={THREAD_RIGHT} />
        </>
      ) : (
        <>
          <rect className={K.material} height={8} width={56} x={4} y={36} />
          <rect className={K.material} height={26} width={20} x={22} y={10} />
          <path className={K.path} d="M22 12 L18 16 L22 20 L18 24 L22 28 L18 32 L22 36" />
          <path className={K.path} d="M42 12 L46 16 L42 20 L46 24 L42 28 L46 32 L42 36" />
        </>
      ),
  };
}

export type FluteRamp = 'complete' | 'start' | 'startEnd';

const FLUTE_FLOOR: Record<FluteRamp, string> = {
  complete: '4,14 10,14 10,30 54,30 54,14 60,14 60,44 4,44',
  start: '4,14 10,14 32,30 54,30 54,14 60,14 60,44 4,44',
  startEnd: '4,14 10,14 24,30 40,30 54,14 60,14 60,44 4,44',
};

/** Fluting: the flute at full depth, or ramped in at its start or both ends. */
export function flutePicture(ramp: FluteRamp): Picture {
  return {
    id: `flute-${ramp}`,
    alt: t.flute[ramp],
    draw: () => (
      <>
        <polygon className={K.material} points={FLUTE_FLOOR[ramp]} />
        <line className={K.vector} x1={10} x2={54} y1={8} y2={8} />
        <circle className={K.mark} cx={10} cy={8} r={1.6} />
        <circle className={K.mark} cx={54} cy={8} r={1.6} />
      </>
    ),
  };
}

/** Moulding: the cross section standing upright at the start of the drive rail. */
export const mouldingPosition: Picture = {
  id: 'moulding-position',
  alt: t.mouldingPosition,
  draw: () => (
    <>
      <path className={K.vector} d="M6 42 Q30 30 58 40" />
      <path className={K.model} d="M10 41 V14 Q16 14 18 20 Q20 26 26 26 Q26 34 30 38 Z" />
      <circle className={K.mark} cx={10} cy={41} r={1.8} />
    </>
  ),
};

/** Texture: a field of short random cuts across the area. */
export const texturePattern: Picture = {
  id: 'texture-pattern',
  alt: t.texturePattern,
  draw: () => (
    <>
      <rect className={K.vector} height={36} width={52} x={6} y={6} />
      <path
        className={K.path}
        d="M10 12 Q16 9 22 12 M28 10 Q34 13 40 10 M45 13 Q50 10 55 13 M12 22 Q18 25 24 22 M30 20 Q36 17 42 20 M46 24 Q51 21 56 24 M10 33 Q16 30 22 33 M27 31 Q33 34 39 31 M44 35 Q50 32 55 35"
      />
    </>
  ),
};

export type PhotoStrategy = 'raster' | 'hatch' | 'vectors';

/** Photo Carve: grooves of varying width that draw the picture. */
export function photoPicture(strategy: PhotoStrategy): Picture {
  const grooves =
    strategy === 'raster'
      ? 'M8 12 H56 M8 20 H56 M8 28 H56 M8 36 H56'
      : strategy === 'hatch'
        ? 'M8 12 H56 M8 20 H56 M8 28 H56 M8 36 H56 M14 8 V40 M26 8 V40 M38 8 V40 M50 8 V40'
        : 'M8 36 Q20 8 32 24 T56 12';
  return {
    id: `photo-${strategy}`,
    alt: t.photo[strategy],
    draw: () => (
      <>
        <rect className={K.vector} height={36} width={52} x={6} y={6} />
        <circle className={K.model} cx={32} cy={24} r={11} />
        <path className={K.path} d={grooves} />
      </>
    ),
  };
}

export type Strategy3d =
  'zLevelRaster' | 'zLevelOffset' | 'raster3d' | 'raster' | 'rasterXY' | 'offset';

const DOME = 'M4 40 Q32 4 60 40 Z';

/** 3D Roughing and Finishing: Z level steps or raster lines over the model. */
export function strategy3dPicture(strategy: Strategy3d): Picture {
  const passes: Record<Strategy3d, string> = {
    zLevelRaster: 'M10 34 H54 M16 27 H48 M22 20 H42',
    zLevelOffset: 'M10 34 H54 M16 27 H48 M22 20 H42',
    raster3d: 'M6 38 Q32 6 58 38',
    raster: 'M10 36 Q32 2 54 36 M16 38 Q32 10 48 38',
    rasterXY: 'M10 36 Q32 2 54 36 M16 38 Q32 10 48 38 M24 38 V20 M32 38 V14 M40 38 V20',
    offset: 'M8 38 Q32 4 56 38 M14 38 Q32 12 50 38 M20 38 Q32 20 44 38',
  };
  return {
    id: `strategy-${strategy}`,
    alt: t.strategy3d[strategy],
    draw: () => (
      <>
        <path className={K.model} d={DOME} />
        <line className={K.line} x1={2} x2={62} y1={40} y2={40} />
        <path className={K.path} d={passes[strategy]} />
      </>
    ),
  };
}

export type Boundary3d = 'model' | 'material' | 'selection' | 'level';

/** Machining limit boundary: where a 3D toolpath stops, seen from above. */
export function boundary3dPicture(boundary: Boundary3d): Picture {
  const outline: Record<Boundary3d, JSX.Element> = {
    model: (
      <path className={K.path} d="M16 24 Q16 10 32 10 Q48 10 48 24 Q48 38 32 38 Q16 38 16 24 Z" />
    ),
    material: <rect className={K.path} height={40} width={56} x={4} y={4} />,
    selection: <rect className={K.path} height={22} rx={3} width={30} x={17} y={13} />,
    level: (
      <path className={K.path} d="M12 24 Q12 6 32 6 Q52 6 52 24 Q52 42 32 42 Q12 42 12 24 Z" />
    ),
  };
  return {
    id: `boundary-${boundary}`,
    alt: t.boundary3d[boundary],
    draw: () => (
      <>
        <rect className={K.faint} height={40} width={56} x={4} y={4} />
        <ellipse className={K.model} cx={32} cy={24} rx={14} ry={12} />
        {boundary === 'selection' ? (
          <rect className={K.vector} height={22} rx={3} width={30} x={17} y={13} />
        ) : null}
        {outline[boundary]}
      </>
    ),
  };
}

/** Inlay: the pocket and the plug pressed into it, with the gaps between. */
export const inlayDepths: Picture = {
  id: 'inlay-depths',
  alt: t.inlayDepths,
  draw: () => (
    <>
      <polygon className={K.material} points="4,28 18,28 22,40 42,40 46,28 60,28 60,46 4,46" />
      <polygon className={K.model} points="12,8 52,8 52,18 44,18 41,36 23,36 20,18 12,18" />
      <Dim x1={8} x2={8} y1={28} y2={40} />
      <Dim x1={56} x2={56} y1={18} y2={28} />
    </>
  ),
};

/** Toolpath Array Copy: the toolpath repeated in rows and columns. */
export const toolpathGrid: Picture = {
  id: 'toolpath-grid',
  alt: t.toolpathGrid,
  draw: () => (
    <>
      <rect className={K.path} height={12} rx={2} width={16} x={6} y={6} />
      <rect className={K.faint} height={12} rx={2} width={16} x={28} y={6} />
      <rect className={K.faint} height={12} rx={2} width={16} x={6} y={28} />
      <rect className={K.faint} height={12} rx={2} width={16} x={28} y={28} />
      <Dim x1={22} x2={28} y1={12} y2={12} />
      <Dim x1={14} x2={14} y1={18} y2={28} />
    </>
  ),
};

/** Toolpath Tiling: a long job cut in tiles, each overlapping the next. */
export const tiling: Picture = {
  id: 'toolpath-tiling',
  alt: t.tiling,
  draw: () => (
    <>
      <rect className={K.material} height={24} width={60} x={2} y={12} />
      <rect className={K.faint} height={28} width={24} x={4} y={10} />
      <rect className={K.faint} height={28} width={24} x={22} y={10} />
      <rect className={K.faint} height={28} width={20} x={40} y={10} />
      <Dim x1={22} x2={28} y1={42} y2={42} />
    </>
  ),
};

export type ToolKind =
  | 'endMill'
  | 'ballNose'
  | 'vBit'
  | 'engravingTool'
  | 'taperedBallNose'
  | 'radiusedEndMill'
  | 'formTool'
  | 'drill'
  | 'diamondDrag'
  | 'laserTool'
  | 'threadMill';

const TOOL_TIPS: Record<ToolKind, string> = {
  endMill: 'M22 6 V40 H42 V6',
  ballNose: 'M22 6 V30 A10 10 0 0 0 42 30 V6',
  vBit: 'M22 6 V20 L32 40 L42 20 V6',
  engravingTool: 'M22 6 V20 L30 40 H34 L42 20 V6',
  taperedBallNose: 'M22 6 V14 L29 36 A3 3 0 0 0 35 36 L42 14 V6',
  radiusedEndMill: 'M22 6 V34 A6 6 0 0 0 28 40 H36 A6 6 0 0 0 42 34 V6',
  formTool: 'M22 6 V22 Q22 28 26 30 Q30 32 28 40 H36 Q34 32 38 30 Q42 28 42 22 V6',
  drill: 'M22 6 V32 L32 42 L42 32 V6',
  diamondDrag: 'M26 6 V30 L32 40 L38 30 V6',
  laserTool: 'M24 6 H40 L36 22 H28 Z',
  threadMill: 'M26 6 V22 L20 26 L26 30 L20 34 L26 38 H38 L44 34 L38 30 L44 26 L38 22 V6',
};

/** Tool geometry: the tool's side profile with its diameter. */
export function toolPicture(kind: ToolKind): Picture {
  return {
    id: `tool-${kind}`,
    alt: t.tool[kind],
    draw: () => (
      <>
        <path className={K.tool} d={TOOL_TIPS[kind]} />
        {kind === 'laserTool' ? (
          <path className={K.path} d="M32 22 V42" />
        ) : (
          <Dim
            x1={kind === 'diamondDrag' ? 26 : 22}
            x2={kind === 'diamondDrag' ? 38 : 42}
            y1={2}
            y2={2}
          />
        )}
        {kind === 'vBit' || kind === 'engravingTool' || kind === 'drill' ? (
          <path className={K.dim} d="M27 30 A6 6 0 0 0 37 30" />
        ) : null}
      </>
    ),
  };
}

export type InlayKind = 'maleStraight' | 'maleStepped' | 'femalePocket' | 'femaleHole';

const INLAY_SHAPES: Record<InlayKind, JSX.Element> = {
  maleStraight: <rect className={K.model} height={22} width={28} x={18} y={14} />,
  maleStepped: <path className={K.model} d="M12 36 V28 H18 V14 H46 V28 H52 V36 Z" />,
  femalePocket: (
    <polygon className={K.material} points="4,14 18,14 18,30 46,30 46,14 60,14 60,40 4,40" />
  ),
  femaleHole: (
    <>
      <rect className={K.material} height={26} width={14} x={4} y={14} />
      <rect className={K.material} height={26} width={14} x={46} y={14} />
    </>
  ),
};

/** Create Inlay Toolpath: the insert or hole each kind cuts. */
export function inlayKindPicture(kind: InlayKind): Picture {
  return {
    id: `inlay-${kind}`,
    alt: t.inlayKind[kind],
    draw: () => (
      <>
        {INLAY_SHAPES[kind]}
        <line
          className={K.line}
          x1={2}
          x2={62}
          y1={kind.startsWith('male') ? 36 : 40}
          y2={kind.startsWith('male') ? 36 : 40}
        />
      </>
    ),
  };
}

export type LaserStrategy = 'outside' | 'inside' | 'on' | 'hatch';

/** Laser Cut and Fill: the beam outside, inside or on the vector, or filling it. */
export function laserCutPicture(strategy: LaserStrategy): Picture {
  const loop =
    strategy === 'outside' ? (
      <rect className={K.path} height={34} rx={4} width={46} x={9} y={7} />
    ) : strategy === 'inside' ? (
      <rect className={K.path} height={18} width={30} x={17} y={15} />
    ) : strategy === 'on' ? (
      <rect className={K.path} height={26} width={38} x={13} y={11} />
    ) : (
      <path className={K.path} d="M15 14 H49 M15 19 H49 M15 24 H49 M15 29 H49 M15 34 H49" />
    );
  return {
    id: `laser-${strategy}`,
    alt: t.laserCut[strategy],
    draw: () => (
      <>
        <rect className={K.vector} height={26} width={38} x={13} y={11} />
        {loop}
      </>
    ),
  };
}

export type LaserPictureStrategy = 'raster' | 'hatch' | 'vectors';

/** Laser Picture: the image burned in lines, crossing lines or along vectors. */
export function laserPicturePicture(strategy: LaserPictureStrategy): Picture {
  const burns =
    strategy === 'raster'
      ? 'M8 12 H56 M8 18 H56 M8 24 H56 M8 30 H56 M8 36 H56'
      : strategy === 'hatch'
        ? 'M8 12 H56 M8 24 H56 M8 36 H56 M14 8 V40 M26 8 V40 M38 8 V40 M50 8 V40'
        : 'M8 36 Q20 8 32 24 T56 12 M8 28 Q20 0 32 16 T56 4';
  return {
    id: `laser-picture-${strategy}`,
    alt: t.laserPicture[strategy],
    draw: () => (
      <>
        <rect className={K.faint} height={36} width={52} x={6} y={6} />
        <circle className={K.model} cx={32} cy={24} r={12} />
        <path className={K.path} d={burns} />
      </>
    ),
  };
}

/** Laser Sketch fill: hatch lines one way, or crossing when cross hatch is on. */
export function laserSketchPicture(cross: boolean): Picture {
  return {
    id: cross ? 'laser-sketch-cross' : 'laser-sketch-hatch',
    alt: cross ? t.laserSketchCross : t.laserSketchHatch,
    draw: () => (
      <>
        <circle className={K.vector} cx={32} cy={24} r={18} />
        <path className={K.path} d="M18 14 L42 38 M15 22 L34 41 M24 10 L46 32 M32 7 L49 24" />
        {cross ? (
          <path className={K.path} d="M46 14 L22 38 M49 22 L30 41 M40 10 L18 32 M32 7 L15 24" />
        ) : null}
      </>
    ),
  };
}
