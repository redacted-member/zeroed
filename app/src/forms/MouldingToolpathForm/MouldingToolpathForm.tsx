import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { MouldingParams } from '../../bridge/cam';
import type { VectorFilter } from '../../bridge/sketch';
import { lengthText } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { usePreview } from '../../lib/useDrawController';
import { useEntityPaths } from '../../lib/useEntityPaths';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { isAutomatic } from '../VectorSelectionField/model';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import {
  convertMouldingUnits,
  cutDepth,
  gapAboveOf,
  initialMouldingState,
  mouldingArgs,
  mouldingPreview,
  mouldingStateFrom,
  otherGap,
  profileHeight,
  railsAndProfile,
  validateMoulding,
  withPosition,
  withSlider,
  type MouldingContext,
  type MouldingField,
  type MouldingPosition,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './MouldingToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { mouldingPosition } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;
const mould = text.moulding;

export function MouldingToolpathForm(): JSX.Element {
  const selectedIds = useDocumentStore((store) => store.selection.ids);
  const entities = useDocumentStore((store) => store.entities);
  const thickness = useDocumentStore((store) => store.job.thickness);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(
    initialMouldingState,
    convertMouldingUnits,
  );
  const load = useCallback(
    (params: MouldingParams & { profileSelector?: VectorFilter | null }, name: string) =>
      setState(mouldingStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('moulding', load, {
    requireSelection: false,
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  const nameId = useId();
  const sliderId = useId();
  const unit = unitSuffix(units);
  const picked = useMemo(() => railsAndProfile(selectedIds, entities), [selectedIds, entities]);
  const railAutomatic = isAutomatic(form.selector);
  const context: MouldingContext = {
    ...picked,
    automatic: railAutomatic,
    thickness,
    profileHeight: profileHeight(picked.profile, entities, state.kept.heightScale),
  };
  const errors = validateMoulding(state, units, context);
  const ids = useMemo(
    () => [...picked.rails, picked.profile].filter((id): id is number => id !== null),
    [picked],
  );
  const paths = useEntityPaths(ids);
  usePreview(useMemo(() => mouldingPreview(picked, paths), [picked, paths]));

  const errorFor = (field: MouldingField) => (showErrors ? errors[field] : undefined);
  const setText = (field: MouldingField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const setPosition = (position: MouldingPosition) =>
    setState((current) => withPosition(current, position, unitsRef.current, context));
  const room = Math.max(0, thickness - (context.profileHeight ?? 0));
  const above = gapAboveOf(state, units, context);
  const depth = cutDepth(state, units, context);
  const clearance = state.useClearance;

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const args = mouldingArgs(
      { ...state, profileSelector: railAutomatic ? state.profileSelector : null },
      units,
      context,
    );
    if (args !== null) {
      void form.calculate(args.params, state.name, { ids: [...args.ids] });
    }
  };

  const gapField = (position: MouldingPosition) => {
    const active = state.position === position;
    const field = position === 'above' ? 'gapAbove' : 'gapBelow';
    return (
      <div className="moulding-toolpath__gap">
        <input
          aria-label={position === 'above' ? mould.gapAbove : mould.gapBelow}
          checked={active}
          className="moulding-toolpath__radio"
          name={`${sliderId}-position`}
          onChange={() => setPosition(position)}
          type="radio"
        />
        <NumberField
          disabled={!active}
          error={active ? errorFor(field) : undefined}
          label={position === 'above' ? mould.gapAbove : mould.gapBelow}
          onChange={(v) => setText(field, v)}
          unit={unit}
          value={active ? state[field] : otherGap(state, units, context)}
        />
      </div>
    );
  };

  const railStatus = errorFor('rail') ?? errorFor('profile');

  const clearWhy = selectedIds.length === 0 ? strings.why.noRailPicked : undefined;
  return (
    <form className="moulding-toolpath" onSubmit={onSubmit}>
      {form.editingName === null ? null : (
        <p className="moulding-toolpath__hint">{text.editing(form.editingName)}</p>
      )}
      <fieldset className="moulding-toolpath__group">
        <legend className="moulding-toolpath__legend">{mould.driveRail}</legend>
        <p className="moulding-toolpath__hint">{mould.railSteps}</p>
        <p
          className={
            railStatus === undefined
              ? 'moulding-toolpath__count'
              : 'moulding-toolpath__count moulding-toolpath__count--error'
          }
          role="status"
        >
          {railStatus ??
            (picked.rails.length === 0
              ? ''
              : mould.railsPicked(picked.rails.length, picked.profile !== null))}
        </p>
        <button
          className="moulding-toolpath__button"
          disabled={clearWhy !== undefined}
          onClick={() => void useDocumentStore.getState().select([])}
          type="button"
          data-tip={whyTip(mould.clearRail, clearWhy)}
        >
          <ButtonFace icon="editDelete" label={mould.clearRail} />
        </button>
      </fieldset>
      <fieldset className="moulding-toolpath__group">
        <legend className="moulding-toolpath__legend">{mould.position}</legend>
        <HelperRow picture={<HelperPicture picture={mouldingPosition} />}>
          <div className="moulding-toolpath__position">
            <input
              aria-label={mould.positionSlider}
              className="moulding-toolpath__slider"
              id={sliderId}
              max={room}
              min={0}
              onChange={(event) =>
                setState((current) =>
                  withSlider(current, Number(event.target.value), unitsRef.current, context),
                )
              }
              step={room / 100 || 1}
              type="range"
              value={Math.min(room, Math.max(0, above ?? 0))}
            />
            <div className="moulding-toolpath__gaps">
              {gapField('above')}
              {gapField('below')}
            </div>
          </div>
        </HelperRow>
        <p className="moulding-toolpath__readout">
          {mould.cutDepth}:{' '}
          <output htmlFor={sliderId}>
            {depth === null ? '' : `${lengthText(depth, units)} ${unit}`}
          </output>
        </p>
      </fieldset>
      <fieldset className="moulding-toolpath__group">
        <legend className="moulding-toolpath__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="ballNose" tool={form.tool} units={units} />
        <div className="moulding-toolpath__checks">
          <CheckField
            checked={state.varyStepover}
            label={mould.varyStepover}
            onChange={(varyStepover) => setState((current) => ({ ...current, varyStepover }))}
          />
          <CheckField
            checked={state.skipFlats}
            disabled={!clearance || !state.machineFlats}
            label={mould.skipFlats}
            onChange={(skipFlats) => setState((current) => ({ ...current, skipFlats }))}
          />
        </div>
      </fieldset>
      <fieldset className="moulding-toolpath__group">
        <legend className="moulding-toolpath__check-legend">
          <CheckField
            checked={clearance}
            label={mould.clearance}
            onChange={(useClearance) => setState((current) => ({ ...current, useClearance }))}
          />
        </legend>
        {clearance ? (
          <ToolPicker
            onChange={(clearanceTool) => setState((current) => ({ ...current, clearanceTool }))}
            prefer="endMill"
            tool={state.clearanceTool}
            units={units}
          />
        ) : (
          <p className="moulding-toolpath__hint">{mould.noClearanceTool}</p>
        )}
        <CheckField
          checked={state.machineFlats}
          disabled={!clearance}
          label={mould.machineFlats}
          onChange={(machineFlats) => setState((current) => ({ ...current, machineFlats }))}
        />
        <CheckField
          checked={state.clearanceRamp}
          disabled={!clearance}
          label={mould.clearanceRamp}
          onChange={(clearanceRamp) => setState((current) => ({ ...current, clearanceRamp }))}
        />
        <NumberField
          disabled={!clearance || !state.clearanceRamp}
          error={errorFor('clearanceRampDistance')}
          label={mould.clearanceRampDistance}
          onChange={(v) => setText('clearanceRampDistance', v)}
          unit={unit}
          value={state.clearanceRampDistance}
        />
        <NumberField
          disabled={!clearance}
          error={errorFor('clearanceAllowance')}
          label={mould.clearanceAllowance}
          onChange={(v) => setText('clearanceAllowance', v)}
          unit={unit}
          value={state.clearanceAllowance}
        />
      </fieldset>
      <fieldset className="moulding-toolpath__group" aria-label={mould.sharpCorners}>
        <CheckField
          checked={state.sharpCorners}
          label={mould.sharpCorners}
          onChange={(sharpCorners) => setState((current) => ({ ...current, sharpCorners }))}
        />
      </fieldset>
      <fieldset className="moulding-toolpath__group" aria-label={mould.boundaryOffset}>
        <NumberField
          disabled={state.autoBoundaryOffset}
          error={errorFor('boundaryOffset')}
          label={mould.boundaryOffset}
          onChange={(v) => setText('boundaryOffset', v)}
          unit={unit}
          value={state.boundaryOffset}
        />
        <CheckField
          checked={state.autoBoundaryOffset}
          label={mould.autoBoundaryOffset}
          onChange={(autoBoundaryOffset) =>
            setState((current) => ({ ...current, autoBoundaryOffset }))
          }
        />
      </fieldset>
      <SafeZReadout />
      <VectorSelectionField
        filter={form.selector}
        label={mould.railSelection}
        onChange={form.setSelector}
      />
      <VectorSelectionField
        offWhy={railAutomatic ? undefined : strings.why.railAutomatic}
        filter={railAutomatic ? state.profileSelector : null}
        label={mould.profileSelection}
        onChange={(profileSelector) => setState((current) => ({ ...current, profileSelector }))}
      />
      <label className="moulding-toolpath__name" htmlFor={nameId}>
        {text.name}
        <input
          className="moulding-toolpath__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={mould.title}
          type="text"
          value={state.name}
        />
      </label>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy(
          [form.tool === null, text.needsTool],
          [clearance && state.clearanceTool === null, strings.why.clearanceTool],
        )}
        primaryLabel={text.calculate}
        submit
      />
    </form>
  );
}
