import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { CutDirection, ProfileParams, ProfileSide } from '../../bridge/cam';
import { depthValues } from '../../lib/toolpathFields';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { DepthFields } from '../DepthFields/DepthFields';
import { EditTabsPanel } from '../EditTabsPanel/EditTabsPanel';
import { FormButtons } from '../FormButtons/FormButtons';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { lastPass, machineVectorsPicture } from '../HelperPicture/pictures/toolpaths';
import { HelperRow } from '../HelperRow/HelperRow';
import { NumberField } from '../NumberField/NumberField';
import { PassesRow } from '../PassesRow/PassesRow';
import { ProfileOptionPages } from '../ProfileOptionPages/ProfileOptionPages';
import { ProjectField } from '../ProjectField/ProjectField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import type { initialProfileState } from './model';
import {
  convertProfileUnits,
  newProfileState,
  profileThroughDepth,
  type RememberedProfile,
  profileParams,
  profileStateFrom,
  validateProfile,
  type ProfileField,
  type ProfileFormState,
} from './model';

import './ProfileToolpathForm.scss';

const text = strings.toolpaths;

const SIDES: ReadonlyArray<Choice<ProfileSide>> = [
  { value: 'outside', label: text.profile.outside },
  { value: 'inside', label: text.profile.inside },
  { value: 'on', label: text.profile.on },
];

/** Show advanced toolpath options, kept for the next Profile form. */
let showAdvanced = true;

/** The form as last calculated, for the next new Profile form. */
let lastProfile: RememberedProfile | null = null;

function initialState(units: Parameters<typeof initialProfileState>[0]): ProfileFormState {
  const through = profileThroughDepth(useDocumentStore.getState().job);
  return { ...newProfileState(lastProfile, units, through), advanced: showAdvanced };
}

export const DIRECTIONS: ReadonlyArray<Choice<CutDirection>> = [
  { value: 'climb', label: text.climb },
  { value: 'conventional', label: text.conventional },
];

export function ProfileToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(initialState, convertProfileUnits);
  const load = useCallback(
    (params: ProfileParams, name: string) =>
      setState(profileStateFrom(params, name, unitsRef.current, showAdvanced)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('profile', load, {
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  const [editingTabs, setEditingTabs] = useState(false);
  const patch = useCallback(
    (change: Partial<ProfileFormState>) => setState((current) => ({ ...current, ...change })),
    [setState],
  );
  const errors = useMemo(() => validateProfile(state, units), [state, units]);
  const depthMm = useMemo(() => depthValues(state.depths, units), [state.depths, units]);
  const nameId = useId();
  const unit = unitSuffix(units);

  const set = (field: ProfileField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: ProfileField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = profileParams(state, units);
    if (params !== null) {
      const typed = state;
      void form.calculate(params, state.name).then((ok) => {
        if (ok) {
          lastProfile = { state: typed, units };
        }
      });
    }
  };

  const vBit = form.tool?.toolType.kind === 'vBit';

  if (editingTabs) {
    return (
      <div className="profile-toolpath">
        <EditTabsPanel
          errors={errors}
          ids={form.selectedIds}
          onChange={patch}
          onClose={() => setEditingTabs(false)}
          state={state}
          units={units}
        />
      </div>
    );
  }

  return (
    <form className="profile-toolpath" onSubmit={onSubmit}>
      {form.editingName === null ? null : (
        <p className="profile-toolpath__hint">{text.editing(form.editingName)}</p>
      )}
      {!form.hasVectors ? <p className="profile-toolpath__notice">{text.needsSelection}</p> : null}
      <DepthFields
        errors={showErrors ? errors : {}}
        onChange={(depths) => setState((current) => ({ ...current, depths }))}
        state={state.depths}
        units={units}
      />
      <CheckField
        checked={state.advanced}
        label={text.profile.showAdvanced}
        onChange={(advanced) => {
          showAdvanced = advanced;
          setState((current) => ({ ...current, advanced }));
        }}
      />
      <fieldset className="profile-toolpath__group">
        <legend className="profile-toolpath__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} tool={form.tool} units={units} />
        <PassesRow
          cutDepth={depthMm?.cutDepth ?? null}
          onChange={(passes) => setState((current) => ({ ...current, passes }))}
          startDepth={depthMm?.startDepth ?? null}
          state={state.passes}
          toolPassDepth={form.tool?.passDepth ?? null}
          units={units}
        />
      </fieldset>
      <fieldset className="profile-toolpath__group" aria-label={text.profile.machineVectors}>
        <HelperRow
          picture={<HelperPicture picture={machineVectorsPicture(state.side, state.direction)} />}
          side="right"
        >
          <ChoiceField
            label={text.profile.machineVectors}
            onChange={(side) => setState((current) => ({ ...current, side }))}
            options={SIDES}
            value={state.side}
          />
        </HelperRow>
        <ChoiceField
          label={text.direction}
          onChange={(direction) => setState((current) => ({ ...current, direction }))}
          options={DIRECTIONS}
          value={state.direction}
        />
        <NumberField
          error={errorFor('allowance')}
          label={text.allowance}
          onChange={(v) => set('allowance', v)}
          unit={unit}
          value={state.allowance}
        />
        <p className="profile-toolpath__hint">{text.profile.openHint}</p>
      </fieldset>
      {state.advanced ? (
        <>
          <fieldset className="profile-toolpath__group" aria-label={text.profile.lastPass}>
            <HelperRow picture={<HelperPicture picture={lastPass} />}>
              <CheckField
                checked={state.lastPass}
                label={text.profile.doLastPass}
                onChange={(lastPass) => setState((current) => ({ ...current, lastPass }))}
              />
              <NumberField
                disabled={!state.lastPass}
                error={errorFor('lastPassAllowance')}
                label={text.profile.lastPassAllowance}
                onChange={(v) => set('lastPassAllowance', v)}
                unit={unit}
                value={state.lastPassAllowance}
              />
              <CheckField
                checked={state.reverseLastPass}
                disabled={!state.lastPass}
                label={text.profile.reverseLastPass}
                onChange={(reverseLastPass) =>
                  setState((current) => ({ ...current, reverseLastPass }))
                }
              />
            </HelperRow>
          </fieldset>
          <ProfileOptionPages
            corners={{ vBit }}
            errors={showErrors ? errors : {}}
            onChange={setState}
            onEditTabs={() => setEditingTabs(true)}
            state={state}
            units={units}
          />
        </>
      ) : null}
      <SafeZReadout />
      <ProjectField checked={form.project} onChange={form.setProject} />
      <VectorSelectionField filter={form.selector} onChange={form.setSelector} />
      <label className="profile-toolpath__name" htmlFor={nameId}>
        {text.name}
        <input
          className="profile-toolpath__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={text.profile.title}
          type="text"
          value={state.name}
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
