import { useEffect, useMemo, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { TabOp } from '../../bridge/cam';
import type { Point } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDrawController, useOverlays } from '../../lib/useDrawController';
import type { DrawController, PreviewOverlay } from '../../state/drawStore';
import { useViewStore } from '../../state/viewStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { NumberField } from '../NumberField/NumberField';
import {
  tabSpec,
  type ProfileErrors,
  type ProfileFormState,
  type TabPlacement,
  type TabRule,
} from '../ProfileToolpathForm/model';
import { isDrag, reachMm, tabMarkers } from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './EditTabsPanel.scss';

const text = strings.toolpaths.profile;

const SPECIFY: ReadonlyArray<Choice<TabPlacement>> = [
  { value: 'count', label: text.tabByCount },
  { value: 'distance', label: text.tabByDistance },
];

const RULES: ReadonlyArray<Choice<TabRule>> = [
  { value: 'firstAtStart', label: text.tabFirstAtStart },
  { value: 'avoidCorners', label: text.tabAvoidCorners },
];

interface EditTabsPanelProps {
  state: ProfileFormState;
  errors: ProfileErrors;
  units: Units;
  /** The vectors the toolpath machines. */
  ids: readonly number[];
  onChange: (patch: Partial<ProfileFormState>) => void;
  onClose: () => void;
}

/**
 * Toolpath Tabs: places tabs automatically on the vectors, and while it is
 * open a click in the 2D view adds or deletes a tab and a drag moves one.
 */
export function EditTabsPanel(props: EditTabsPanelProps): JSX.Element {
  const { state, errors, units, ids, onChange, onClose } = props;
  const pxPerMm = useViewStore((view) => view.camera.pxPerMm);
  const [busy, setBusy] = useState(false);
  const live = useRef({ state, units, ids, onChange });
  useEffect(() => {
    live.current = { state, units, ids, onChange };
  });
  const pressed = useRef<Point | null>(null);

  const run = useRef(async (op: TabOp, at?: Point, from?: Point) => {
    const { state: s, units: u, ids: vectors, onChange: change } = live.current;
    const reply = await api.cam.tabs({
      ids: vectors,
      tabs: { ...tabSpec(s, u), enabled: true },
      positions: s.tabPositions ?? [],
      op,
      ...(at === undefined ? {} : { at }),
      ...(from === undefined ? {} : { from }),
      reach: reachMm(useViewStore.getState().camera.pxPerMm),
    });
    change({ tabs: true, tabPositions: reply.positions });
  });

  const controller = useMemo<DrawController>(
    () => ({
      cursor: 'crosshair',
      pointerDown: (p) => {
        pressed.current = { x: p.rawX, y: p.rawY };
      },
      pointerMove: () => undefined,
      pointerUp: (p) => {
        const from = pressed.current;
        pressed.current = null;
        if (from === null) {
          return;
        }
        const at = { x: p.rawX, y: p.rawY };
        const drag = isDrag(from, at, useViewStore.getState().camera.pxPerMm);
        void run.current(drag ? 'move' : 'click', at, drag ? from : undefined);
      },
      cancel: () => {
        pressed.current = null;
      },
    }),
    [],
  );
  useDrawController(controller);

  const overlays = useMemo<PreviewOverlay[]>(
    () =>
      tabMarkers(state.tabPositions ?? [], pxPerMm).map((path) => ({
        path,
        tone: 'accent',
        fill: true,
      })),
    [pxPerMm, state.tabPositions],
  );
  useOverlays(overlays);

  useEffect(() => {
    // The toolpath needs tabs on while they are being edited.
    if (!state.tabs) {
      onChange({ tabs: true });
    }
  }, [onChange, state.tabs]);

  const act = (op: TabOp) => {
    setBusy(true);
    void run.current(op).finally(() => setBusy(false));
  };
  const count = state.tabPositions?.length ?? null;
  const addWhy = firstWhy([busy, strings.why.busy], [ids.length === 0, text.tabsNeedVectors]);
  const clearWhy = firstWhy(
    [busy, strings.why.busy],
    [count === null || count === 0, strings.why.noTabs],
  );

  return (
    <section aria-labelledby="edit-tabs-title" className="edit-tabs">
      <h3 className="edit-tabs__title" id="edit-tabs-title">
        {text.tabsTitle}
      </h3>
      {ids.length === 0 ? <p className="edit-tabs__notice">{text.tabsNeedVectors}</p> : null}
      <fieldset className="edit-tabs__group">
        <legend className="edit-tabs__legend">{text.tabsAuto}</legend>
        <ChoiceField
          label={text.tabsSpecify}
          onChange={(tabPlacement) => onChange({ tabPlacement })}
          options={SPECIFY}
          value={state.tabPlacement}
        />
        {state.tabPlacement === 'count' ? (
          <NumberField
            error={errors.tabCount}
            label={text.tabCount}
            onChange={(tabCount) => onChange({ tabCount })}
            step={1}
            value={state.tabCount}
          />
        ) : (
          <NumberField
            error={errors.tabSpacing}
            label={text.tabSpacing}
            onChange={(tabSpacing) => onChange({ tabSpacing })}
            unit={unitSuffix(units)}
            value={state.tabSpacing}
          />
        )}
        <ChoiceField
          label={text.tabPlacement}
          onChange={(tabRule) => onChange({ tabRule })}
          options={RULES}
          value={state.tabRule}
        />
        <button
          className="edit-tabs__button"
          disabled={addWhy !== undefined}
          onClick={() => act('auto')}
          type="button"
          data-tip={whyTip(text.tabsAdd, addWhy)}
        >
          <ButtonFace icon="formCreate" label={text.tabsAdd} />
        </button>
      </fieldset>
      <fieldset className="edit-tabs__group">
        <legend className="edit-tabs__legend">{text.tabsInteractive}</legend>
        <p className="edit-tabs__hint">{text.tabsInteractiveHint}</p>
        <p className="edit-tabs__count" aria-live="polite">
          {text.tabsPlaced(count)}
        </p>
        <button
          className="edit-tabs__button"
          disabled={clearWhy !== undefined}
          onClick={() => act('clear')}
          type="button"
          data-tip={whyTip(text.tabsDeleteAll, clearWhy)}
        >
          <ButtonFace icon="deleteToolpath" label={text.tabsDeleteAll} />
        </button>
      </fieldset>
      <button
        className="edit-tabs__close"
        onClick={onClose}
        type="button"
        data-tip={text.tabsClose}
      >
        <ButtonFace icon="dismiss" label={text.tabsClose} />
      </button>
    </section>
  );
}
