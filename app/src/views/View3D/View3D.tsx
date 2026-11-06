import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type JSX,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import type { Texture } from 'three';

import { api } from '../../bridge/formApi';
import type { Preferences } from '../../bridge/prefs';
import type { ComponentTransformParams, Composite, CustomBrushParams } from '../../bridge/model';
import type { JobSetup } from '../../bridge/types';
import { moveVisibility } from '../../lib/moveGroups';
import { CTRL } from '../../lib/keyLabels';
import { isRotary, rotaryOf } from '../../lib/rotary';
import { isDoubleSided } from '../../lib/sides';
import { registerShadedSnapshot } from '../../lib/shadedImage';
import { strings } from '../../lib/formStrings';
import { readCanvasPalette, readScenePalette } from '../../lib/theme';
import { unitSuffix } from '../../lib/units';
import { modalDialogOpen } from '../../lib/dialogKeys';
import { useDocumentStore } from '../../state/documentStore';
import { useImportStore } from '../../state/importStore';
import { useMeshPreviewStore } from '../../state/meshPreviewStore';
import { pickedComponents, useModelStore } from '../../state/modelStore';
import { usePrefsStore } from '../../state/prefsStore';
import { toolpathColour } from '../../forms/PreviewToolpathsForm/model';
import { shownPreview, usePreviewStore } from '../../state/previewStore';
import { strokeBrush, useSculptStore } from '../../state/sculptStore';
import { doubleClickEdit, useSliceViewStore, wheelEdit } from '../../state/sliceViewStore';
import { penPressure } from './penPressure';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { useView3dStore } from '../../state/view3dStore';
import { PlanePin } from '../PlanePin/PlanePin';
import { HandleValueBox } from '../HandleValueBox/HandleValueBox';
import { PieceGizmo } from '../PieceGizmo/PieceGizmo';
import { RotateRings } from '../RotateRings/RotateRings';
import { PlaneHandles } from '../PlaneHandles/PlaneHandles';
import { UsageInfo } from '../UsageInfo/UsageInfo';
import { watchPixelRatio } from '../../lib/pixelRatio';
import { TransformHandles } from '../TransformHandles/TransformHandles';
import { View3DToolbar } from '../View3DToolbar/View3DToolbar';
import { useDropTarget, type DropResolver } from '../../panels/ClipartPanel/drop';
import { ViewCube } from '../ViewCube/ViewCube';
import { jobMaterial, materialLook } from './appearance';
import { finishSlot } from './componentFinish';
import { loadMaterialTexture, texturePathFor } from './materialTexture';
import { blankTurnMatrix, restingTurn } from './blankTurn';
import { toolScenePosition, wrappedToolPose } from './toolMarker';
import { blendView, VIEW_PRESETS, type OrbitView, type ViewPreset } from './camera3d';
import { clipDropPoint } from './clipDrop';
import { componentAt } from './componentAt';
import { tintColours, type HighlightMask } from './highlight';
import { importScene, materialRect, segmentPositions } from './importPreview';
import { LiveTransform } from './liveTransform';
import { flatComposite, modelField, planeDepth, withUnderside } from './modelField';
import { volumeLine } from './modelVolume';
import { machinedPalette } from './ownerPalette';
import { dollyFactor, dragFor, sculptDragFor, sculptWheel, type CameraDrag } from './navigation';
import { toolpathLines, vectorLines, wrappedToolpathLines } from './overlayLines';
import type { SurfacePaint } from './gpuSurface';
import { exportScene } from './exportPreview';
import { View3DRenderer } from './renderer';
import { useAssemblyFit } from './useAssemblyFit';
import { usePreviewChecks } from './usePreviewChecks';
import { useAssemblyView } from './useAssemblyView';
import { useFurnitureView } from './useFurnitureView';
import { useFurnitureFaces } from './useFurnitureFaces';
import { FaceMenu } from '../FaceMenu/FaceMenu';
import { FurnitureOverlay } from '../FurnitureOverlay/FurnitureOverlay';
import { useRotateRings } from './useRotateRings';
import { overlayColours } from './sculptOverlay';
import { usePlaneHandles } from './usePlaneHandles';
import { endsTransform, shadowKeyStep, startsTwiddle, TWIDDLE_KEY, typingIn } from './viewKeys';
import {
  frameFor,
  layoutFrame,
  NO_TRANSFORM,
  pivotAfter,
  postOf,
  type ShownFrame,
  type TransformFrame,
} from './handleFrame';
import {
  isHeightHandle,
  isProfileHandle,
  typedHeightValues,
  typedTransform,
  valueFields,
  type AnyHandle,
  type TypedValues,
  type ValueField,
} from './handleValues';
import {
  dragHeights,
  heightPatch,
  riseAlong,
  sameHeights,
  type HeightKind,
  type Heights,
} from './heightHandles';
import {
  dragAngle,
  profileArgs,
  profileHeightOf,
  profilePointOf,
  typedAngle,
} from './profileHandle';
import {
  applyParams,
  centreOf,
  dragParams,
  handleKey,
  isIdentity,
  type HandleKind,
  type XY,
} from './transformHandles';
import { markerPlacement, shownJob } from './viewOptions';

import './View3D.scss';

const ZOOM_PER_WHEEL_NOTCH = 1.1;
const LEFT_BUTTON = 0;
const RIGHT_BUTTON = 2;
/** Stroke points closer than this fraction of the brush diameter are dropped. */
const STROKE_SPACING = 0.125;
/** How long an animated view change takes, milliseconds. */
const GLIDE_MS = 350;
/** Twiddle turning speed, radians per second. */
const TWIDDLE_SPEED = 0.45;
/** While sculpting, this key toggles Show Visible Model. */
const SHOW_VISIBLE_KEY = 'f';
/** While sculpting, the wheel with this key held turns a custom brush. */
const ROTATE_BRUSH_KEY = 'r';
/** Chord tolerance for vectors drawn in 3D, millimetres. */
const VECTOR_TOLERANCE = 0.05;
/** Vectors float this far above the material top so they are not hidden in it. */
const VECTOR_LIFT = 0.05;
/** The key held to drag a component, as the status bar names it. */
const MOVE_KEY = CTRL;

type DragMode = CameraDrag | 'sculpt' | 'move';

/** A component being dragged across the model. */
interface ComponentDrag {
  pointerId: number;
  id: number;
  bbox: readonly [number, number, number, number];
  /** Scene height of the plane the drag slides on. */
  planeY: number;
  start: { x: number; y: number };
  offset: { x: number; y: number };
}

/** The handles on the canvas with the values of the active one. */
interface ShownHandles extends ShownFrame {
  centreMode: boolean;
  fields: ValueField[] | null;
}

/** A drag on a transform handle. */
type HandleDrag =
  | {
      type: 'box';
      pointerId: number;
      handle: HandleKind;
      /** Pressed on the middle while it was already the active handle. */
      second: boolean;
      start: XY;
      screen: XY;
      live: LiveTransform<ComponentTransformParams>;
    }
  | { type: 'pivot'; pointerId: number; start: XY; screen: XY; pivot: XY }
  | {
      type: 'height';
      pointerId: number;
      kind: HeightKind;
      screen: XY;
      heights: Heights;
      live: LiveTransform<Heights>;
    }
  | {
      type: 'profile';
      pointerId: number;
      screen: XY;
      angle: number;
      live: LiveTransform<number>;
    };

/** Where the custom brush's arrow points, radians counter clockwise from
 * +X, or null for the round brush. */
function brushArrow(custom: CustomBrushParams | null, heading: number): number | null {
  if (custom === null) {
    return null;
  }
  const angle = (custom.angle * Math.PI) / 180;
  return custom.dynamic ? heading + angle : angle;
}

/** How far the value box sits from its handle, CSS pixels. */
const VALUE_BOX_GAP = 14;
/** A press that moves less than this many pixels is a click. */
const CLICK_SLOP = 4;

async function refreshMirrors(): Promise<void> {
  await useDocumentStore.getState().refresh();
  await useModelStore.getState().refresh();
}

function showError(error: unknown): void {
  useDocumentStore.setState({
    lastError: error instanceof Error ? error.message : String(error),
  });
}

function sameFrame(a: ShownFrame | null, b: ShownFrame | null): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

interface Stroke {
  pointerId: number;
  points: { x: number; y: number }[];
  /** Pen pressure at each point, null for a mouse or with tablet support off. */
  pressures: number[] | null;
  /** Shift was held: the Undo Brush erases. */
  erase: boolean;
  /** Where the stroke began on the screen, for V and H. */
  client: { x: number; y: number };
}

/** While sculpting, holding these keys keeps a stroke straight up or
 * straight across the view. */
const VERTICAL_KEY = 'v';
const HORIZONTAL_KEY = 'h';

