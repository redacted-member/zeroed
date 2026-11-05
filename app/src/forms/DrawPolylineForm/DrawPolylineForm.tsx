import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/api';
import type { Point } from '../../bridge/document';
import { quickStepPoint } from '../../lib/quickKeys';
import { strings } from '../../lib/formStrings';
import { toMillimetres, unitSuffix } from '../../lib/units';
import { useDrawController, usePreview } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController } from '../../state/drawStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { extendPath, nearestArc, openEndNear } from '../../lib/spans';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  addPoint,
  clearPoints,
  closesShape,
  constrainAngle,
  convertPolylineUnits,
  initialPolylineState,
  lastPoint,
  tangentPoint,
  commonTangent,
  moveLastPoint,
  onCircle,
  polarPoint,
  polylinePathFromState,
  polylinePreview,
  setRelative,
  startExtending,
  toggleCurveMode,
  typedPoint,
  undoPoint,
  validatePolyline,
  type PolylineField,
  lineEndKey,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './DrawPolylineForm.scss';

const CLOSE_TOLERANCE_PX = 8;
/** Moving at least this far between press and release makes a drag. */
const DRAG_PX = 4;

const ENTRIES: ReadonlyArray<Choice<'xy' | 'polar' | 'offset'>> = [
  { value: 'xy', label: strings.createVectors.polyline.entryXY },
  { value: 'polar', label: strings.createVectors.polyline.entryPolar },
  { value: 'offset', label: strings.createVectors.polyline.entryOffset },
];

type NextEntry = 'xy' | 'polar' | 'offset';

