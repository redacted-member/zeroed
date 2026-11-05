import { useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { Point } from '../../bridge/document';
import type { ComponentFill, WeaveMode } from '../../bridge/model';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDrawController, useOverlays } from '../../lib/useDrawController';
import { useEntityPaths } from '../../lib/useEntityPaths';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import type { ControllerMenuItem, DrawController, DrawPointer } from '../../state/drawStore';
import { useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { ModelingActions } from '../ModelingActions/ModelingActions';
import { useModelingApply } from '../ModelingActions/useModelingApply';
import { NumberField } from '../NumberField/NumberField';
import { nearestOnRail, railOf, type Rail } from '../TwoRailSweepForm/rails';
import {
  addToAllNodes,
  attachSection,
  componentToUse,
  convertExtrudeUnits,
  deleteStation,
  extrudeArgs,
  extrudeOverlays,
  initialExtrudeState,
  moveStation,
  openVectorIds,
  railsFromSelection,
  removeAllOn,
  reverseRail,
  sectionsToUse,
  setStart,
  spansMatch,
  startPointFor,
  stationAt,
  toggleSmooth,
  validateExtrude,
  withRails,
  type ExtrudeFormState,
  type ExtrudeSource,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './ExtrudeWeaveForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { weavePicture } from '../HelperPicture/pictures/modeling';

const text = strings.model.extrude;

/** Pick distance around rails and section markers, in screen pixels. */
const PICK_PX = 6;

const SOURCES: ReadonlyArray<Choice<ExtrudeSource>> = [
  { value: 'vectors', label: text.vectors },
  { value: 'component', label: text.component },
];

const FILLS: ReadonlyArray<Choice<ComponentFill>> = [
  { value: 'stretch', label: text.stretch },
  { value: 'copies', label: text.copies },
];

const WEAVES: ReadonlyArray<Choice<WeaveMode>> = [
  { value: 'scaleShape', label: text.scaleShape },
  { value: 'addBase', label: text.addBase },
];

function mmPerPx(): number {
  return 1 / Math.max(useViewStore.getState().camera.pxPerMm, 1e-9);
}

/** What a right click was on, for the menu entry chosen after it. */
type MenuTarget = { station: number } | { rail: number; at: Point } | null;

export function ExtrudeWeaveForm(): JSX.Element {
  const selectedIds = useDocumentStore((store) => store.selection.ids);
  const entities = useDocumentStore((store) => store.entities);
  const components = useModelStore((store) => store.components);
  const closeForm = useUiStore((store) => store.closeForm);
  const camera = useViewStore((store) => store.camera);
  // A component selected when the form opens is the one to carry.
  const { state, setState, stateRef, units } = useFormState(
    (u) => ({ ...initialExtrudeState(u), component: useModelStore.getState().selectedId }),
    convertExtrudeUnits,
  );
  const { apply, reset, startNew } = useModelingApply(null);
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateExtrude(state, units), [state, units]);
  const selectedOpen = openVectorIds(selectedIds, entities);
  const sections = state.source === 'vectors' ? sectionsToUse(state, selectedOpen) : [];
  const paths = useEntityPaths([...state.rails, ...sections]);
  const railsFromPick = railsFromSelection(selectedIds, entities);
  const component = componentToUse(state, components);

  const rails = useMemo(
    (): (Rail | null)[] =>
      state.rails.map((id, k) => {
        const path = paths.get(id);
        return path === undefined
          ? null
          : railOf(path, state.reverse[k] ?? false, state.starts[k] ?? null);
      }),
    [paths, state.rails, state.reverse, state.starts],
  );
  const railsRef = useRef(rails);
  useEffect(() => {
    railsRef.current = rails;
  }, [rails]);
  useEffect(() => () => useUiStore.getState().setStatusHint(null), []);

  const railPaths = state.rails.map((id) => paths.get(id));
  useOverlays(
    useMemo(
      () => extrudeOverlays(state, rails, railPaths, 1 / Math.max(camera.pxPerMm, 1e-9)),
      // railPaths follows paths and state.rails.
      // eslint-disable-next-line react-hooks/exhaustive-deps
      [state, rails, paths, camera.pxPerMm],
    ),
  );

  const spansOk = spansMatch(sections.map((id) => paths.get(id)));
  const menuTarget = useRef<MenuTarget>(null);
  const drag = useRef<number | null>(null);

  const controller = useMemo((): DrawController => {
    /** The rail within reach of `p`, if any. */
    const railNear = (p: Point): { k: number; at: Point } | null => {
      const tolerance = PICK_PX * mmPerPx();
      let best: { k: number; at: Point; d: number } | null = null;
      for (const [k, rail] of railsRef.current.entries()) {
        if (rail === null) {
          continue;
        }
        const hit = nearestOnRail(rail, p);
        if (hit.distance <= tolerance && (best === null || hit.distance < best.d)) {
          best = { k, at: hit.point, d: hit.distance };
        }
      }
      return best === null ? null : { k: best.k, at: best.at };
    };

    /** The selected open vector that is not a rail: the section to attach. */
    const sectionToAttach = (): number | null => {
      if (stateRef.current.source !== 'vectors') {
        return null;
      }
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
        const station = stationAt(stateRef.current, at, PICK_PX * mmPerPx());
        if (station !== null) {
          drag.current = station;
          return;
        }
        const section = sectionToAttach();
        const near = railNear(at);
        if (section !== null && near !== null) {
          setState((s) => attachSection(s, railsRef.current, near.k, section, near.at));
          return;
        }
        void select(p);
      },
      pointerMove: (p) => {
        const dragging = drag.current;
        const ui = useUiStore.getState();
        if (dragging !== null) {
          setState((s) => moveStation(s, railsRef.current, dragging, { x: p.rawX, y: p.rawY }));
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
            { id: 'smooth', label: text.menu.smooth, checked: s.stations[station]?.smooth ?? true },
            { id: 'allNodes', label: text.menu.allNodes },
          ];
          return items;
        }
        const near = railNear(at);
        const rail = near === null ? null : (railsRef.current[near.k] ?? null);
        if (near === null || rail === null) {
          return null;
        }
        menuTarget.current = { rail: near.k, at: near.at };
        const items: ControllerMenuItem[] = [{ id: 'reverse', label: text.menu.reverse }];
        if (rail.closed) {
          items.push(
            { id: 'makeStart', label: text.menu.makeStart },
            { id: 'insertStart', label: text.menu.insertStart },
          );
        }
        items.push({
          id: 'removeAll',
          label: text.menu.removeAll,
          disabled: !s.stations.some((st) => st.rail === near.k),
        });
        return items;
      },
      menuAction: (id) => {
        const target = menuTarget.current;
        if (target === null) {
          return;
        }
        if ('station' in target) {
          const i = target.station;
          if (id === 'deleteSection') {
            setState((s) => deleteStation(s, i));
          } else if (id === 'smooth') {
            setState((s) => toggleSmooth(s, i));
          } else if (id === 'allNodes') {
            setState((s) => addToAllNodes(s, railsRef.current, i));
          }
          return;
        }
        const { rail: k, at } = target;
        const rail = railsRef.current[k] ?? null;
        if (id === 'reverse') {
          setState((s) => reverseRail(s, k));
        } else if ((id === 'makeStart' || id === 'insertStart') && rail !== null) {
          setState((s) => setStart(s, k, startPointFor(rail, at, id === 'insertStart')));
        } else if (id === 'removeAll') {
          setState((s) => removeAllOn(s, k));
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
    const args = extrudeArgs(state, rails, selectedOpen, components, units);
    if (args === null) {
      return;
    }
    const name = state.name.trim();
    void apply((target) =>
      api.model.extrude({
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

  const set = (patch: Partial<ExtrudeFormState>) => setState((s) => ({ ...s, ...patch }));
  const percent = text.percent;

  const pickWhy = railsFromPick === null ? text.needsRails : undefined;
  const clearWhy = state.rails.length === 0 ? strings.why.noRails : undefined;
  return (
    <form className="extrude-weave" onSubmit={onSubmit}>
      <fieldset className="extrude-weave__group">
        <legend className="extrude-weave__legend">{text.railsTitle}</legend>
        <p className="extrude-weave__hint">{text.railsHint}</p>
        <div className="extrude-weave__buttons">
          <button
            className="extrude-weave__pick"
            disabled={pickWhy !== undefined}
            onClick={() => railsFromPick !== null && setState((s) => withRails(s, railsFromPick))}
            data-tip={whyTip(text.useSelection, pickWhy)}
            type="button"
          >
            <ButtonFace icon="vectorSelection" label={text.useSelection} />
          </button>
          <button
            className="extrude-weave__pick"
            disabled={clearWhy !== undefined}
            onClick={clearRails}
            type="button"
            data-tip={whyTip(text.clear, clearWhy)}
          >
            <ButtonFace icon="editDelete" label={text.clear} />
          </button>
        </div>
        <p className="extrude-weave__count" aria-live="polite">
          {text.railCount(state.rails.length)}
        </p>
      </fieldset>
      <fieldset className="extrude-weave__group">
        <legend className="extrude-weave__legend">{text.sectionsTitle}</legend>
        <ChoiceField
          label={text.source}
          onChange={(source) => set({ source })}
          options={SOURCES}
          value={state.source}
        />
        {state.source === 'vectors' ? (
          <>
            <p className="extrude-weave__hint">{text.sectionsHint}</p>
            <p className="extrude-weave__count" aria-live="polite">
              {text.stationCount(state.stations.length)}
            </p>
            {state.rails.length > 0 && sections.length === 0 ? (
              <p className="extrude-weave__count extrude-weave__count--error">
                {text.needsSections}
              </p>
            ) : null}
            <CheckField
              checked={state.sharpCorners}
              label={text.sharpCorners}
              onChange={(sharpCorners) => set({ sharpCorners })}
            />
            <CheckField
              checked={state.betweenSpans && spansOk}
              disabled={!spansOk}
              label={text.betweenSpans}
              onChange={(betweenSpans) => set({ betweenSpans })}
            />
          </>
        ) : (
          <>
            {components.length === 0 ? (
              <p className="extrude-weave__count extrude-weave__count--error">
                {text.noComponents}
              </p>
            ) : (
              <label className="extrude-weave__field">
                <span className="extrude-weave__label">{text.componentList}</span>
                <select
                  className="extrude-weave__select"
                  onChange={(event) => set({ component: Number(event.target.value) })}
                  value={component ?? ''}
                >
                  {components.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.name}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <p className="extrude-weave__hint">{text.componentHint}</p>
            <ChoiceField
              label={text.fill}
              onChange={(fill) => set({ fill })}
              options={FILLS}
              value={state.fill}
            />
            <NumberField
              disabled={state.fill !== 'copies'}
              error={showErrors ? errors.overlap : undefined}
              label={text.overlap}
              onChange={(overlap) => set({ overlap })}
              unit={percent}
              value={state.overlap}
            />
            <CheckField
              checked={state.flipAlternate}
              disabled={state.fill !== 'copies'}
              label={text.flipAlternate}
              onChange={(flipAlternate) => set({ flipAlternate })}
            />
          </>
        )}
      </fieldset>
      <fieldset className="extrude-weave__group">
        <legend className="extrude-weave__legend">{text.weave}</legend>
        <HelperRow picture={<HelperPicture picture={weavePicture(state.weave)} />}>
          <CheckField
            checked={state.weave}
            label={text.weave}
            onChange={(weave) => set({ weave })}
          />
          <p className="extrude-weave__hint">{text.weaveHint}</p>
          <ChoiceField
            disabled={!state.weave}
            label={text.weaveMode}
            onChange={(weaveMode) => set({ weaveMode })}
            options={WEAVES}
            value={state.weaveMode}
          />
          <NumberField
            disabled={!state.weave}
            error={showErrors ? errors.zUnder : undefined}
            label={text.zUnder}
            onChange={(zUnder) => set({ zUnder })}
            unit={percent}
            value={state.zUnder}
          />
          <NumberField
            disabled={!state.weave}
            error={showErrors ? errors.zOver : undefined}
            label={text.zOver}
            onChange={(zOver) => set({ zOver })}
            unit={percent}
            value={state.zOver}
          />
        </HelperRow>
      </fieldset>
      <fieldset className="extrude-weave__group">
        <legend className="extrude-weave__legend">{text.settings}</legend>
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
      </fieldset>
      <ModelingActions
        applyWhy={firstWhy(
          [state.rails.length === 0, text.needsRails],
          [state.source === 'vectors' && sections.length === 0, text.needsSections],
          [state.source !== 'vectors' && component === null, strings.why.pickComponent],
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
