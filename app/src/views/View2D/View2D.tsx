import { useCallback, useEffect, useMemo, useRef, type JSX } from 'react';

import { api } from '../../bridge/api';
import type { Path } from '../../bridge/document';
import { moveVisibility } from '../../lib/moveGroups';
import { watchPixelRatio } from '../../lib/pixelRatio';
import { isDoubleSided } from '../../lib/sides';
import { strings } from '../../lib/strings';
import { readCanvasPalette } from '../../lib/theme';
import { selectionFrame } from '../../lib/transformMath';
import { useDocumentStore } from '../../state/documentStore';
import { onSelectionTransformed } from '../../state/selectionTransforms';
import { useDrawStore } from '../../state/drawStore';
import { useModelStore } from '../../state/modelStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useTraceViewStore } from '../../state/traceViewStore';
import { useViewStore } from '../../state/viewStore';
import { DisplayCache } from './displayCache';
import { InteractionController } from './interaction';
import { View2DRenderer, type LayerStyle } from './renderer';
import { toolpathPaths } from './toolpathPaths';
import { centreAfterTransform } from './tools/handleModes';
import { useDropTarget, type DropResolver } from '../../panels/ClipartPanel/drop';
import { dropPoint } from '../../panels/ClipartPanel/model';
import { ContextMenu } from '../ContextMenu/ContextMenu';
import { NodeProperties } from '../NodeProperties/NodeProperties';
import { SnapBar } from '../SnapBar/SnapBar';

import './View2D.scss';

/** Chord tolerance requested from the kernel, as a fraction of a pixel. */
const DISPLAY_TOLERANCE_PX = 0.25;

