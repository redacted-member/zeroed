import { useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { Point } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDrawController, useOverlays } from '../../lib/useDrawController';
import { useEntityPaths } from '../../lib/useEntityPaths';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import type { ControllerMenuItem, DrawController, DrawPointer } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { CheckField } from '../CheckField/CheckField';
import { ModelingActions } from '../ModelingActions/ModelingActions';
import { useModelingApply } from '../ModelingActions/useModelingApply';
import { NumberField } from '../NumberField/NumberField';
import {
  addToAllNodes,
  attachSection,
  convertSweepUnits,
  deleteStation,
  initialSweepState,
  moveStationEnd,
  openVectorIds,
  railsFromSelection,
  reverseRail,
  sectionsToUse,
  setStart,
  spansMatch,
  startPointFor,
  stationAt,
  stationEndAt,
  swapRails,
  sweepArgs,
  sweepOverlays,
  toggleSmooth,
  validateSweep,
  withRails,
  type SweepFormState,
} from './model';
import { nearestOnRail, railOf, type Rail } from './rails';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './TwoRailSweepForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { sweep } from '../HelperPicture/pictures/modeling';

const text = strings.model.sweep;

/** Pick distance around rails and section ends, in screen pixels. */
const PICK_PX = 6;

function mmPerPx(): number {
  return 1 / Math.max(useViewStore.getState().camera.pxPerMm, 1e-9);
}

/** What a right click was on, for the menu entry chosen after it. */
type MenuTarget = { station: number } | { rail: 0 | 1; at: Point } | null;

