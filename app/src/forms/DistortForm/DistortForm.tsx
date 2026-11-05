import { useEffect, useMemo, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { DistortArgs, DistortMode, StoredDistortion } from '../../bridge/api';
import type { Path } from '../../bridge/document';
import {
  distortPreview,
  envelopeKeyEdit,
  envelopeOutline,
  type Envelope,
} from '../../lib/envelope';
import { strings } from '../../lib/formStrings';
import { rotationAbout, transformPath } from '../../lib/geometry';
import { useDrawController, usePreview } from '../../lib/useDrawController';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController } from '../../state/drawStore';
import { boxOf } from '../../state/componentSelection';
import { pickedComponents, useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import {
  curveArgs,
  distortArgs,
  distortTargetProblem,
  dragNode,
  isBent,
  modeFromSelection,
  nodeMarkers,
  pickNode,
  splitSelection,
  startEnvelope,
  storedEnvelope,
  targetBox,
  toFrame,
  type NodeRef,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './DistortForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { distortPicture } from '../HelperPicture/pictures/drawing';

const text = strings.transform.distort;

/** Pick distance for envelope nodes, in screen pixels. */
const PICK_PX = 8;
/** Node marker size, in screen pixels. */
const MARKER_PX = 7;
/** Chord tolerance of the preview, in screen pixels. */
const PREVIEW_PX = 0.5;

const MODES: ReadonlyArray<Choice<DistortMode>> = [
  { value: 'box', label: text.modeBox },
  { value: 'curve', label: text.modeCurve },
  { value: 'twoCurves', label: text.modeTwoCurves },
];

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Geometry of the selected vectors and text, groups opened up. */
function useSourcePaths(ids: readonly number[]): Path[] {
  const revision = useDocumentStore((store) => store.revision);
  const key = ids.join(',');
  const [result, setResult] = useState<{ key: string; paths: Path[] }>({ key: '', paths: [] });

  useEffect(() => {
    if (key === '') {
      return undefined;
    }
    let live = true;
    const collect = async (id: number): Promise<Path[]> => {
      const entity = await api.entities.get(id).catch(() => null);
      if (entity === null) {
        return [];
      }
      if (entity.kind === 'path') {
        return [entity.path];
      }
      if (entity.kind === 'text') {
        return entity.paths;
      }
      if (entity.kind === 'group') {
        const nested = await Promise.all(entity.children.map(collect));
        return nested.flat();
      }
      return [];
    };
    void Promise.all(key.split(',').map(Number).map(collect)).then((lists) => {
      if (live) {
        setResult({ key, paths: lists.flat() });
      }
    });
    return () => {
      live = false;
    };
  }, [key, revision]);

  return key === '' || result.key !== key ? [] : result.paths;
}

/** The distortion the objects still carry (editable until baked). */
function useStoredDistortion(ids: readonly number[]): StoredDistortion | null {
  const revision = useDocumentStore((store) => store.revision);
  const key = [...ids].sort((a, b) => a - b).join(',');
  const [result, setResult] = useState<{ key: string; stored: StoredDistortion | null }>({
    key: '',
    stored: null,
  });
  useEffect(() => {
    if (key === '') {
      return undefined;
    }
    let live = true;
    void api.ops
      .distortion(key.split(',').map(Number))
      .then((reply) => reply.distortion)
      .catch(() => null)
      .then((stored) => {
        if (live) {
          setResult({ key, stored });
        }
      });
    return () => {
      live = false;
    };
  }, [key, revision]);
  return result.key === key ? result.stored : null;
}

/** Paths in a frame turned `angle` degrees, drawn in job space. */
function fromFramePaths(paths: Path[], angle: number): Path[] {
  if (angle === 0) {
    return paths;
  }
  const m = rotationAbout(angle, 0, 0);
  return paths.map((path) => transformPath(path, m));
}

export function DistortForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const selection = useDocumentStore((store) => store.selection);
  // The components selected with the vectors bend with them.
  const componentKey = useModelStore((store) => pickedComponents(store).join(','));
  const componentIds = useMemo(
    () => (componentKey === '' ? [] : componentKey.split(',').map(Number)),
    [componentKey],
  );
  const components = useModelStore((store) => store.components);
  const pxPerMm = useViewStore((store) => store.camera.pxPerMm);
  // The mode the selection suggests when the form opens.
  const [mode, setMode] = useState<DistortMode>(() => {
    const doc = useDocumentStore.getState();
    const open = (id: number) =>
      doc.entities.some((e) => e.id === id && e.kind === 'path' && !e.closed);
    return modeFromSelection(doc.selection.ids, open);
  });
  const [rotated, setRotated] = useState(false);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  const { objects, curves } = splitSelection(selection.ids, mode);
  const stored = useStoredDistortion(objects);
  const frame = selection.frame;
  const canRotate = objects.length === 1 && frame !== undefined;
  const angle = stored !== null ? stored.angle : rotated && canRotate ? frame.angle : 0;
  const box =
    stored !== null
      ? stored.bbox
      : angle !== 0 && frame !== undefined
        ? frame.extent
        : targetBox(selection.bbox, boxOf(components, componentIds));
  const problem = distortTargetProblem(objects.length, componentIds.length);
  const missingCurves =
    mode !== 'box' && (objects.length + componentIds.length === 0 || curves.length === 0);

  // The envelope starts as the one the objects were bent with, else as a
  // plain box. A string key stands in for it, so a refresh that leaves it
  // alone keeps the nodes the user is dragging.
  const storedKey = stored !== null && stored.mode === 'box' ? JSON.stringify(stored) : '';
  const startKey = storedKey !== '' ? storedKey : JSON.stringify(box);
  const start = useMemo(
    () =>
      storedKey !== ''
        ? storedEnvelope(JSON.parse(storedKey) as StoredDistortion)
        : startEnvelope(JSON.parse(startKey) as typeof box),
    [storedKey, startKey],
  );
  const [edit, setEdit] = useState<{ start: Envelope | null; envelope: Envelope } | null>(null);
  const envelope =
    mode !== 'box' ? null : edit !== null && edit.start === start ? edit.envelope : start;
  const bent = envelope !== null && start !== null && isBent(envelope, start);

  const envelopeRef = useRef(envelope);
  const startRef = useRef(start);
  const dragRef = useRef<NodeRef | null>(null);
  // The node last picked, and the pointer, for the node editing keys.
  const pickedRef = useRef<NodeRef | null>(null);
  const pointerRef = useRef<{ x: number; y: number } | null>(null);
  const storedRef = useRef(stored !== null);
  const angleRef = useRef(angle);
  const applyRef = useRef<(envelope: Envelope) => void>(() => undefined);
  useEffect(() => {
    envelopeRef.current = envelope;
    startRef.current = start;
    angleRef.current = angle;
    storedRef.current = stored !== null;
  });

  const sources = useSourcePaths(objects);
  const preview = useMemo(() => {
    if (envelope === null) {
      return [];
    }
    const size = MARKER_PX / pxPerMm;
    const turn = rotationAbout(-angle, 0, 0);
    // Before the first bend the result is previewed here; once bent the
    // objects show it themselves.
    const shown =
      bent && stored === null
        ? distortPreview(
            sources.map((path) => (angle === 0 ? path : transformPath(path, turn))),
            envelope,
            PREVIEW_PX / pxPerMm,
          )
        : [];
    return fromFramePaths(
      [envelopeOutline(envelope), ...nodeMarkers(envelope, size), ...shown],
      angle,
    );
  }, [envelope, bent, stored, sources, pxPerMm, angle]);
  usePreview(preview);

  const run = (action: () => Promise<unknown>, done?: string) => {
    setBusy(true);
    setStatus(null);
    setFailure(null);
    action()
      .then(() => {
        if (done !== undefined) {
          setStatus(done);
        }
        setEdit(null);
      })
      .catch((error: unknown) => setFailure(message(error)))
      .finally(() => {
        void useDocumentStore
          .getState()
          .refresh()
          .then(() => useModelStore.getState().refresh())
          .finally(() => setBusy(false));
      });
  };

  const distortWith = (args: DistortArgs) =>
    run(async () => {
      const reply = await api.ops.distort(args);
      setStatus(text.applied(reply.ids.length + reply.componentIds.length));
    });

  // A box envelope applies as soon as a node is let go
  // bends while its envelope is node edited.
  useEffect(() => {
    applyRef.current = (bentEnvelope: Envelope) => {
      if (problem !== null || busy) {
        return;
      }
      distortWith(distortArgs(bentEnvelope, objects, componentIds, angle));
    };
  });

  const controller = useMemo<DrawController>(
    () => ({
      cursor: 'crosshair',
      pointerDown: (p) => {
        const current = envelopeRef.current;
        if (current === null) {
          return;
        }
        const tolerance = PICK_PX / useViewStore.getState().camera.pxPerMm;
        const at = toFrame({ x: p.rawX, y: p.rawY }, angleRef.current);
        dragRef.current = pickNode(current, at, tolerance);
        pickedRef.current = dragRef.current;
      },
      pointerMove: (p) => {
        pointerRef.current = { x: p.rawX, y: p.rawY };
        const node = dragRef.current;
        const current = envelopeRef.current;
        if (node === null || current === null) {
          return;
        }
        const moved = dragNode(current, node, toFrame({ x: p.x, y: p.y }, angleRef.current));
        envelopeRef.current = moved;
        setEdit({ start: startRef.current, envelope: moved });
      },
      pointerUp: () => {
        const dragged = dragRef.current !== null;
        dragRef.current = null;
        const current = envelopeRef.current;
        const first = startRef.current;
        if (dragged && current !== null && first !== null && isBent(current, first)) {
          applyRef.current(current);
        }
      },
      keyDown: (event) => {
        const current = envelopeRef.current;
        const first = startRef.current;
        if (current === null || first === null) {
          return false;
        }
        const tolerance = PICK_PX / useViewStore.getState().camera.pxPerMm;
        const pointer = pointerRef.current;
        const next = envelopeKeyEdit(
          current,
          event.key,
          pointer === null ? null : toFrame(pointer, angleRef.current),
          tolerance,
          pickedRef.current,
        );
        if (next === null) {
          return false;
        }
        pickedRef.current = null;
        envelopeRef.current = next;
        setEdit({ start: first, envelope: next });
        // A live distortion bends again at once; a fresh one on the next drag.
        if (storedRef.current || isBent(next, first)) {
          applyRef.current(next);
        }
        return true;
      },
      cancel: () => {
        dragRef.current = null;
      },
    }),
    [],
  );
  useDrawController(controller);

  const applyCurves = () => {
    if (mode === 'box' || missingCurves || problem !== null || busy) {
      return;
    }
    distortWith(curveArgs(mode, objects, curves, componentIds, angle));
  };

  const bake = () => run(() => api.ops.bakeDistortion(objects), text.baked);

  const bakeComponents = () =>
    run(async () => {
      await useModelStore.getState().bake(componentIds, text.bakedName);
    });

  const hint = mode === 'box' ? text.hint : mode === 'curve' ? text.hintCurve : text.hintTwoCurves;
  const bakeWhy = busy ? strings.why.busy : undefined;
  return (
    <form className="distort-form" onSubmit={(event) => event.preventDefault()}>
      <HelperRow picture={<HelperPicture picture={distortPicture(mode)} />}>
        <ChoiceField label={text.mode} onChange={setMode} options={MODES} value={mode} />
      </HelperRow>
      <p className="distort-form__hint">{hint}</p>
      {box === null && mode === 'box' ? (
        <p className="distort-form__notice" role="status">
          {text.needsSelection}
        </p>
      ) : null}
      {missingCurves ? (
        <p className="distort-form__notice" role="status">
          {mode === 'curve' ? text.needsCurve : text.needsTwoCurves}
        </p>
      ) : null}
      {problem !== null ? (
        <div className="distort-form__notice" role="status">
          <p className="distort-form__notice-text">{text[problem]}</p>
          {problem === 'several' ? (
            <button
              className="distort-form__button"
              disabled={bakeWhy !== undefined}
              onClick={bakeComponents}
              type="button"
              data-tip={whyTip(text.bakeComponents, bakeWhy)}
            >
              <ButtonFace icon="bakeComponents" label={text.bakeComponents} />
            </button>
          ) : null}
        </div>
      ) : null}
      <CheckField
        checked={angle !== 0}
        disabled={!canRotate || stored !== null}
        label={text.rotatedBounds}
        onChange={setRotated}
      />
      {stored !== null ? (
        <button
          className="distort-form__button"
          disabled={bakeWhy !== undefined}
          onClick={bake}
          type="button"
          data-tip={whyTip(text.bake, bakeWhy)}
        >
          <ButtonFace icon="bakeComponents" label={text.bake} />
        </button>
      ) : null}
      {failure !== null ? (
        <p className="distort-form__error" role="alert">
          {failure}
        </p>
      ) : null}
      {status !== null ? (
        <p className="distort-form__hint" role="status">
          {status}
        </p>
      ) : null}
      {mode === 'box' ? (
        <FormButtons onClose={closeForm} />
      ) : (
        <FormButtons
          onClose={closeForm}
          onPrimary={applyCurves}
          primaryWhy={firstWhy(
            [busy, strings.why.busy],
            [problem !== null, problem === null ? '' : text[problem]],
            [missingCurves, mode === 'curve' ? text.needsCurve : text.needsTwoCurves],
          )}
          primaryLabel={text.apply}
        />
      )}
    </form>
  );
}