export function View2D(): JSX.Element {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const rendererRef = useRef<View2DRenderer | null>(null);
  const controllerRef = useRef<InteractionController | null>(null);
  const cacheRef = useRef<DisplayCache>(new DisplayCache());
  const frameRef = useRef<number | null>(null);
  const fittedRef = useRef(false);
  const toleranceRef = useRef(0.05);

  const job = useDocumentStore((state) => state.job);
  const revision = useDocumentStore((state) => state.revision);
  const layers = useDocumentStore((state) => state.layers);
  const selection = useDocumentStore((state) => state.selection);
  const camera = useViewStore((state) => state.camera);
  const showGrid = useViewStore((state) => state.showGrid);
  const showRulers = useViewStore((state) => state.showRulers);
  const showBitmaps = useViewStore((state) => state.showBitmaps);
  const showComponents = useViewStore((state) => state.showComponentPreview);
  const composite = useModelStore((state) => state.composite);
  const gridSpacingMm = useViewStore((state) => state.gridSpacing);
  const gridStyle = usePrefsStore((state) => state.prefs.view2d.gridStyle);
  const sideShown = useDocumentStore((state) =>
    isDoubleSided(state.job) ? state.activeSide : null,
  );
  const topRuler = usePrefsStore((state) => state.prefs.window.topRulerColour);
  const bottomRuler = usePrefsStore((state) => state.prefs.window.bottomRulerColour);
  const rulerTint = sideShown === null ? null : sideShown === 'bottom' ? bottomRuler : topRuler;
  const units = useDocumentStore((state) => state.job.units);
  const allGuides = useViewStore((state) => state.guides);
  const showGuides = useViewStore((state) => state.showGuides);
  const guides = useMemo(() => (showGuides ? allGuides : []), [showGuides, allGuides]);
  const activeTool = useViewStore((state) => state.activeTool);
  const preview = useDrawStore((state) => state.preview);
  const overlays = useDrawStore((state) => state.overlays);
  const labels = useDrawStore((state) => state.labels);
  const toolpathMoves = useToolpathStore((state) => state.moves);
  const setViewport = useViewStore((state) => state.setViewport);
  const zoomToJob = useViewStore((state) => state.zoomToJob);

  const scheduleDraw = useCallback(() => {
    if (frameRef.current !== null) {
      return;
    }
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = null;
      rendererRef.current?.draw();
    });
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (canvas === null || container === null) {
      return;
    }
    const renderer = new View2DRenderer(canvas);
    renderer.update({ palette: readCanvasPalette(container) });
    rendererRef.current = renderer;
    const controller = new InteractionController(canvas, renderer, scheduleDraw);
    controllerRef.current = controller;
    controller.attach();

    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry === undefined) {
        return;
      }
      const { width, height } = entry.contentRect;
      renderer.resize(width, height, window.devicePixelRatio);
      setViewport({ width, height });
      if (!fittedRef.current && width > 0 && height > 0) {
        fittedRef.current = true;
        zoomToJob(useDocumentStore.getState().job);
      }
      scheduleDraw();
    });
    observer.observe(container);
    // Moving to a display with another pixel ratio redraws sharp.
    const stopRatio = watchPixelRatio((ratio) => {
      const { width, height } = container.getBoundingClientRect();
      renderer.resize(width, height, ratio);
      scheduleDraw();
    });

    const scheme = window.matchMedia('(prefers-color-scheme: dark)');
    const onSchemeChange = () => {
      renderer.update({ palette: readCanvasPalette(container) });
      scheduleDraw();
    };
    scheme.addEventListener('change', onSchemeChange);
    const stopTraceView = useTraceViewStore.subscribe(() => scheduleDraw());
    const stopPrefs = usePrefsStore.subscribe((state, previous) => {
      if (state.prefs !== previous.prefs) {
        onSchemeChange();
      }
    });

    return () => {
      stopRatio();
      observer.disconnect();
      scheme.removeEventListener('change', onSchemeChange);
      stopPrefs();
      stopTraceView();
      controller.detach();
      if (frameRef.current !== null) {
        cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
      controllerRef.current = null;
      rendererRef.current = null;
    };
  }, [scheduleDraw, setViewport, zoomToJob]);

  // Camera and view options.
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    renderer.update({
      job,
      camera,
      options: {
        showGrid,
        showRulers,
        showBitmaps,
        showComponents,
        gridStyle,
        gridSpacingMm,
        rulerTint,
        units,
      },
      guides,
    });
    scheduleDraw();
    // Refetch display geometry when the zoom moved far enough to matter.
    const wanted = DISPLAY_TOLERANCE_PX / camera.pxPerMm;
    if (wanted < toleranceRef.current / 2 || wanted > toleranceRef.current * 2) {
      toleranceRef.current = wanted;
      const cache = cacheRef.current;
      void cache.sync(useDocumentStore.getState().revision, wanted, true).then(() => {
        renderer.update({ entries: cache.list() });
        scheduleDraw();
      });
    }
  }, [
    job,
    camera,
    showGrid,
    showRulers,
    showBitmaps,
    showComponents,
    gridStyle,
    gridSpacingMm,
    rulerTint,
    units,
    guides,
    scheduleDraw,
  ]);

  // The shaded component preview follows the model.
  useEffect(() => {
    rendererRef.current?.setComposite(composite);
    scheduleDraw();
  }, [composite, scheduleDraw]);

  // Document content.
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    const layerStyles = new Map<number, LayerStyle>(
      layers.map((layer) => [
        layer.id,
        { colour: layer.colour, visible: layer.visible, keepOut: layer.keepOut === true },
      ]),
    );
    renderer.update({
      layers: layerStyles,
      selectionBbox: selection.bbox,
      selectionFrame: selectionFrame(selection),
    });
    if (selection.ids.length === 0) {
      renderer.update({ handleMode: 'none', rotationCentre: null });
    }
    const cache = cacheRef.current;
    void cache.sync(revision, toleranceRef.current).then(() => {
      renderer.update({ entries: cache.list() });
      scheduleDraw();
    });
    scheduleDraw();
  }, [revision, layers, selection, scheduleDraw]);

  useEffect(() => {
    controllerRef.current?.useTool(activeTool);
  }, [activeTool]);

  // The rotational anchor goes with the selection however it is moved,
  // turned, scaled or copied: by a drag, a form, a menu or a key.
  useEffect(
    () =>
      onSelectionTransformed((matrix) => {
        const renderer = rendererRef.current;
        if (renderer === null) {
          return;
        }
        renderer.update({
          rotationCentre: centreAfterTransform(renderer.state.rotationCentre, matrix),
        });
        scheduleDraw();
      }),
    [scheduleDraw],
  );

  // Entering Interactive Selection shows the handles on the current
  // selection; leaving it hides them.
  const transformMode = useViewStore((state) => state.transformMode);
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null || renderer.state.selectionFrame === null) {
      return;
    }
    renderer.update({ handleMode: transformMode ? 'scale' : 'none' });
    scheduleDraw();
  }, [transformMode, scheduleDraw]);

  useEffect(() => {
    rendererRef.current?.update({ previewPaths: preview });
    scheduleDraw();
  }, [preview, scheduleDraw]);

  useEffect(() => {
    rendererRef.current?.update({ previewOverlays: overlays });
    scheduleDraw();
  }, [overlays, scheduleDraw]);

  useEffect(() => {
    rendererRef.current?.update({ previewLabels: labels });
    scheduleDraw();
  }, [labels, scheduleDraw]);

  // Toolpaths > Toolpath Drawing decides what of the toolpaths shows.
  const toolpathPrefs = usePrefsStore((state) => state.prefs.toolpaths);
  const toolpaths2d = useMemo(
    () =>
      toolpathMoves === null || !toolpathPrefs.draw2dPreviews
        ? null
        : toolpathPaths(toolpathMoves, moveVisibility(toolpathPrefs), toolpathPrefs.solidPreviews),
    [toolpathMoves, toolpathPrefs],
  );

  useEffect(() => {
    rendererRef.current?.update({ toolpaths: toolpaths2d });
    scheduleDraw();
  }, [toolpaths2d, scheduleDraw]);

  // Multi Sided View: the other side's vectors, turned over, drawn faint.
  const multiSided = usePrefsStore((state) => state.prefs.view3d.multiSided);
  const doubleSided = useDocumentStore((state) => isDoubleSided(state.job));
  const activeSide = useDocumentStore((state) => state.activeSide);
  const docRevision = useDocumentStore((state) => state.revision);
  useEffect(() => {
    let cancelled = false;
    const show = (paths: readonly Path[]) => {
      if (!cancelled) {
        rendererRef.current?.update({ otherSide: paths });
        scheduleDraw();
      }
    };
    if (!multiSided || !doubleSided) {
      show([]);
      return;
    }
    void api.job
      .otherSidePaths()
      .then((reply) => show(reply.paths))
      .catch(() => show([]));
    return () => {
      cancelled = true;
    };
  }, [activeSide, docRevision, doubleSided, multiSided, scheduleDraw]);

  // Toolpath Tiling's tiles, while it is on.
  const tiling = useToolpathStore((state) => state.tiling);
  useEffect(() => {
    rendererRef.current?.update({
      tiles:
        tiling === null || !tiling.enabled ? null : { tiles: tiling.tiles, active: tiling.active },
    });
    scheduleDraw();
  }, [tiling, scheduleDraw]);

  // Clips dragged from the Clipart tab land centred where they are let go.
  const clipDropRef = useRef<DropResolver>((x, y) => {
    const surface = containerRef.current;
    if (surface === null) {
      return null;
    }
    const { camera, viewport } = useViewStore.getState();
    return dropPoint(camera, viewport, surface.getBoundingClientRect(), { x, y });
  });
  useDropTarget(containerRef, clipDropRef);

  return (
    <div className="view-2d">
      <SnapBar />
      <div className="view-2d__surface" ref={containerRef}>
        <canvas
          aria-label={strings.views.canvas2d}
          className="view-2d__canvas"
          ref={canvasRef}
          role="img"
          tabIndex={0}
        />
        <ContextMenu />
        <NodeProperties />
      </div>
    </div>
  );
}