export function TwoRailSweepForm(): JSX.Element {
  const selectedIds = useDocumentStore((store) => store.selection.ids);
  const entities = useDocumentStore((store) => store.entities);
  const closeForm = useUiStore((store) => store.closeForm);
  const camera = useViewStore((store) => store.camera);
  const { state, setState, stateRef, units } = useFormState(initialSweepState, convertSweepUnits);
  const { apply, reset, startNew } = useModelingApply(null);
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateSweep(state, units), [state, units]);
  const selectedOpen = openVectorIds(selectedIds, entities);
  const sections = sectionsToUse(state, selectedOpen);
  const paths = useEntityPaths([...state.rails, ...sections]);
  const railsFromPick = railsFromSelection(selectedIds, entities);

  const rails = useMemo((): [Rail, Rail] | null => {
    const [a, b] = state.rails;
    const pa = a === undefined ? undefined : paths.get(a);
    const pb = b === undefined ? undefined : paths.get(b);
    if (pa === undefined || pb === undefined) {
      return null;
    }
    const ra = railOf(pa, state.reverse[0], state.starts[0]);
    const rb = railOf(pb, state.reverse[1], state.starts[1]);
    return ra === null || rb === null ? null : [ra, rb];
  }, [paths, state.rails, state.reverse, state.starts]);
  const railsRef = useRef(rails);
  useEffect(() => {
    railsRef.current = rails;
  }, [rails]);
  useEffect(() => () => useUiStore.getState().setStatusHint(null), []);

  const railPaths = state.rails.map((id) => paths.get(id));
  useOverlays(
    useMemo(
      () => sweepOverlays(state, rails, railPaths, 1 / Math.max(camera.pxPerMm, 1e-9)),
      // railPaths follows paths and state.rails.
      // eslint-disable-next-line react-hooks/exhaustive-deps
      [state, rails, paths, camera.pxPerMm],
    ),
  );

  const closedRails = rails !== null && rails[0].closed && rails[1].closed;
  const spansOk = spansMatch(sections.map((id) => paths.get(id)));

  const menuTarget = useRef<MenuTarget>(null);
  const drag = useRef<{ index: number; end: 'a' | 'b' } | null>(null);

  const controller = useMemo((): DrawController => {
    /** The rail within reach of `p`, if any. */
    const railNear = (p: Point): { k: 0 | 1; at: Point } | null => {
      const current = railsRef.current;
      if (current === null) {
        return null;
      }
      const tolerance = PICK_PX * mmPerPx();
      let best: { k: 0 | 1; at: Point; d: number } | null = null;
      for (const k of [0, 1] as const) {
        const hit = nearestOnRail(current[k], p);
        if (hit.distance <= tolerance && (best === null || hit.distance < best.d)) {
          best = { k, at: hit.point, d: hit.distance };
        }
      }
      return best === null ? null : { k: best.k, at: best.at };
    };

    /** The selected open vector that is not a rail: the section to attach. */
    const sectionToAttach = (): number | null => {
      const doc = useDocumentStore.getState();
      const open = openVectorIds(doc.selection.ids, doc.entities).filter(
        (id) => !stateRef.current.rails.includes(id),
      );
      return open.length === 1 ? (open[0] ?? null) : null;
    };

    const select = async (p: DrawPointer) => {
      const { ids } = await api.entities.hitTest({
        x: p.rawX,
        y: p.rawY,
        tolerance: PICK_PX * mmPerPx(),
      });
      const hit = ids[0];
      const doc = useDocumentStore.getState();
      if (hit === undefined) {
        if (!p.shift) {
          await doc.clearSelection();
        }
        return;
      }
      await doc.select([hit], p.shift ? 'toggle' : 'set');
    };

    return {
      cursor: 'default',
      pointerDown: (p) => {
        const at = { x: p.rawX, y: p.rawY };
        const current = railsRef.current;
        const end = stationEndAt(stateRef.current, at, PICK_PX * mmPerPx());
        if (end !== null && current !== null) {
          drag.current = end;
          return;
        }
        const section = sectionToAttach();
        const near = railNear(at);
        if (current !== null && section !== null && near !== null) {
          setState((s) => attachSection(s, current, near.k, section, near.at));
          return;
        }
        void select(p);
      },
      pointerMove: (p) => {
        const current = railsRef.current;
        const dragging = drag.current;
        const ui = useUiStore.getState();
        if (dragging !== null && current !== null) {
          setState((s) =>
            moveStationEnd(s, current, dragging.index, dragging.end, { x: p.rawX, y: p.rawY }),
          );
          return;
        }
        const near = railNear({ x: p.rawX, y: p.rawY });
        const hint = near !== null && sectionToAttach() !== null ? text.attachHere : null;
        if (ui.statusHint !== hint) {
          ui.setStatusHint(hint);
        }
      },
      pointerUp: () => {
        drag.current = null;
      },
      contextMenu: (at) => {
        const s = stateRef.current;
        const station = stationAt(s, at, PICK_PX * mmPerPx());
        if (station !== null) {
          menuTarget.current = { station };
          const items: ControllerMenuItem[] = [
            { id: 'deleteSection', label: text.menu.deleteSection },
            {
              id: 'smooth',
              label: text.menu.smooth,
              checked: s.stations[station]?.smooth ?? true,
            },
            { id: 'allNodes', label: text.menu.allNodes },
          ];
          return items;
        }
        const near = railNear(at);
        const current = railsRef.current;
        if (near === null || current === null) {
          return null;
        }
        menuTarget.current = { rail: near.k, at: near.at };
        const items: ControllerMenuItem[] = [{ id: 'reverse', label: text.menu.reverse }];
        if (current[near.k].closed) {
          items.push(
            { id: 'makeStart', label: text.menu.makeStart },
            { id: 'insertStart', label: text.menu.insertStart },
          );
        }
        items.push({
          id: 'removeAll',
          label: text.menu.removeAll,
          disabled: s.stations.length === 0,
        });
        return items;
      },
      menuAction: (id) => {
        const target = menuTarget.current;
        const current = railsRef.current;
        if (target === null || current === null) {
          return;
        }
        if ('station' in target) {
          const i = target.station;
          if (id === 'deleteSection') {
            setState((s) => deleteStation(s, i));
          } else if (id === 'smooth') {
            setState((s) => toggleSmooth(s, i));
          } else if (id === 'allNodes') {
            setState((s) => {
              const section = s.stations[i]?.section;
              const next = section === undefined ? null : addToAllNodes(s, current, section);
              if (next === null) {
                useDocumentStore.setState({ lastError: text.nodesDiffer });
                return s;
              }
              return next;
            });
          }
          return;
        }
        const { rail: k, at } = target;
        if (id === 'reverse') {
          setState((s) => reverseRail(s, k));
        } else if (id === 'makeStart' || id === 'insertStart') {
          setState((s) => setStart(s, k, startPointFor(current[k], at, id === 'insertStart')));
        } else if (id === 'removeAll') {
          setState((s) => ({ ...s, stations: [] }));
        }
      },
      cancel: () => {
        drag.current = null;
      },
    };
  }, [setState, stateRef]);
  useDrawController(controller);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const args = sweepArgs(state, rails, selectedOpen, units);
    if (args === null) {
      return;
    }
    const name = state.name.trim();
    void apply((target) =>
      api.model.sweep({
        ...args,
        combine: state.combine,
        ...(name === '' ? {} : { name }),
        ...target,
      }),
    ).then(() => {
      if (useUiStore.getState().viewMode === '2d') {
        useUiStore.getState().setViewMode('split');
      }
    });
  };

  const clearRails = () => {
    void reset();
    setState((s) => withRails(s, []));
    void useDocumentStore.getState().clearSelection();
  };

  const set = (patch: Partial<SweepFormState>) => setState((s) => ({ ...s, ...patch }));
  const status =
    state.rails.length === 2
      ? text.stationCount(state.stations.length)
      : text.railCount(state.rails.length);

  const pickWhy = railsFromPick === null ? text.needsTwoRails : undefined;
  const swapWhy = state.rails.length !== 2 ? strings.why.twoRails : undefined;
  const clearWhy = state.rails.length === 0 ? strings.why.noRails : undefined;
  return (
    <form className="two-rail-sweep" onSubmit={onSubmit}>
      <fieldset className="two-rail-sweep__group">
        <legend className="two-rail-sweep__legend">{text.railsTitle}</legend>
        <p className="two-rail-sweep__hint">{text.railsHint}</p>
        <div className="two-rail-sweep__buttons">
          <button
            className="two-rail-sweep__pick"
            disabled={pickWhy !== undefined}
            onClick={() => railsFromPick !== null && setState((s) => withRails(s, railsFromPick))}
            data-tip={whyTip(text.useSelection, pickWhy)}
            type="button"
          >
            <ButtonFace icon="vectorSelection" label={text.useSelection} />
          </button>
          <button
            className="two-rail-sweep__pick"
            disabled={swapWhy !== undefined}
            onClick={() => setState(swapRails)}
            type="button"
            data-tip={whyTip(text.swap, swapWhy)}
          >
            <ButtonFace icon="swap" label={text.swap} />
          </button>
          <button
            className="two-rail-sweep__pick"
            disabled={clearWhy !== undefined}
            onClick={clearRails}
            type="button"
            data-tip={whyTip(text.clear, clearWhy)}
          >
            <ButtonFace icon="editDelete" label={text.clear} />
          </button>
        </div>
        <p className="two-rail-sweep__count" aria-live="polite">
          {status}
        </p>
      </fieldset>
      <fieldset className="two-rail-sweep__group">
        <legend className="two-rail-sweep__legend">{text.sectionsTitle}</legend>
        <p className="two-rail-sweep__hint">{text.sectionsHint}</p>
        {state.rails.length === 2 && sections.length === 0 ? (
          <p className="two-rail-sweep__count two-rail-sweep__count--error">{text.needsSections}</p>
        ) : null}
      </fieldset>
      <fieldset className="two-rail-sweep__group">
        <legend className="two-rail-sweep__legend">{text.settings}</legend>
        <HelperRow picture={<HelperPicture picture={sweep} />}>
          <CheckField
            checked={state.scaleWithWidth}
            label={text.scaleHeight}
            onChange={(scaleWithWidth) => set({ scaleWithWidth })}
          />
          <CheckField
            checked={state.betweenSpans && spansOk}
            disabled={!spansOk}
            label={text.betweenSpans}
            onChange={(betweenSpans) => set({ betweenSpans })}
          />
          <CheckField
            checked={state.fillInner && closedRails}
            disabled={!closedRails}
            label={text.fillInner}
            onChange={(fillInner) => set({ fillInner })}
          />
          <CheckField
            checked={state.exactHeight}
            label={text.exactHeight}
            onChange={(exactHeight) => set({ exactHeight })}
          />
          <NumberField
            disabled={!state.exactHeight}
            error={showErrors ? errors.height : undefined}
            label={text.height}
            onChange={(height) => set({ height })}
            unit={unitSuffix(units)}
            value={state.height}
          />
        </HelperRow>
      </fieldset>
      <ModelingActions
        applyWhy={firstWhy(
          [rails === null, text.needsTwoRails],
          [sections.length === 0, text.needsSections],
        )}
        combine={state.combine}
        name={state.name}
        onClose={closeForm}
        onCombine={(combine) => set({ combine })}
        onName={(name) => set({ name })}
        onReset={() => void reset()}
        onStartNew={() => {
          void startNew();
          setState((s) => withRails(s, []));
        }}
      />
    </form>
  );
}