/** Whether this display can give the view a WebGL2 context at all. */
function hasWebGl(): boolean {
  try {
    return document.createElement('canvas').getContext('webgl2') !== null;
  } catch {
    return false;
  }
}

/** Job Z of the material top. */
function surfaceZ(job: JobSetup): number {
  return job.zZero === 'bottom' ? job.thickness : 0;
}

/** Makes the renderer follow the 3D view preferences and the theme. */
function applyPrefs(renderer: View3DRenderer, prefs: Preferences, container: Element): void {
  const view3d = prefs.view3d;
  const scene = readScenePalette(container);
  const canvas = readCanvasPalette(container);
  renderer.setPalette(scene);
  renderer.setFlatShading(view3d.shading === 'flat');
  renderer.setProjection(view3d.projection);
  renderer.setRenderMode(view3d.renderMode);
  // The job's own look; the preference is only the default for new jobs.
  const chosen = jobMaterial(useDocumentStore.getState().job.appearance, view3d);
  renderer.setLook(materialLook(chosen.material, scene.material, chosen.colour));
  renderer.setJobFinish(finishSlot(chosen.material));
  renderer.extras.setOptions({ ...view3d, shadowDepth: useView3dStore.getState().shadowDepth });
  renderer.extras.setColours({
    plunge: canvas.toolpathPlunge,
    retract: canvas.toolpathRetract,
    join: canvas.toolpathJoin,
  });
  renderer.setLineColours({
    cut: canvas.toolpathCut,
    rapid: canvas.toolpathRapid,
    vector: canvas.node,
  });
  renderer.setVisibility({
    model: view3d.showModel,
    block: view3d.showBlock,
    toolpaths: view3d.showToolpaths,
    vectors: view3d.showVectors,
  });
}