export function DrawPolylineForm(): JSX.Element {
  const createPath = useDocumentStore((store) => store.createPath);
  const closeForm = useUiStore((store) => store.closeForm);
  const text = strings.createVectors.polyline;
  const { state, setState, stateRef, units, unitsRef } = useFormState(
    (u) => initialPolylineState(u, usePrefsStore.getState().prefs.general.lineCurveMode),
    convertPolylineUnits,
  );
  // The curve mode is remembered for the next time.
  const curveMode = state.curveMode;
  useEffect(() => {
    const prefs = usePrefsStore.getState();
    if (prefs.prefs.general.lineCurveMode !== curveMode) {
      void prefs.update({ general: { lineCurveMode: curveMode } });
    }
  }, [curveMode]);
  const [cursor, setCursor] = useState<Point | null>(null);
  // How the Next Point is typed: X and Y, Angle and Length, or dX and dY.
  const [entry, setEntry] = useState<NextEntry>('xy');
  const [showErrors, setShowErrors] = useState(false);
  const cursorRef = useRef<Point | null>(null);

  useEffect(() => {
    cursorRef.current = cursor;
  }, [cursor]);

  const errors = useMemo(() => validatePolyline(state, units), [state, units]);
  usePreview(useMemo(() => polylinePreview(state, cursor), [state, cursor]));

  const finish = useCallback(
    async (closed: boolean) => {
      const current = stateRef.current;
      const built = polylinePathFromState(current, closed);
      // Clear at once so a second finish (Escape then close) finds nothing.
      stateRef.current = clearPoints(current);
      setState((s) => clearPoints(s));
      setCursor(null);
      if (built === null) {
        return;
      }
      const extend = current.extend;
      const drawn = built.subpaths[0];
      if (extend !== null && !closed && drawn !== undefined) {
        const entity = await api.entities.get(extend.id).catch(() => null);
        if (entity !== null && entity.kind === 'path') {
          const path = extendPath(entity.path, drawn, extend.atEnd);
          if (path !== null) {
            await api.entities.replace(extend.id, { ...entity, path });
            await useDocumentStore.getState().refresh();
            return;
          }
        }
      }
      await createPath(built);
    },
    [createPath, setState, stateRef],
  );

  const pressRef = useRef<{ at: Point; extending: Promise<void> | null } | null>(null);

  const controller = useMemo<DrawController>(
    () => ({
      pointerDown: (p) => {
        const current = stateRef.current;
        const from = lastPoint(current);
        let at = { x: p.x, y: p.y };
        if (from !== null && p.shift) {
          at = constrainAngle(from, at);
        }
        let extending: Promise<void> | null = null;
        if (p.ctrl && current.points.length === 0) {
          // Ctrl on the end of an open vector carries that vector on.
          const tolerance = CLOSE_TOLERANCE_PX / useViewStore.getState().camera.pxPerMm;
          extending = api.entities
            .hitTest({ x: p.rawX, y: p.rawY, tolerance })
            .then(async ({ ids }) => {
              for (const id of ids) {
                const entity = await api.entities.get(id);
                if (entity.kind !== 'path') {
                  continue;
                }
                const end = openEndNear(entity.path, { x: p.rawX, y: p.rawY }, tolerance);
                if (end !== null) {
                  const next = startExtending(stateRef.current, unitsRef.current, end.point, {
                    id,
                    atEnd: end.atEnd,
                  });
                  stateRef.current = next;
                  setState(next);
                  return;
                }
              }
            })
            .catch(() => undefined);
        }
        pressRef.current = { at, extending };
      },
      pointerMove: (p) => {
        const from = lastPoint(stateRef.current);
        const point = { x: p.x, y: p.y };
        setCursor(from !== null && p.shift ? constrainAngle(from, point) : point);
      },
      pointerUp: (p) => {
        const press = pressRef.current;
        pressRef.current = null;
        const release = { x: p.x, y: p.y };
        const pxPerMm = useViewStore.getState().camera.pxPerMm;
        const apply = () => {
          const current = stateRef.current;
          if (press?.extending !== null && press?.extending !== undefined) {
            // The press started the line on the vector's end; the release
            // adds the next point when the pointer moved.
            const moved = Math.hypot(release.x - press.at.x, release.y - press.at.y);
            if (current.points.length > 0 && moved * pxPerMm >= DRAG_PX) {
              setState((s) => addPoint(s, unitsRef.current, release));
            }
            return;
          }
          const from = lastPoint(current);
          const start = press?.at ?? release;
          const dragged = Math.hypot(release.x - start.x, release.y - start.y) * pxPerMm >= DRAG_PX;
          let point = dragged ? start : release;
          if (!dragged && from !== null && p.shift) {
            point = constrainAngle(from, point);
          }
          const tolerance = CLOSE_TOLERANCE_PX / pxPerMm;
          if (closesShape(current, point, tolerance)) {
            void finish(true);
            return;
          }
          const drag = dragged ? { x: release.x - start.x, y: release.y - start.y } : undefined;
          setState((s) => addPoint(s, unitsRef.current, point, drag));
        };
        if (press?.extending) {
          void press.extending.then(apply);
        } else {
          apply();
        }
      },
      keyDown: (event) => {
        const ending = lineEndKey(event.key);
        if (ending !== null) {
          void finish(ending === 'close');
          return true;
        }
        if (event.key === 's' || event.key === 'S') {
          setState((s) => toggleCurveMode(s));
          return true;
        }
        if (event.key === 't' || event.key === 'T') {
          // Tangents: from the last point to the arc under the pointer; from
          // an arc the last point sits on to the pointer; or from that arc to
          // the arc under the pointer. A point on an arc slides along it.
          const from = lastPoint(stateRef.current);
          const cursor = cursorRef.current;
          if (from === null || cursor === null) {
            return false;
          }
          const tolerance = CLOSE_TOLERANCE_PX / useViewStore.getState().camera.pxPerMm;
          void (async () => {
            const arcAt = async (at: Point, within: number) => {
              const { ids } = await api.entities.hitTest({ x: at.x, y: at.y, tolerance: within });
              for (const id of ids) {
                const entity = await api.entities.get(id);
                const circle = entity.kind === 'path' ? nearestArc(entity.path, at) : null;
                if (circle !== null && onCircle(at, circle, within)) {
                  return circle;
                }
              }
              return null;
            };
            const start = await arcAt(from, tolerance);
            const target = await arcAt(cursor, tolerance);
            const units = unitsRef.current;
            const same =
              start !== null &&
              target !== null &&
              Math.hypot(start.centre.x - target.centre.x, start.centre.y - target.centre.y) <
                1e-6 &&
              Math.abs(start.radius - target.radius) < 1e-6;
            if (start !== null && target !== null && !same) {
              const pair = commonTangent(start, target, from, cursor);
              if (pair !== null) {
                setState((s) => addPoint(moveLastPoint(s, units, pair[0]), units, pair[1]));
              }
              return;
            }
            if (start !== null && target === null) {
              const touch = tangentPoint(cursor, start, from);
              if (touch !== null) {
                setState((s) => addPoint(moveLastPoint(s, units, touch), units, cursor));
              }
              return;
            }
            const touch = target === null ? null : tangentPoint(from, target, cursor);
            if (touch !== null) {
              setState((s) => addPoint(s, units, touch));
            }
          })().catch(() => undefined);
          return true;
        }
        if (event.key === 'Backspace') {
          setState((s) => undoPoint(s));
          return true;
        }
        return false;
      },
      quickEntry: (entry, cursor) => {
        const from = lastPoint(stateRef.current);
        if (from === null) {
          return false;
        }
        const units = unitsRef.current;
        const point = quickStepPoint(
          entry,
          from,
          cursor,
          (v) => toMillimetres(v, units),
          'absolute',
        );
        if (point === null) {
          return false;
        }
        setState((s) => addPoint(s, units, point));
        return true;
      },
      cancel: () => {
        // Escape finishes the line drawn so far and leaves the tool.
        void finish(false);
        closeForm();
      },
    }),
    [closeForm, finish, setState, stateRef, unitsRef],
  );
  useDrawController(controller);

  const addTyped = () => {
    setShowErrors(true);
    const point = typedPoint(stateRef.current, unitsRef.current);
    if (point !== null) {
      setState((s) => addPoint(s, unitsRef.current, point));
    }
  };

  const addPolar = () => {
    setShowErrors(true);
    const point = polarPoint(stateRef.current, unitsRef.current);
    if (point !== null) {
      setState((s) => addPoint(s, unitsRef.current, point));
    }
  };

  const setField = (field: PolylineField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: PolylineField) => (showErrors ? errors[field] : undefined);
  const unit = unitSuffix(units);
  const canFinish = state.points.length >= 2;

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void finish(false);
  };

  const addWhy = entry === 'polar' && state.points.length === 0 ? strings.why.noPoints : undefined;
  const undoWhy = state.points.length === 0 ? strings.why.noPoints : undefined;
  return (
    <form className="draw-polyline" onSubmit={onSubmit}>
      <p className="draw-polyline__hint">{text.hint}</p>
      <fieldset className="draw-polyline__group">
        <legend className="draw-polyline__legend">{text.nextPoint}</legend>
        <ChoiceField
          label={text.entry}
          onChange={(entry) => {
            setEntry(entry);
            setState((s) => setRelative(s, unitsRef.current, entry === 'offset'));
          }}
          options={ENTRIES}
          value={entry}
        />
        {entry === 'polar' ? (
          <>
            <NumberField
              error={errorFor('angle')}
              label={text.angle}
              onChange={(v) => setField('angle', v)}
              onEnter={addPolar}
              unit="deg"
              value={state.angle}
            />
            <NumberField
              error={errorFor('length')}
              label={text.length}
              onChange={(v) => setField('length', v)}
              onEnter={addPolar}
              unit={unit}
              value={state.length}
            />
          </>
        ) : (
          <>
            <NumberField
              error={errorFor('nextX')}
              label={state.relative ? text.offsetX : text.nextX}
              onChange={(v) => setField('nextX', v)}
              onEnter={addTyped}
              unit={unit}
              value={state.nextX}
            />
            <NumberField
              error={errorFor('nextY')}
              label={state.relative ? text.offsetY : text.nextY}
              onChange={(v) => setField('nextY', v)}
              onEnter={addTyped}
              unit={unit}
              value={state.nextY}
            />
          </>
        )}
        <button
          className="draw-polyline__button"
          disabled={addWhy !== undefined}
          onClick={entry === 'polar' ? addPolar : addTyped}
          type="button"
          data-tip={whyTip(text.addPoint, addWhy)}
        >
          <ButtonFace icon="formCreate" label={text.addPoint} />
        </button>
      </fieldset>
      <div className="draw-polyline__actions">
        <button
          className="draw-polyline__button"
          disabled={undoWhy !== undefined}
          onClick={() => setState((s) => undoPoint(s))}
          type="button"
          data-tip={whyTip(text.undoPoint, undoWhy)}
        >
          <ButtonFace icon="editUndo" label={text.undoPoint} />
        </button>
      </div>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => void finish(false)}
        primaryWhy={canFinish ? undefined : strings.why.twoPoints}
        primaryLabel={text.finish}
        submit
      />
    </form>
  );
}
