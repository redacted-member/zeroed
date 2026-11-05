import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { StraightInlayParams } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { nextToolpathName } from '../../lib/toolpathNames';
import { depthValues } from '../../lib/toolpathFields';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { ChoiceField } from '../ChoiceField/ChoiceField';
import { DepthFields } from '../DepthFields/DepthFields';
import { EditTabsPanel } from '../EditTabsPanel/EditTabsPanel';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { PassesRow } from '../PassesRow/PassesRow';
import { ProfileOptionPages } from '../ProfileOptionPages/ProfileOptionPages';
import { DIRECTIONS } from '../ProfileToolpathForm/ProfileToolpathForm';
import type { ProfileFormState } from '../ProfileToolpathForm/model';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { baseNameOf, KIND_TITLES, type InlayKind } from '../StraightInlayToolpathForm/model';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import {
  convertInlayProfileUnits,
  initialInlayProfileState,
  inlayProfileParams,
  inlayProfileStateFrom,
  validateInlayProfile,
} from './model';

import './InlayProfileForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { machineVectorsPicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;
const own = text.straightInlay;

interface InlayProfileFormProps {
  kind: Exclude<InlayKind, 'femalePocket'>;
}

/** Male Straight, Male Stepped and Female Hole: a Profile cut with the
 * inlay's corner rounding and allowance, on its fixed side. */
export function InlayProfileForm({ kind }: InlayProfileFormProps): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const initial = useCallback(
    (units: Parameters<typeof initialInlayProfileState>[0]) =>
      initialInlayProfileState(units, kind),
    [kind],
  );
  const { state, setState, units, unitsRef } = useFormState(initial, convertInlayProfileUnits);
  const setProfile = useCallback(
    (update: (current: ProfileFormState) => ProfileFormState) =>
      setState((current) => ({ ...current, profile: update(current.profile) })),
    [setState],
  );
  const load = useCallback(
    (params: StraightInlayParams, name: string) =>
      setState(inlayProfileStateFrom(params, name, kind, unitsRef.current)),
    [kind, setState, unitsRef],
  );
  const form = useToolpathForm('straightInlay', load, {
    setName: () => {
      const names = useToolpathStore.getState().toolpaths.map((t) => t.name);
      const name = nextToolpathName(baseNameOf(kind), names);
      setProfile((current) => ({ ...current, name }));
    },
  });
  const [showErrors, setShowErrors] = useState(false);
  const [editingTabs, setEditingTabs] = useState(false);
  const errors = useMemo(() => validateInlayProfile(state, kind, units), [kind, state, units]);
  const depthMm = useMemo(
    () => depthValues(state.profile.depths, units),
    [state.profile.depths, units],
  );
  const nameId = useId();
  const unit = unitSuffix(units);
  const shown = showErrors ? errors : {};

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = inlayProfileParams(state, kind, units);
    if (params !== null) {
      void form.calculate(params, state.profile.name);
    }
  };

  if (editingTabs) {
    return (
      <div className="inlay-profile">
        <EditTabsPanel
          errors={errors}
          ids={form.selectedIds}
          onChange={(change) => setProfile((current) => ({ ...current, ...change }))}
          onClose={() => setEditingTabs(false)}
          state={state.profile}
          units={units}
        />
      </div>
    );
  }

  return (
    <form className="inlay-profile" onSubmit={onSubmit}>
      <h3 className="inlay-profile__title">{KIND_TITLES[kind]}</h3>
      {form.editingName === null ? null : (
        <p className="inlay-profile__hint">{text.editing(form.editingName)}</p>
      )}
      {!form.hasVectors ? <p className="inlay-profile__notice">{text.needsSelection}</p> : null}
      <DepthFields
        errors={shown}
        onChange={(depths) => setProfile((current) => ({ ...current, depths }))}
        state={state.profile.depths}
        units={units}
      />
      <fieldset className="inlay-profile__group">
        <legend className="inlay-profile__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="endMill" tool={form.tool} units={units} />
        <PassesRow
          cutDepth={depthMm?.cutDepth ?? null}
          onChange={(passes) => setProfile((current) => ({ ...current, passes }))}
          startDepth={depthMm?.startDepth ?? null}
          state={state.profile.passes}
          toolPassDepth={form.tool?.passDepth ?? null}
          units={units}
        />
      </fieldset>
      <fieldset className="inlay-profile__group">
        <legend className="inlay-profile__legend">{own.machineVectors}</legend>
        <HelperRow
          picture={
            <HelperPicture
              picture={machineVectorsPicture(
                kind === 'femaleHole' ? 'inside' : 'outside',
                state.profile.direction,
              )}
            />
          }
          side="right"
        >
          <p className="inlay-profile__hint">{kind === 'femaleHole' ? own.inside : own.outside}</p>
          <ChoiceField
            label={text.direction}
            onChange={(direction) => setProfile((current) => ({ ...current, direction }))}
            options={DIRECTIONS}
            value={state.profile.direction}
          />
        </HelperRow>
        <NumberField
          error={shown.allowance}
          label={own.allowance}
          onChange={(allowance) => setProfile((current) => ({ ...current, allowance }))}
          unit={unit}
          value={state.profile.allowance}
        />
        {kind === 'maleStepped' ? (
          <>
            <NumberField
              error={shown.stepDepth}
              label={own.stepDepth}
              onChange={(stepDepth) => setState((current) => ({ ...current, stepDepth }))}
              unit={unit}
              value={state.stepDepth}
            />
            <NumberField
              error={shown.stepWidth}
              label={own.stepWidth}
              onChange={(stepWidth) => setState((current) => ({ ...current, stepWidth }))}
              unit={unit}
              value={state.stepWidth}
            />
          </>
        ) : null}
      </fieldset>
      <ProfileOptionPages
        errors={shown}
        onChange={setProfile}
        onEditTabs={() => setEditingTabs(true)}
        state={state.profile}
        units={units}
      />
      <SafeZReadout />
      <VectorSelectionField filter={form.selector} onChange={form.setSelector} />
      <label className="inlay-profile__name" htmlFor={nameId}>
        {text.name}
        <input
          className="inlay-profile__input"
          id={nameId}
          onChange={(event) => {
            const name = event.target.value;
            setProfile((current) => ({ ...current, name }));
          }}
          type="text"
          value={state.profile.name}
        />
      </label>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={form.calculateWhy}
        primaryLabel={text.calculate}
        submit
      />
    </form>
  );
}