export function View3D(): JSX.Element {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const rendererRef = useRef<View3DRenderer | null>(null);
  const frameRef = useRef<number | null>(null);
  /** Lays the Orientate Model rotation rings out after each frame. */
  const ringsLayoutRef = useRef<(() => void) | null>(null);
  /** Lays the Assembly View's piece handles out after each frame. */
  const piecesLayoutRef = useRef<(() => void) | null>(null);
  /** Lays the Furniture Designer's dimension and seam labels out after each frame. */
  const furnitureLayoutRef = useRef<(() => void) | null>(null);
  /** Lays the 3D Segmenting plane's handles out after each frame. */
  const planeLayoutRef = useRef<(() => void) | null>(null);
  const dragRef = useRef<{ pointerId: number; x: number; y: number; mode: DragMode } | null>(null);
  const [available] = useState(hasWebGl);
  const strokeRef = useRef<Stroke | null>(null);
  const moveRef = useRef<ComponentDrag | null>(null);
  /** The component with transform handles, and the drag on them so far. */
  const transformRef = useRef<TransformFrame | null>(null);
  const handleDragRef = useRef<HandleDrag | null>(null);
  const clickRef = useRef<{ pointerId: number; x: number; y: number } | null>(null);
  const rightClickRef = useRef<{ pointerId: number; x: number; y: number } | null>(null);
  /** V or H held while sculpting: keep the stroke straight. */
  const constrainRef = useRef<'v' | 'h' | null>(null);
  const [transformId, setTransformId] = useState<number | null>(null);
  const [shown, setShown] = useState<ShownHandles | null>(null);
  /** The handle whose values the value box shows. */
  const [active, setActiveState] = useState<AnyHandle | null>(null);
  const activeRef = useRef<AnyHandle | null>(null);
  const units = useDocumentStore((state) => state.job.units);
  /** The Replace Below plane's middle in job XY and scene height. */
  const pinRef = useRef<{ x: number; y: number; sceneY: number } | null>(null);
  const [pinShown, setPinShown] = useState<XY | null>(null);
  const pinDragRef = useRef<{
    pointerId: number;
    startY: number;
    height: number;
    pixelsPerMm: number;
  } | null>(null);
  const sculpting = useSculptStore((state) => state.active);
  const [angles, setAngles] = useState({
    theta: VIEW_PRESETS.iso.theta,
    phi: VIEW_PRESETS.iso.phi,
  });
  /** A running glide between views, stopped by any direct camera move. */
  const glideRef = useRef<number | null>(null);
  const [twiddling, setTwiddling] = useState(false);
  const twiddleOn = useView3dStore((state) => state.twiddle);
  const hoverRef = useRef(false);
  /** R is held: the wheel turns the custom brush. */
  const rotateKeyRef = useRef(false);
  /** The pointer's last surface point and heading, for the custom brush's
   * arrow under Dynamic Rotation. */
  const brushTrailRef = useRef<XY | null>(null);
  const brushHeadingRef = useRef(0);
  const [hovering, setHovering] = useState(false);
  const hasComponents = useModelStore((state) => state.components.length > 0);

  const autoWrap = usePrefsStore((state) => state.prefs.view3d.autoWrap);
  const colorShaded = usePrefsStore((state) => state.prefs.view3d.colorShaded);
  const toolpathPrefs = usePrefsStore((state) => state.prefs.toolpaths);
  const documentJob = useDocumentStore((state) => state.job);
  // View > Auto-Wrapping View off lays a rotary job flat.
  const job = useMemo(() => shownJob(documentJob, autoWrap), [autoWrap, documentJob]);
  const revision = useDocumentStore((state) => state.revision);
  const layers = useDocumentStore((state) => state.layers);
  const preview = usePreviewStore(shownPreview);
  const composite = useModelStore((state) => state.composite);
  const colours = useModelStore((state) => state.colors);
  const moves = useToolpathStore((state) => state.moves);
  const showToolpaths = usePrefsStore((state) => state.prefs.view3d.showToolpaths);
  const showVectors = usePrefsStore((state) => state.prefs.view3d.showVectors);
  const showCube = usePrefsStore((state) => state.prefs.view3d.showCube);
  const navigation = usePrefsStore((state) => state.prefs.view3d.navigation);
  // While sculpting, the sculpted component shows on its own unless Show
  // Visible Model is on.
  const showVisible = useSculptStore((state) => state.showVisible);
  const sculptedId = useModelStore((state) => (sculpting ? state.selectedId : null));
  const modelStamp = useModelStore((state) => state.stamp);
  const [alone, setAlone] = useState<Composite | null>(null);
  useEffect(() => {
    if (sculptedId === null || showVisible || modelStamp === null) {
      setAlone(null);
      return;
    }
    let cancelled = false;
    void api.model
      .composite([sculptedId])
      .then((field) => {
        if (!cancelled) {
          setAlone(field);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setAlone(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [modelStamp, sculptedId, showVisible]);
  const shownModel = alone ?? composite;
  // Multi Sided View: the other side's model shows as the underside.
  const multiSided = usePrefsStore((state) => state.prefs.view3d.multiSided);
  const activeSide = useDocumentStore((state) => state.activeSide);
  const wantsOther = multiSided && isDoubleSided(documentJob) && !sculpting;
  const [otherSide, setOtherSide] = useState<Composite | null>(null);
  useEffect(() => {
    if (!wantsOther) {
      setOtherSide(null);
      return;
    }
    let cancelled = false;
    void api.model
      .composite(undefined, true)
      .then((field) => {
        if (!cancelled) {
          setOtherSide(field);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setOtherSide(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [activeSide, modelStamp, wantsOther]);
  // Ctrl+Shift+Alt+V: the composite model volume, while the model is the
  // one measured.
  const shownVolume = useUiStore((state) => state.modelVolume);
  const volume = volumeLine(shownVolume, modelStamp, documentJob.units);
  // Where the modelling plane sits under this side's face: both sides of a
  // double sided job share it.
  const depth = useMemo(
    () =>
      planeDepth(composite?.max ?? 0, composite?.otherPeak ?? otherSide?.max ?? 0, job, activeSide),
    [activeSide, composite, job, otherSide],
  );
  // A toolpath preview shows what will be cut; otherwise show the model.
  const heightfield = useMemo(() => {
    if (preview !== null) {
      return preview;
    }
    const other = wantsOther ? otherSide : null;
    const top = shownModel ?? (other === null ? null : flatComposite(other));
    if (top === null) {
      return null;
    }
    const field = modelField(top, job, depth);
    return other === null ? field : withUnderside(field, other, depth);
  }, [depth, job, otherSide, preview, shownModel, wantsOther]);

  const scheduleDraw = useCallback(() => {
    if (frameRef.current !== null) {
      return;
    }
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = null;
      const renderer = rendererRef.current;
      if (renderer === null) {
        return;
      }
      renderer.draw();
      ringsLayoutRef.current?.();
      piecesLayoutRef.current?.();
      furnitureLayoutRef.current?.();
      planeLayoutRef.current?.();
      const frame = transformRef.current;
      const laid =
        frame === null
          ? null
          : layoutFrame((x, y, sceneY) => renderer.project(x, y, sceneY), frame);
      const chosen = activeRef.current;
      const next =
        laid === null || frame === null
          ? null
          : {
              ...laid,
              centreMode: frame.centreMode,
              fields: chosen === null ? null : valueFields(chosen, frame),
            };
      setShown((old) => (sameFrame(old, next) ? old : next));
      const pin = pinRef.current;
      const pinAt = pin === null ? null : renderer.project(pin.x, pin.y, pin.sceneY);
      setPinShown((old) =>
        old === pinAt || (old !== null && pinAt !== null && old.x === pinAt.x && old.y === pinAt.y)
          ? old
          : pinAt,
      );
      const { theta, phi } = renderer.view();
      setAngles((old) => (old.theta === theta && old.phi === phi ? old : { theta, phi }));
    });
  }, []);

  const rings = useRotateRings(rendererRef, canvasRef, ringsLayoutRef, scheduleDraw);
  // Bumped once the renderer exists: these hooks' effects run before the one that
  // makes it, so a design waiting when the view first opens is shown on the next pass.
  const [rendererEpoch, setRendererEpoch] = useState(0);
  const pieces = useAssemblyView(
    rendererRef,
    canvasRef,
    piecesLayoutRef,
    scheduleDraw,
    rendererEpoch,
  );
  const furniture = useFurnitureView(rendererRef, furnitureLayoutRef, scheduleDraw, rendererEpoch);
  const faces = useFurnitureFaces(rendererRef, canvasRef);
  const setRingModel = rings.setModel;
  const planeHandles = usePlaneHandles(rendererRef, canvasRef, planeLayoutRef, scheduleDraw);
  const setPlaneFrame = planeHandles.setFrame;

  const setActive = useCallback(
    (handle: AnyHandle | null) => {
      activeRef.current = handle;
      setActiveState(handle);
      scheduleDraw();
    },
    [scheduleDraw],
  );

  const stopGlide = useCallback(() => {
    if (glideRef.current !== null) {
      cancelAnimationFrame(glideRef.current);
      glideRef.current = null;
    }
  }, []);

  /** Moves the camera to `to`, gliding there when the preferences say so. */
  const goTo = useCallback(
    (to: OrbitView) => {
      const renderer = rendererRef.current;
      if (renderer === null) {
        return;
      }
      stopGlide();
      if (!usePrefsStore.getState().prefs.view3d.animateCamera) {
        renderer.setView(to);
        scheduleDraw();
        return;
      }
      const from = renderer.view();
      const start = performance.now();
      const step = (now: number) => {
        const t = Math.min(1, (now - start) / GLIDE_MS);
        renderer.setView(blendView(from, to, t));
        scheduleDraw();
        glideRef.current = t < 1 ? requestAnimationFrame(step) : null;
      };
      glideRef.current = requestAnimationFrame(step);
    },
    [scheduleDraw, stopGlide],
  );
  useAssemblyFit(rendererRef, scheduleDraw, goTo);

  const showPreset = useCallback(
    (preset: ViewPreset) => {
      const renderer = rendererRef.current;
      if (renderer !== null) {
        goTo(renderer.fitted(VIEW_PRESETS[preset]));
      }
    },
    [goTo],
  );

  const zoomToFit = useCallback(() => {
    const renderer = rendererRef.current;
    if (renderer !== null) {
      goTo(renderer.fitted(renderer.view()));
    }
  }, [goTo]);

  // Standard views and zoom to fit asked for from the menus.
  useEffect(
    () =>
      useView3dStore.subscribe((state, previous) => {
        const request = state.request;
        if (request === null || request === previous.request) {
          return;
        }
        if (request.kind === 'preset') {
          showPreset(request.preset);
        } else if (request.kind === 'frame') {
          const renderer = rendererRef.current;
          if (renderer !== null) {
            goTo(renderer.framedOn(request.box, request.surfaceZ));
          }
        } else {
          zoomToFit();
        }
      }),
    [goTo, showPreset, zoomToFit],
  );

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (canvas === null || container === null || !available) {
      return;
    }

    let renderer: View3DRenderer;
    try {
      renderer = new View3DRenderer(canvas);
    } catch (error) {
      console.error('the 3D view could not start', error);
      return;
    }
    applyPrefs(renderer, usePrefsStore.getState().prefs, container);
    renderer.setJob(useDocumentStore.getState().job);
    rendererRef.current = renderer;
    setRendererEpoch((epoch) => epoch + 1);
    registerShadedSnapshot(() => renderer.snapshot());
    if (window.carveDev !== undefined) {
      window.carveDev.view3d = renderer;
    }

    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry === undefined) {
        return;
      }
      renderer.resize(entry.contentRect.width, entry.contentRect.height);
      scheduleDraw();
    });
    observer.observe(canvas);
    const stopRatio = watchPixelRatio(() => {
      const { width, height } = canvas.getBoundingClientRect();
      renderer.resize(width, height);
      scheduleDraw();
    });

    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      stopGlide();
      // Shift + wheel raises or lowers the Slice Model form's picked height.
      const slice = useSliceViewStore.getState();
      const nudge = slice.active && !modalDialogOpen() ? wheelEdit(event) : null;
      if (nudge !== null) {
        slice.requestEdit(nudge);
        return;
      }
      const sculpt = useSculptStore.getState();
      // Shift turns a vertical wheel sideways on some systems.
      const delta = event.deltaY !== 0 ? event.deltaY : event.deltaX;
      if (sculpt.active && delta !== 0) {
        const action = sculptWheel({
          alt: event.altKey,
          shift: event.shiftKey,
          ctrl: event.ctrlKey || event.metaKey,
        });
        if (action !== 'zoom') {
          // R held turns a custom brush instead.
          const kind = rotateKeyRef.current
            ? 'angle'
            : action === 'strength'
              ? 'strength'
              : 'diameter';
          sculpt.requestAdjust(
            kind,
            delta < 0 ? 1 : -1,
            action === 'fineDiameter' || (kind === 'angle' && (event.ctrlKey || event.metaKey)),
          );
          return;
        }
      }
      const factor = event.deltaY > 0 ? ZOOM_PER_WHEEL_NOTCH : 1 / ZOOM_PER_WHEEL_NOTCH;
      renderer.dolly(factor);
      scheduleDraw();
    };
    canvas.addEventListener('wheel', onWheel, { passive: false });

    const scheme = window.matchMedia('(prefers-color-scheme: dark)');
    const onSchemeChange = () => {
      applyPrefs(renderer, usePrefsStore.getState().prefs, container);
      scheduleDraw();
    };
    scheme.addEventListener('change', onSchemeChange);
    const stopPrefs = usePrefsStore.subscribe((state, previous) => {
      if (state.prefs !== previous.prefs) {
        onSchemeChange();
      }
    });
    const stopShadow = useView3dStore.subscribe((state, previous) => {
      if (state.shadowDepth !== previous.shadowDepth) {
        onSchemeChange();
      }
    });
    // Ctrl + NumPad+ or - deepens or lightens the shadow shading.
    const onShadowKey = (event: KeyboardEvent) => {
      const by = shadowKeyStep(event, usePrefsStore.getState().prefs.view3d.shadowShading);
      if (by === 0) {
        return;
      }
      event.preventDefault();
      useView3dStore.getState().stepShadow(by);
    };
    window.addEventListener('keydown', onShadowKey);

    return () => {
      stopRatio();
      observer.disconnect();
      canvas.removeEventListener('wheel', onWheel);
      scheme.removeEventListener('change', onSchemeChange);
      stopPrefs();
      stopShadow();
      window.removeEventListener('keydown', onShadowKey);
      stopGlide();
      if (frameRef.current !== null) {
        cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
      registerShadedSnapshot(null);
      if (window.carveDev?.view3d === renderer) {
        window.carveDev.view3d = null;
      }
      renderer.dispose();
      rendererRef.current = null;
    };
  }, [available, scheduleDraw, stopGlide]);

  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    renderer.setJob(job);
    const container = containerRef.current;
    if (container !== null) {
      applyPrefs(renderer, usePrefsStore.getState().prefs, container);
    }
    scheduleDraw();
  }, [job, scheduleDraw]);

  // Declared after the mount and job effects, so it runs after them and the
  // surface replaces the plain block they draw.
  useEffect(() => {
    // An animation step says which cells it changed, so only those upload.
    rendererRef.current?.setHeightfield(heightfield, usePreviewStore.getState().patched);
    scheduleDraw();
  }, [heightfield, scheduleDraw]);

  // While the model import flow is open its model shows in place of the job,
  // and so does a mesh the Export Model form calculated.
  const exportMesh = useMeshPreviewStore((store) => store.mesh);
  const exportWireframe = useMeshPreviewStore((store) => store.wireframe);
  const importPreview = useImportStore((store) => store.preview);
  const importView = useImportStore((store) => store.view);
  const importPlace = useImportStore((store) => store.place);
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    const scene =
      importPreview === null
        ? null
        : importScene(importPreview, {
            ...importView,
            zeroPlane: importView.step === 'position' ? importPlace.zeroPlane : null,
            discardBelow: importPlace.discardBelow,
            thickness: job.thickness,
            material: isRotary(job) ? null : materialRect(job),
            rotary: isRotary(job)
              ? { axis: job.rotary.axis, radius: job.rotary.diameter / 2 }
              : null,
          });
    renderer.setImportPreview(
      scene ?? (exportMesh === null ? null : exportScene(exportMesh, job, -depth, exportWireframe)),
    );
    // The rotation rings show while the model is orientated.
    setRingModel(
      scene === null || importPreview === null
        ? null
        : importView.step === 'orientate'
          ? scene.positions
          : importView.step === 'segment' && importView.layout && importView.ringSegment !== null
            ? segmentPositions(scene.positions, importPreview, importView.ringSegment)
            : null,
    );
    setPlaneFrame(scene?.planeHandle ?? null);
    scheduleDraw();
  }, [
    depth,
    exportMesh,
    exportWireframe,
    importPlace,
    importPreview,
    importView,
    job,
    scheduleDraw,
    setPlaneFrame,
    setRingModel,
  ]);

  // Component colours show on the model; a toolpath preview paints its
  // machined areas as the Preview Toolpaths form says.
  const fill = usePreviewStore((state) => state.fill);
  const fillColour = usePreviewStore((state) =>
    state.fill === 'surface' ? state.surfaceColour : state.fillColour,
  );
  const chosenColours = usePreviewStore((state) => state.toolpathColours);
  const toolpathIds = useToolpathStore((state) => state.toolpaths.map((t) => t.id).join(','));
  // The machined colours are a palette by cell owner, so they do not change
  // as the preview animation cuts.
  const machined = useMemo(() => {
    const ids = toolpathIds === '' ? [] : toolpathIds.split(',').map(Number);
    return machinedPalette(ids.length, fill, fillColour, (owner) => {
      const id = ids[owner - 1];
      if (id === undefined) {
        return null;
      }
      const colour = toolpathColour(owner - 1, id, chosenColours);
      return colour === 'none' ? null : colour;
    });
  }, [chosenColours, fill, fillColour, toolpathIds]);
  const previewing = preview !== null;
  // The selected components are tinted where they show and where other
  // components hide them.
  const pickedKey = useModelStore((state) => pickedComponents(state).join(','));
  const anchoring = useModelStore(
    (state) => state.anchorPick !== null && state.anchorPick.points.length < 2,
  );
  const highlightOn = useModelStore((state) => state.highlight);
  const stamp = useModelStore((state) => state.stamp);
  const [highlight, setHighlight] = useState<HighlightMask | null>(null);
  useEffect(() => {
    if (previewing || sculpting || !highlightOn || pickedKey === '' || stamp === null) {
      setHighlight(null);
      return;
    }
    let cancelled = false;
    void api.model.tree
      .highlight(pickedKey.split(',').map(Number))
      .then((mask) => {
        if (!cancelled) {
          setHighlight(mask);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setHighlight(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [highlightOn, pickedKey, previewing, sculpting, stamp]);
  // While sculpting, a selected picture lies over the model.
  const overlay = useSculptStore((state) => (sculpting ? state.overlay : null));
  const [overlayImage, setOverlayImage] = useState<{ id: number; image: ImageBitmap } | null>(null);
  const overlayId = overlay?.imageId ?? null;
  useEffect(() => {
    if (overlayId === null) {
      setOverlayImage(null);
      return;
    }
    let cancelled = false;
    void api.bitmap
      .pixels(overlayId)
      .then((blob) => createImageBitmap(blob))
      .then((image) => {
        if (!cancelled) {
          setOverlayImage({ id: overlayId, image });
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [overlayId]);
  const overlayCells = useMemo(
    () =>
      overlay === null || !overlay.shown || overlayImage === null || shownModel === null
        ? null
        : overlayColours(shownModel, overlayImage.image, overlay.quad, overlay.opacity),
    [overlay, overlayImage, shownModel],
  );

  useEffect(() => {
    let paint: SurfacePaint = { kind: 'none' };
    if (overlayCells !== null && !previewing) {
      paint = { kind: 'cells', colours: overlayCells };
    } else if (previewing) {
      paint = machined === null ? paint : { kind: 'owners', palette: machined };
    } else {
      const cells = tintColours(colorShaded ? colours : null, highlight);
      if (cells !== null) {
        paint = { kind: 'cells', colours: cells };
      }
    }
    rendererRef.current?.setPaint(paint);
    scheduleDraw();
  }, [colorShaded, colours, highlight, machined, overlayCells, previewing, scheduleDraw]);

  useEffect(() => {
    rendererRef.current?.extras.setPreviewShown(previewing);
    scheduleDraw();
  }, [previewing, scheduleDraw]);

  // A preview can be shaded as a lithophane, lit from behind.
  const lithophane = usePreviewStore((state) => (state.lithophane ? state.lithoStrength : null));
  useEffect(() => {
    rendererRef.current?.setLithophane(previewing && lithophane !== null ? lithophane / 100 : null);
    scheduleDraw();
  }, [lithophane, previewing, scheduleDraw]);

  // A material texture from the user's list lies over the material.
  const texturePath = usePrefsStore((state) =>
    texturePathFor(documentJob.appearance, state.prefs.view3d.textures),
  );
  useEffect(() => {
    if (texturePath === null) {
      rendererRef.current?.setMaterialTexture(null);
      scheduleDraw();
      return;
    }
    let cancelled = false;
    let made: Texture | null = null;
    void loadMaterialTexture(texturePath)
      .then((texture) => {
        if (cancelled) {
          texture.dispose();
          return;
        }
        made = texture;
        rendererRef.current?.setMaterialTexture(texture);
        scheduleDraw();
      })
      .catch(() => {
        rendererRef.current?.setMaterialTexture(null);
      });
    return () => {
      cancelled = true;
      rendererRef.current?.setMaterialTexture(null);
      made?.dispose();
    };
  }, [scheduleDraw, texturePath]);

  // The animated preview draws the tool where it is.
  const previewTool = usePreviewStore((state) => state.tool);
  const drawTool = usePreviewStore((state) => state.drawTool);
  useEffect(() => {
    const renderer = rendererRef.current;
    const pose =
      previewTool !== null && isRotary(job)
        ? wrappedToolPose(previewTool, rotaryOf(job), surfaceZ(job))
        : null;
    // The blank turns under the tool, as on the machine, drawn or not. With no
    // tool running it rests with the cut work facing up.
    if (isRotary(job)) {
      const { axis, diameter } = rotaryOf(job);
      const radius = diameter / 2;
      const turn = pose?.turn ?? (moves === null ? 0 : restingTurn(moves, axis, radius));
      renderer?.setBlankTurn(blankTurnMatrix(axis, radius, turn));
    } else {
      renderer?.setBlankTurn(null);
    }
    const shown = drawTool ? previewTool : null;
    renderer?.setTool(
      shown,
      shown === null ? null : (pose?.at ?? toolScenePosition(shown, surfaceZ(job))),
    );
    scheduleDraw();
  }, [drawTool, job, moves, previewTool, scheduleDraw]);

  usePreviewChecks(rendererRef, scheduleDraw, job, preview, surfaceZ(job));

  // Origin arrows and the modeling plane follow the job and the model.
  useEffect(() => {
    rendererRef.current?.extras.setPlacement(markerPlacement(job, depth));
    scheduleDraw();
  }, [depth, job, scheduleDraw]);

  // Toolpaths are drawn where the machine takes them: flat, or round the blank.
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    if (!showToolpaths || moves === null) {
      renderer.setToolpaths(null);
    } else if (isRotary(job)) {
      renderer.setToolpaths(
        wrappedToolpathLines(moves, job, surfaceZ(job), moveVisibility(toolpathPrefs)),
      );
    } else {
      renderer.setToolpaths(toolpathLines(moves, surfaceZ(job), moveVisibility(toolpathPrefs)));
    }
    scheduleDraw();
  }, [job, moves, scheduleDraw, showToolpaths, toolpathPrefs]);

  useEffect(() => {
    if (!showVectors || isRotary(job)) {
      rendererRef.current?.setVectors(null);
      scheduleDraw();
      return;
    }
    let cancelled = false;
    const hidden = new Set(layers.filter((layer) => !layer.visible).map((layer) => layer.id));
    void api.entities
      .display({ tolerance: VECTOR_TOLERANCE })
      .then((reply) => {
        if (cancelled) {
          return;
        }
        const vectors = reply.entities
          .filter((entity) => !hidden.has(entity.layer))
          .map((entity) => ({
            closed: entity.closed,
            runs: entity.runs.map((run) =>
              reply.xy.slice(run.offset * 2, (run.offset + run.count) * 2),
            ),
          }));
        rendererRef.current?.setVectors(vectorLines(vectors, VECTOR_LIFT));
        scheduleDraw();
      })
      .catch((error: unknown) => console.error('the 3D view could not read the vectors', error));
    return () => {
      cancelled = true;
    };
  }, [job, layers, revision, scheduleDraw, showVectors]);

  // The Replace Below form shows its height as a plane over the job.
  const cuttingPlane = useModelStore((state) => state.cuttingPlane);
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    if (cuttingPlane === null || isRotary(job)) {
      renderer.setCuttingPlane(null, 0);
      pinRef.current = null;
    } else {
      const rect = materialRect(job);
      const sceneY = -depth + cuttingPlane.height;
      renderer.setCuttingPlane(rect, sceneY);
      pinRef.current = { x: (rect[0] + rect[2]) / 2, y: (rect[1] + rect[3]) / 2, sceneY };
    }
    scheduleDraw();
  }, [cuttingPlane, depth, job, scheduleDraw]);

  // The Slice Model form's picked slice: red up to its height, green above.
  const sliceBand = useSliceViewStore((state) => state.band);
  const slicing = useSliceViewStore((state) => state.active);
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    renderer.setSliceBand(
      sliceBand === null || isRotary(job)
        ? null
        : { low: -depth + sliceBand.low, high: -depth + sliceBand.high },
    );
    scheduleDraw();
  }, [sliceBand, depth, job, scheduleDraw]);

  // Transform handles follow the component while no handle is dragged, and
  // go away when the selection moves on, sculpting starts or a toolpath
  // preview shows.
  const selectedId = useModelStore((state) => state.selectedId);
  const components = useModelStore((state) => state.components);
  useEffect(() => {
    if (transformId !== null && (transformId !== selectedId || sculpting || preview !== null)) {
      setTransformId(null);
      setActive(null);
    }
  }, [preview, sculpting, selectedId, setActive, transformId]);
  useEffect(() => {
    if (handleDragRef.current !== null) {
      return;
    }
    const component = components.find((c) => c.id === transformId);
    transformRef.current =
      component === undefined ? null : frameFor(component, -depth, transformRef.current);
    scheduleDraw();
  }, [components, depth, scheduleDraw, transformId]);
  useEffect(() => {
    if (transformId === null) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (endsTransform(event, handleDragRef.current !== null)) {
        setTransformId(null);
        setActive(null);
        transformRef.current = null;
      }
    };
    const hint = strings.model.transform3d.hint;
    useUiStore.getState().setStatusHint(hint);
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      if (useUiStore.getState().statusHint === hint) {
        useUiStore.getState().setStatusHint(null);
      }
    };
  }, [setActive, transformId]);

  // While the pointer is over the model, the status bar says how to drag a
  // component.
  useEffect(() => {
    if (!hovering || !hasComponents || sculpting || preview !== null || transformId !== null) {
      return;
    }
    const hint = strings.model.moveHint(MOVE_KEY);
    const ui = useUiStore.getState();
    ui.setStatusHint(hint);
    return () => {
      if (useUiStore.getState().statusHint === hint) {
        useUiStore.getState().setStatusHint(null);
      }
    };
  }, [hasComponents, hovering, preview, sculpting, transformId]);

  useEffect(() => {
    if (!sculpting) {
      rendererRef.current?.setBrush(null, 0);
      strokeRef.current = null;
      scheduleDraw();
    }
  }, [sculpting, scheduleDraw]);

  // While sculpting, F toggles Show Visible Model instead of its usual
  // zoom, so it is caught before the shortcut table sees it.
  useEffect(() => {
    if (!sculpting) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (modalDialogOpen()) {
        return;
      }
      const key = event.key.toLowerCase();
      if (
        (key === VERTICAL_KEY || key === HORIZONTAL_KEY) &&
        !event.ctrlKey &&
        !event.metaKey &&
        !typingIn(event.target)
      ) {
        constrainRef.current = key === VERTICAL_KEY ? 'v' : 'h';
        event.preventDefault();
        event.stopImmediatePropagation();
        return;
      }
      if (
        event.key.toLowerCase() === ROTATE_BRUSH_KEY &&
        !event.ctrlKey &&
        !event.metaKey &&
        !typingIn(event.target)
      ) {
        rotateKeyRef.current = true;
        event.preventDefault();
        event.stopImmediatePropagation();
        return;
      }
      if (
        event.key.toLowerCase() === SHOW_VISIBLE_KEY &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.altKey &&
        !typingIn(event.target)
      ) {
        event.preventDefault();
        event.stopImmediatePropagation();
        const sculpt = useSculptStore.getState();
        sculpt.setShowVisible(!sculpt.showVisible);
      }
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() === ROTATE_BRUSH_KEY) {
        rotateKeyRef.current = false;
      }
      if ([VERTICAL_KEY, HORIZONTAL_KEY].includes(event.key.toLowerCase())) {
        constrainRef.current = null;
      }
    };
    const onBlur = () => {
      rotateKeyRef.current = false;
      constrainRef.current = null;
    };
    window.addEventListener('keydown', onKeyDown, { capture: true });
    window.addEventListener('keyup', onKeyUp);
    window.addEventListener('blur', onBlur);
    return () => {
      window.removeEventListener('keydown', onKeyDown, { capture: true });
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', onBlur);
      rotateKeyRef.current = false;
    };
  }, [sculpting]);

  // Twiddle: holding the key with the pointer over the view turns it slowly
  // for as long as the key is down.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (startsTwiddle(event, hoverRef.current)) {
        event.preventDefault();
        setTwiddling(true);
      }
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() === TWIDDLE_KEY) {
        setTwiddling(false);
      }
    };
    const onBlur = () => setTwiddling(false);
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    window.addEventListener('blur', onBlur);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', onBlur);
    };
  }, []);

  useEffect(() => {
    if (!twiddling && !twiddleOn) {
      return;
    }
    stopGlide();
    let last = performance.now();
    let frame = requestAnimationFrame(function turn(now: number) {
      rendererRef.current?.spin(((now - last) / 1000) * TWIDDLE_SPEED);
      last = now;
      scheduleDraw();
      frame = requestAnimationFrame(turn);
    });
    return () => cancelAnimationFrame(frame);
  }, [scheduleDraw, stopGlide, twiddling, twiddleOn]);

  const ndcOf = (event: ReactMouseEvent<HTMLCanvasElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return {
      x: ((event.clientX - rect.left) / rect.width) * 2 - 1,
      y: -(((event.clientY - rect.top) / rect.height) * 2 - 1),
    };
  };

  /** The model surface under the pointer, when the view shows the model. */
  const surfaceUnder = (event: ReactMouseEvent<HTMLCanvasElement>) => {
    const renderer = rendererRef.current;
    if (renderer === null || preview !== null) {
      return null;
    }
    const ndc = ndcOf(event);
    return renderer.pick(ndc.x, ndc.y);
  };

  /** Ctrl or Cmd drag on a component moves it; answers whether one was hit. */
  const startMove = (event: ReactPointerEvent<HTMLCanvasElement>): boolean => {
    const at = surfaceUnder(event);
    if (at === null) {
      return false;
    }
    const model = useModelStore.getState();
    const hit = componentAt(model.components, at.x, at.y);
    if (hit === null || hit.bbox === undefined) {
      return false;
    }
    model.select(hit.id);
    event.currentTarget.setPointerCapture(event.pointerId);
    moveRef.current = {
      pointerId: event.pointerId,
      id: hit.id,
      bbox: hit.bbox,
      planeY: at.scene.y,
      start: { x: at.x, y: at.y },
      offset: { x: 0, y: 0 },
    };
    rendererRef.current?.setOutline(hit.bbox, at.scene.y);
    scheduleDraw();
    return true;
  };

  const showBrush = (event: ReactPointerEvent<HTMLCanvasElement>) =>
    showBrushAt(event.currentTarget, event.clientX, event.clientY);

  /** The brush at a screen point (for V and H, not always the pointer). */
  const showBrushAt = (canvas: HTMLCanvasElement, clientX: number, clientY: number) => {
    const { brush, custom } = useSculptStore.getState();
    const renderer = rendererRef.current;
    const rect = canvas.getBoundingClientRect();
    const at =
      renderer === null || preview !== null
        ? null
        : renderer.pick(
            ((clientX - rect.left) / rect.width) * 2 - 1,
            -(((clientY - rect.top) / rect.height) * 2 - 1),
          );
    const last = brushTrailRef.current;
    if (at !== null && (last === null || Math.hypot(at.x - last.x, at.y - last.y) > 0.5)) {
      brushHeadingRef.current =
        last === null ? brushHeadingRef.current : Math.atan2(at.y - last.y, at.x - last.x);
      brushTrailRef.current = { x: at.x, y: at.y };
    }
    rendererRef.current?.setBrush(
      at,
      brush === null ? 0 : brush.diameter / 2,
      brushArrow(custom, brushHeadingRef.current),
    );
    scheduleDraw();
    return at;
  };

  const onPointerDown = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    stopGlide();
    rightClickRef.current =
      event.button === RIGHT_BUTTON
        ? { pointerId: event.pointerId, x: event.clientX, y: event.clientY }
        : null;
    clickRef.current =
      event.button === LEFT_BUTTON && !event.ctrlKey && !event.metaKey && !event.altKey
        ? { pointerId: event.pointerId, x: event.clientX, y: event.clientY }
        : null;
    if (
      event.button === LEFT_BUTTON &&
      (event.ctrlKey || event.metaKey) &&
      !sculpting &&
      !pieces.active &&
      startMove(event)
    ) {
      dragRef.current = {
        pointerId: event.pointerId,
        x: event.clientX,
        y: event.clientY,
        mode: 'move',
      };
      return;
    }
    const brush = useSculptStore.getState().brush;
    const component = useModelStore.getState().selectedId;
    const canSculpt =
      sculpting &&
      brush !== null &&
      component !== null &&
      event.button === LEFT_BUTTON &&
      !event.altKey;
    if (canSculpt) {
      const at = showBrush(event);
      if (at === null) {
        return;
      }
      event.currentTarget.setPointerCapture(event.pointerId);
      const pressure = penPressure(event);
      strokeRef.current = {
        pointerId: event.pointerId,
        points: [{ x: at.x, y: at.y }],
        pressures: pressure === null ? null : [pressure],
        erase: event.shiftKey,
        client: { x: event.clientX, y: event.clientY },
      };
      dragRef.current = {
        pointerId: event.pointerId,
        x: event.clientX,
        y: event.clientY,
        mode: 'sculpt',
      };
      return;
    }
    const mode = sculpting
      ? sculptDragFor(event.button, event.altKey)
      : dragFor(event.button, event.shiftKey, navigation);
    if (mode === null) {
      return;
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = { pointerId: event.pointerId, x: event.clientX, y: event.clientY, mode };
  };

  const onPointerMove = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    const drag = dragRef.current;
    const renderer = rendererRef.current;
    const moving = moveRef.current;
    if (moving !== null && renderer !== null && moving.pointerId === event.pointerId) {
      const ndc = ndcOf(event);
      const at = renderer.pickPlane(ndc.x, ndc.y, moving.planeY);
      if (at !== null) {
        moving.offset = { x: at.x - moving.start.x, y: at.y - moving.start.y };
        const [minX, minY, maxX, maxY] = moving.bbox;
        const { x: dx, y: dy } = moving.offset;
        renderer.setOutline([minX + dx, minY + dy, maxX + dx, maxY + dy], moving.planeY);
        scheduleDraw();
      }
      return;
    }
    if (pieces.active && drag === null) {
      // The Assembly View: light the face under the pointer while a mate
      // waits for one.
      const ndc = ndcOf(event);
      const rect = event.currentTarget.getBoundingClientRect();
      // A seam of the open furniture design lights before the pieces' faces.
      const onSeam = furniture.hoverAt(
        event.clientX - rect.left,
        event.clientY - rect.top,
        ndc.x,
        ndc.y,
      );
      pieces.hoverAt(onSeam ? null : ndc.x, ndc.y);
      faces.hoverAt(ndc.x, ndc.y);
    }
    if (sculpting && (drag === null || drag.mode === 'sculpt')) {
      const held = strokeRef.current;
      const straight = constrainRef.current;
      const at =
        held !== null && straight !== null
          ? showBrushAt(
              event.currentTarget,
              straight === 'v' ? held.client.x : event.clientX,
              straight === 'h' ? held.client.y : event.clientY,
            )
          : showBrush(event);
      const stroke = strokeRef.current;
      const brush = useSculptStore.getState().brush;
      if (
        at !== null &&
        stroke !== null &&
        brush !== null &&
        stroke.pointerId === event.pointerId
      ) {
        const last = stroke.points[stroke.points.length - 1];
        const gap = last === undefined ? Infinity : Math.hypot(at.x - last.x, at.y - last.y);
        if (gap >= brush.diameter * STROKE_SPACING) {
          stroke.points.push({ x: at.x, y: at.y });
          stroke.pressures?.push(penPressure(event) ?? 1);
        }
      }
      return;
    }
    if (drag === null || renderer === null || drag.pointerId !== event.pointerId) {
      return;
    }
    const deltaX = event.clientX - drag.x;
    const deltaY = event.clientY - drag.y;
    dragRef.current = { ...drag, x: event.clientX, y: event.clientY };
    if (drag.mode === 'pan') {
      renderer.pan(deltaX, deltaY);
    } else if (drag.mode === 'dolly') {
      renderer.dolly(dollyFactor(deltaY));
    } else {
      renderer.orbit(deltaX, deltaY);
    }
    scheduleDraw();
  };

  /** Whether the pointer going up ends a click (not a drag). */
  const isClick = (event: ReactPointerEvent<HTMLCanvasElement>): boolean => {
    const click = clickRef.current;
    return (
      click !== null &&
      click.pointerId === event.pointerId &&
      event.type === 'pointerup' &&
      Math.hypot(event.clientX - click.x, event.clientY - click.y) <= CLICK_SLOP
    );
  };

  /** Set Anchor: a click on the model is the next anchor point. */
  const clickedAnchor = (event: ReactPointerEvent<HTMLCanvasElement>): boolean => {
    const model = useModelStore.getState();
    const pick = model.anchorPick;
    if (pick === null || pick.points.length >= 2 || !isClick(event)) {
      return false;
    }
    clickRef.current = null;
    const at = surfaceUnder(event);
    if (at !== null) {
      model.addAnchorPoint({ x: at.x, y: at.y });
    }
    return true;
  };

  /** A click on the selected component shows its transform handles. */
  const clickedSelected = (event: ReactPointerEvent<HTMLCanvasElement>): boolean => {
    const clicked = isClick(event);
    clickRef.current = null;
    if (!clicked || sculpting) {
      return false;
    }
    const at = surfaceUnder(event);
    const model = useModelStore.getState();
    const hit = at === null ? null : componentAt(model.components, at.x, at.y);
    return hit !== null && hit.id === model.selectedId;
  };

  const endDrag = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    // A right click (not a pan) on a furniture panel's face opens its menu.
    const right = rightClickRef.current;
    rightClickRef.current = null;
    if (
      pieces.active &&
      right !== null &&
      right.pointerId === event.pointerId &&
      event.type === 'pointerup' &&
      Math.hypot(event.clientX - right.x, event.clientY - right.y) <= CLICK_SLOP
    ) {
      const ndc = ndcOf(event);
      faces.contextAt(event.clientX, event.clientY, ndc.x, ndc.y);
    }
    if (pieces.active) {
      // The Assembly View: a click picks the piece under it.
      if (isClick(event)) {
        const ndc = ndcOf(event);
        const rect = event.currentTarget.getBoundingClientRect();
        // A click on a seam opens its joints instead of picking a piece.
        const x = event.clientX - rect.left;
        const y = event.clientY - rect.top;
        if (!furniture.clickAt(x, y, ndc.x, ndc.y)) {
          pieces.pickAt(ndc.x, ndc.y);
        }
      }
      clickRef.current = null;
    } else if (!clickedAnchor(event) && clickedSelected(event)) {
      setTransformId(useModelStore.getState().selectedId);
    }
    if (dragRef.current?.pointerId === event.pointerId) {
      event.currentTarget.releasePointerCapture(event.pointerId);
      dragRef.current = null;
    }
    const moving = moveRef.current;
    if (moving !== null && moving.pointerId === event.pointerId) {
      moveRef.current = null;
      rendererRef.current?.setOutline(null, 0);
      scheduleDraw();
      const { x, y } = moving.offset;
      if (event.type === 'pointerup' && Math.hypot(x, y) > 1e-3) {
        void useModelStore.getState().transformComponent(moving.id, {
          offset: { x, y },
          scaleX: 1,
          scaleY: 1,
          rotateDeg: 0,
        });
      }
      return;
    }
    const stroke = strokeRef.current;
    strokeRef.current = null;
    const brush = useSculptStore.getState().brush;
    const component = useModelStore.getState().selectedId;
    if (
      event.type === 'pointerup' &&
      stroke !== null &&
      stroke.pointerId === event.pointerId &&
      brush !== null &&
      component !== null
    ) {
      void useModelStore
        .getState()
        .sculpt(
          component,
          stroke.points,
          strokeBrush(brush, stroke.erase),
          useSculptStore.getState().custom,
          stroke.pressures ?? undefined,
        );
    }
  };

  /** Where the pointer meets the handles' plane, in job XY. */
  const onHandlePlane = (event: ReactPointerEvent<SVGElement>, planeY: number): XY | null => {
    const canvas = canvasRef.current;
    const renderer = rendererRef.current;
    if (canvas === null || renderer === null) {
      return null;
    }
    const rect = canvas.getBoundingClientRect();
    const x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
    const y = -(((event.clientY - rect.top) / rect.height) * 2 - 1);
    const at = renderer.pickPlane(x, y, planeY);
    return at === null ? null : { x: at.x, y: at.y };
  };

  /** Pointer position over the canvas, CSS pixels. */
  const screenOf = (event: ReactPointerEvent<SVGElement>): XY => {
    const rect = canvasRef.current?.getBoundingClientRect();
    return { x: event.clientX - (rect?.left ?? 0), y: event.clientY - (rect?.top ?? 0) };
  };

  /** The canvas points of the height post's foot and one millimetre up. */
  const postAxis = (frame: TransformFrame): { foot: XY; up: XY } | null => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return null;
    }
    const { x, y } = applyParams(frame.bbox, frame.params, postOf(frame.bbox));
    const foot = renderer.project(x, y, frame.zeroY);
    const up = renderer.project(x, y, frame.zeroY + 1);
    return foot === null || up === null ? null : { foot, up };
  };

  /** The canvas points under the profile angle handle and one millimetre up. */
  const profileAxis = (frame: TransformFrame): { foot: XY; up: XY } | null => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return null;
    }
    const { x, y } = applyParams(frame.bbox, frame.params, profilePointOf(frame.bbox));
    const sceneY = frame.zeroY + profileHeightOf(frame.heights);
    const foot = renderer.project(x, y, sceneY);
    const up = renderer.project(x, y, sceneY + 1);
    return foot === null || up === null ? null : { foot, up };
  };

  /** Makes the shape again with a new edge angle, replacing the step made
   * at `replace`; answers the new revision. */
  const reshape = async (id: number, angle: number, replace?: number): Promise<number> => {
    const component = useModelStore.getState().components.find((c) => c.id === id);
    const args =
      component === undefined
        ? null
        : profileArgs(component, angle, useDocumentStore.getState().job.units);
    if (args === null) {
      throw new Error(strings.model.transform3d.badValue);
    }
    return (await api.model.createShape({ ...args, replace })).revision;
  };

  const onHandleDown = (handle: AnyHandle, event: ReactPointerEvent<SVGElement>) => {
    const frame = transformRef.current;
    if (event.button !== LEFT_BUTTON || frame === null || handleDragRef.current !== null) {
      return;
    }
    event.stopPropagation();
    const id = frame.id;
    const screen = screenOf(event);
    if (isProfileHandle(handle)) {
      const angle = frame.profile;
      if (angle === null) {
        return;
      }
      event.currentTarget.setPointerCapture(event.pointerId);
      setActive(handle);
      handleDragRef.current = {
        type: 'profile',
        pointerId: event.pointerId,
        screen,
        angle,
        live: new LiveTransform<number>(
          (now, replace) => reshape(id, now, replace),
          refreshMirrors,
          showError,
          (now) => now === angle,
        ),
      };
      return;
    }
    if (isHeightHandle(handle)) {
      const heights = frame.heights;
      event.currentTarget.setPointerCapture(event.pointerId);
      setActive(handle);
      handleDragRef.current = {
        type: 'height',
        pointerId: event.pointerId,
        kind: handle.kind,
        screen,
        heights,
        live: new LiveTransform<Heights>(
          async (now, replace) =>
            (await api.model.tree.props([id], heightPatch(heights, now), replace)).revision,
          refreshMirrors,
          showError,
          (now) => sameHeights(now, heights),
        ),
      };
      return;
    }
    const start = onHandlePlane(event, frame.planeY);
    if (start === null) {
      return;
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    if (handle.kind === 'move' && frame.centreMode) {
      const pivot = frame.pivot ?? centreOf(frame.bbox);
      handleDragRef.current = { type: 'pivot', pointerId: event.pointerId, start, screen, pivot };
      return;
    }
    // A second click on the middle switches it to the rotation centre.
    const second = handle.kind === 'move' && activeRef.current?.kind === 'move';
    setActive(handle);
    handleDragRef.current = {
      type: 'box',
      pointerId: event.pointerId,
      handle,
      second,
      start,
      screen,
      live: new LiveTransform<ComponentTransformParams>(
        async (params, replace) => (await api.model.transform(id, params, replace)).revision,
        refreshMirrors,
        showError,
        isIdentity,
      ),
    };
  };

  const onHandleMove = (event: ReactPointerEvent<SVGElement>) => {
    const drag = handleDragRef.current;
    const frame = transformRef.current;
    if (drag === null || frame === null || drag.pointerId !== event.pointerId) {
      return;
    }
    if (drag.type === 'profile') {
      const axis = profileAxis(frame);
      if (axis === null) {
        return;
      }
      const dz = riseAlong(axis.foot, axis.up, drag.screen, screenOf(event));
      frame.profile = dragAngle(drag.angle, dz, frame.bbox);
      scheduleDraw();
      drag.live.update(frame.profile);
      return;
    }
    if (drag.type === 'height') {
      const axis = postAxis(frame);
      if (axis === null) {
        return;
      }
      const dz = riseAlong(axis.foot, axis.up, drag.screen, screenOf(event));
      frame.heights = dragHeights(drag.kind, drag.heights, dz);
      scheduleDraw();
      drag.live.update(frame.heights);
      return;
    }
    const now = onHandlePlane(event, frame.planeY);
    if (now === null) {
      return;
    }
    if (drag.type === 'pivot') {
      frame.pivot = {
        x: drag.pivot.x + now.x - drag.start.x,
        y: drag.pivot.y + now.y - drag.start.y,
      };
      scheduleDraw();
      return;
    }
    const params = dragParams(
      drag.handle,
      frame.bbox,
      drag.start,
      now,
      { alt: event.altKey, shift: event.shiftKey },
      frame.pivot,
    );
    frame.params = params;
    scheduleDraw();
    drag.live.update(params);
  };

  /** Puts the handles on the component as it now stands. */
  const settleFrame = () => {
    const frame = transformRef.current;
    const component = useModelStore.getState().components.find((c) => c.id === frame?.id);
    if (frame !== null && component !== undefined) {
      const pivot = pivotAfter(frame);
      const next = frameFor(component, frame.zeroY, { ...frame, pivot });
      transformRef.current = next;
    }
    scheduleDraw();
  };

  const onHandleUp = (event: ReactPointerEvent<SVGElement>) => {
    const drag = handleDragRef.current;
    const frame = transformRef.current;
    if (drag === null || drag.pointerId !== event.pointerId) {
      return;
    }
    event.currentTarget.releasePointerCapture(event.pointerId);
    const at = screenOf(event);
    const clicked = Math.hypot(at.x - drag.screen.x, at.y - drag.screen.y) <= CLICK_SLOP;
    if (drag.type === 'pivot') {
      handleDragRef.current = null;
      // A click on the rotation centre gives the middle back to moving.
      if (clicked && frame !== null) {
        frame.centreMode = false;
      }
      scheduleDraw();
      return;
    }
    const second = drag.type === 'box' && drag.second && clicked;
    void drag.live.finish().then(() => {
      handleDragRef.current = null;
      if (second && !drag.live.applied && frame !== null) {
        frame.centreMode = true;
        frame.pivot = frame.pivot ?? centreOf(frame.bbox);
      }
      settleFrame();
    });
  };

  const onHandleDoubleClick = (handle: AnyHandle) => {
    const frame = transformRef.current;
    if (handle.kind !== 'base' || frame === null || frame.heights.base === 0) {
      return;
    }
    void api.model.tree
      .props([frame.id], { baseHeight: 0 })
      .then(refreshMirrors)
      .then(settleFrame)
      .catch(showError);
  };

  /** Typed values from the value box, applied to the component. */
  const applyValues = (values: TypedValues) => {
    const frame = transformRef.current;
    const active = activeRef.current;
    if (frame === null || active === null || handleDragRef.current !== null) {
      return;
    }
    if (isProfileHandle(active)) {
      const angle = typedAngle(values.profile);
      if (angle === null) {
        showError(new Error(strings.model.transform3d.badValue));
        return;
      }
      void reshape(frame.id, angle).then(refreshMirrors).then(settleFrame).catch(showError);
      return;
    }
    if (isHeightHandle(active)) {
      const heights = typedHeightValues(active.kind, frame.heights, values);
      if (heights === null) {
        showError(new Error(strings.model.transform3d.badValue));
        return;
      }
      void api.model.tree
        .props([frame.id], heightPatch(frame.heights, heights))
        .then(refreshMirrors)
        .then(settleFrame)
        .catch(showError);
      return;
    }
    const result = typedTransform(active, frame.bbox, frame.pivot, frame.centreMode, values);
    if (result === null) {
      showError(new Error(strings.model.transform3d.badValue));
      return;
    }
    if (result.type === 'pivot') {
      frame.pivot = result.pivot;
      scheduleDraw();
      return;
    }
    if (isIdentity(result.params)) {
      return;
    }
    frame.params = result.params;
    void api.model
      .transform(frame.id, result.params)
      .then(refreshMirrors)
      .then(settleFrame)
      .catch((error: unknown) => {
        frame.params = NO_TRANSFORM;
        showError(error);
        settleFrame();
      });
  };

  const onPinDown = (event: ReactPointerEvent<SVGElement>) => {
    const pin = pinRef.current;
    const renderer = rendererRef.current;
    const plane = useModelStore.getState().cuttingPlane;
    if (event.button !== LEFT_BUTTON || pin === null || renderer === null || plane === null) {
      return;
    }
    const at = renderer.project(pin.x, pin.y, pin.sceneY);
    const above = renderer.project(pin.x, pin.y, pin.sceneY + 1);
    if (at === null || above === null || Math.abs(above.y - at.y) < 1e-6) {
      return;
    }
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    pinDragRef.current = {
      pointerId: event.pointerId,
      startY: event.clientY,
      height: plane.height,
      pixelsPerMm: above.y - at.y,
    };
  };

  const onPinMove = (event: ReactPointerEvent<SVGElement>) => {
    const drag = pinDragRef.current;
    if (drag === null || drag.pointerId !== event.pointerId) {
      return;
    }
    const height = drag.height + (event.clientY - drag.startY) / drag.pixelsPerMm;
    useModelStore.getState().setCuttingPlane({ height, from: 'view' });
  };

  const onPinUp = (event: ReactPointerEvent<SVGElement>) => {
    if (pinDragRef.current?.pointerId === event.pointerId) {
      event.currentTarget.releasePointerCapture(event.pointerId);
      pinDragRef.current = null;
    }
  };

  /** Double click selects the component under the pointer; with Shift it
   * joins (or leaves) the selection. Nothing under it clears it. */
  const onDoubleClick = (event: ReactMouseEvent<HTMLCanvasElement>) => {
    if (sculpting) {
      return;
    }
    // A toolpath preview is up: double clicking a piece cut free removes it.
    if (preview !== null) {
      const ndc = ndcOf(event);
      const hit = rendererRef.current?.pick(ndc.x, ndc.y) ?? null;
      if (hit !== null && !modalDialogOpen()) {
        void usePreviewStore.getState().removeWaste(hit.x, hit.y);
      }
      return;
    }
    const at = surfaceUnder(event);
    // With the Slice Model form open it edits the slice heights instead:
    // the picked one moves to the height clicked, Shift adds one there.
    const slice = useSliceViewStore.getState();
    if (slice.active) {
      if (at !== null && !isRotary(job) && !modalDialogOpen()) {
        slice.requestEdit(doubleClickEdit(at.scene.y + depth, event.shiftKey));
      }
      return;
    }
    const model = useModelStore.getState();
    const hit = at === null ? null : componentAt(model.components, at.x, at.y);
    if (hit === null) {
      if (!event.shiftKey) {
        model.select(null);
        model.setPicked([]);
      }
      return;
    }
    if (event.shiftKey) {
      const current = pickedComponents(model);
      const next = current.includes(hit.id)
        ? current.filter((id) => id !== hit.id)
        : [...current, hit.id];
      model.setPicked(next);
      model.select(next.includes(hit.id) ? hit.id : (next[next.length - 1] ?? null));
      return;
    }
    model.setPicked([hit.id]);
    model.select(hit.id);
  };

  const hideBrush = () => {
    pieces.hoverAt(null);
    furniture.hoverAt(null);
    faces.hoverAt(null);
    hoverRef.current = false;
    setHovering(false);
    if (strokeRef.current === null) {
      rendererRef.current?.setBrush(null, 0);
      scheduleDraw();
    }
  };

  const activeAt =
    active === null || shown === null
      ? undefined
      : shown.handles.find((h) => handleKey(h.handle) === handleKey(active))?.at;
  const valueBox =
    transformId === null || shown === null || shown.fields === null || activeAt === undefined
      ? null
      : { fields: shown.fields, at: activeAt };

  // Clips dragged from the Clipart tab land on the model under the pointer.
  const clipDropRef = useRef<DropResolver>((x, y) => {
    const renderer = rendererRef.current;
    const canvas = canvasRef.current;
    if (renderer === null || canvas === null) {
      return null;
    }
    return clipDropPoint(renderer, canvas.getBoundingClientRect(), x, y);
  });
  useDropTarget(canvasRef, clipDropRef);

  return (
    <div className="view-3d" ref={containerRef}>
      {available ? <View3DToolbar onFit={zoomToFit} onPreset={showPreset} /> : null}
      <div className="view-3d__stage">
        <canvas
          aria-label={strings.views.canvas3d}
          className={
            sculpting
              ? 'view-3d__canvas view-3d__canvas--sculpt'
              : anchoring
                ? 'view-3d__canvas view-3d__canvas--anchor'
                : pieces.placing
                  ? 'view-3d__canvas view-3d__canvas--place'
                  : 'view-3d__canvas'
          }
          onContextMenu={(event) => event.preventDefault()}
          onDoubleClick={onDoubleClick}
          onPointerCancel={endDrag}
          onPointerEnter={() => {
            hoverRef.current = true;
            setHovering(true);
          }}
          onPointerLeave={hideBrush}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={endDrag}
          ref={canvasRef}
          role="img"
        />
        {shown === null || transformId === null ? null : (
          <TransformHandles
            active={active}
            centreMode={shown.centreMode}
            handles={shown.handles}
            onHandleDoubleClick={onHandleDoubleClick}
            onHandleDown={onHandleDown}
            onHandleMove={onHandleMove}
            onHandleUp={onHandleUp}
            outline={shown.outline}
            post={shown.post}
            stalk={shown.stalk}
          />
        )}
        {planeHandles.shown === null ? null : (
          <PlaneHandles
            active={planeHandles.active}
            onDown={planeHandles.onDown}
            onMove={planeHandles.onMove}
            onUp={planeHandles.onUp}
            shown={planeHandles.shown}
          />
        )}
        {rings.shown === null ? null : (
          <RotateRings
            active={rings.active}
            ball={rings.shown.ball}
            cube={rings.shown.cube}
            field={rings.field}
            onAngle={rings.onAngle}
            onCloseField={rings.closeField}
            onDown={rings.onDown}
            onMove={rings.onMove}
            onUp={rings.onUp}
            rings={rings.shown.rings}
          />
        )}
        {pieces.shown === null ? null : (
          <PieceGizmo
            active={pieces.dragging}
            field={pieces.field}
            onCloseField={pieces.closeField}
            onDown={pieces.onDown}
            onMove={pieces.onMove}
            onUp={pieces.onUp}
            onValue={pieces.onValue}
            shown={pieces.shown}
            unit={unitSuffix(job.units)}
          />
        )}
        {furniture.active ? (
          <FurnitureOverlay
            arrows={furniture.arrows}
            labels={furniture.labels}
            seamLabel={furniture.seamLabel}
          />
        ) : null}
        <FaceMenu />
        {available ? (
          <UsageInfo
            mode={
              sculpting
                ? 'sculpt'
                : transformId !== null
                  ? 'transform'
                  : slicing && hasComponents
                    ? 'slice'
                    : hasComponents
                      ? 'model'
                      : 'navigate'
            }
            moveKey={MOVE_KEY}
          />
        ) : null}
        {valueBox === null ? null : (
          <HandleValueBox
            fields={valueBox.fields}
            key={JSON.stringify(valueBox.fields)}
            onApply={applyValues}
            onClose={() => setActive(null)}
            units={units}
            x={valueBox.at.x + VALUE_BOX_GAP}
            y={valueBox.at.y + VALUE_BOX_GAP}
          />
        )}
        {pinShown === null ? null : (
          <PlanePin
            onPointerDown={onPinDown}
            onPointerMove={onPinMove}
            onPointerUp={onPinUp}
            x={pinShown.x}
            y={pinShown.y}
          />
        )}
        {available && showCube ? (
          <div className="view-3d__cube">
            <ViewCube onPick={showPreset} phi={angles.phi} theta={angles.theta} />
          </div>
        ) : null}
        {volume === null ? null : (
          <p className="view-3d__volume" role="status">
            {volume}
          </p>
        )}
        {available ? null : <p className="view-3d__message">{strings.views.webglMissing}</p>}
      </div>
    </div>
  );
}
