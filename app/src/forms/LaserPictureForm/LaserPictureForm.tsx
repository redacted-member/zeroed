import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { LaserPictureParams, PictureStrategy } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { numberValue } from '../../lib/formFields';
import { unitSuffix } from '../../lib/units';
import { startFrom, type Remembered } from '../../lib/rememberedForm';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useDocumentStore } from '../../state/documentStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { ImmediateOutput } from '../ImmediateOutput/ImmediateOutput';
import { NumberField } from '../NumberField/NumberField';
import { ProjectField } from '../ProjectField/ProjectField';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { SliderBoxField } from '../SliderBoxField/SliderBoxField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import {
  convertLaserPictureUnits,
  initialLaserPictureState,
  laserPictureParams,
  laserPictureStateFrom,
  lineSpacingRange,
  validateLaserPicture,
  type LaserPictureField,
  type LaserPictureState,
} from './model';
import { firstWhy } from '../../shell/Tooltip/model';

import './LaserPictureForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { laserPicturePicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;
const laser = text.laser;

const STRATEGIES: ReadonlyArray<Choice<PictureStrategy>> = [
  { value: 'raster', label: laser.raster },
  { value: 'hatch', label: laser.hatch },
  { value: 'vectors', label: laser.selectedVectors },
];

/** The fields as last calculated, for the next new form. */
let lastPicture: Remembered<LaserPictureState> | null = null;

function initialState(units: Parameters<typeof initialLaserPictureState>[0]): LaserPictureState {
  // An older toolpath's image file belongs to that toolpath.
  return startFrom(
    lastPicture,
    units,
    initialLaserPictureState,
    convertLaserPictureUnits,
    (state) => ({ ...state, name: '', legacy: null }),
  );
}

export function LaserPictureForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(initialState, convertLaserPictureUnits);
  const load = useCallback(
    (params: LaserPictureParams, name: string) =>
      setState(laserPictureStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('laserPicture', load, {
    requireSelection: false,
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const hasBitmap = useDocumentStore((store) =>
    store.selection.ids.some((id) => store.entities.find((e) => e.id === id)?.kind === 'bitmap'),
  );
  const [showErrors, setShowErrors] = useState(false);
  // The toolpath this form calculated last, for Immediate Output.
  const [lastId, setLastId] = useState<number | null>(() => useUiStore.getState().editingToolpath);
  const errors = useMemo(() => validateLaserPicture(state, units), [state, units]);
  const nameId = useId();
  const unit = unitSuffix(units);
  const isLaser = form.tool?.toolType.kind === 'laserTool';
  const spacing = lineSpacingRange(units);
  const canBurn = hasBitmap || state.legacy !== null;

  const set = (field: LaserPictureField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: LaserPictureField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = laserPictureParams(state, units);
    if (params === null || !isLaser || !canBurn) {
      return;
    }
    const typed = state;
    void form.calculate(params, state.name, { keepOpen: true }).then((ok) => {
      if (ok) {
        lastPicture = { state: typed, units };
      }
      const id = useToolpathStore.getState().selectedId;
      if (ok && id !== null) {
        setLastId(id);
        useUiStore.setState({ editingToolpath: id });
      }
    });
  };

  return (
    <form className="laser-picture" onSubmit={onSubmit}>
      <p className="laser-picture__hint">
        {form.editingName === null ? laser.pictureHint : text.editing(form.editingName)}
      </p>
      {canBurn ? null : <p className="laser-picture__notice">{laser.needsBitmap}</p>}
      <fieldset className="laser-picture__group">
        <legend className="laser-picture__legend">{laser.toolSection}</legend>
        <ToolPicker onChange={form.setTool} prefer="laserTool" tool={form.tool} units={units} />
        {form.tool !== null && !isLaser ? (
          <p className="laser-picture__notice">{laser.notLaser}</p>
        ) : null}
        <NumberField
          error={errorFor('power')}
          label={laser.power}
          onChange={(v) => set('power', v)}
          unit={laser.percent}
          value={state.power}
        />
        <NumberField
          error={errorFor('speed')}
          label={laser.speed}
          onChange={(v) => set('speed', v)}
          unit={laser.perMinute(unit)}
          value={state.speed}
        />
        <p className="laser-picture__hint">{laser.toolSetting}</p>
        <NumberField
          error={errorFor('repeats')}
          label={laser.picturePasses}
          onChange={(v) => set('repeats', v)}
          step={1}
          value={state.repeats}
        />
        <NumberField
          error={errorFor('overscan')}
          label={laser.overscan}
          onChange={(v) => set('overscan', v)}
          unit={laser.percent}
          value={state.overscan}
        />
      </fieldset>
      <fieldset className="laser-picture__group">
        <legend className="laser-picture__legend">{laser.strategy}</legend>
        <HelperRow picture={<HelperPicture picture={laserPicturePicture(state.strategy)} />}>
          <ChoiceField
            label={laser.strategy}
            onChange={(strategy) => setState((current) => ({ ...current, strategy }))}
            options={STRATEGIES}
            value={state.strategy}
          />
        </HelperRow>
        <CheckField
          checked={state.ditherImage}
          label={laser.ditherImage}
          onChange={(ditherImage) => setState((current) => ({ ...current, ditherImage }))}
        />
        <SliderBoxField
          endLabel={laser.sparse}
          label={laser.lineSpacing}
          max={spacing.max}
          min={spacing.min}
          onChange={(v) => set('lineSpacing', String(v))}
          startLabel={laser.dense}
          step={spacing.step}
          unit={unit}
          value={numberValue(state.lineSpacing) ?? spacing.min}
        />
        <SliderBoxField
          disabled={state.strategy === 'vectors'}
          endLabel="90"
          label={laser.lineAngle}
          max={90}
          min={-90}
          onChange={(v) => set('lineAngle', String(v))}
          startLabel="-90"
          step={1}
          unit={strings.units.degrees}
          value={numberValue(state.lineAngle) ?? 0}
        />
      </fieldset>
      <SafeZReadout />
      <ProjectField checked={form.project} onChange={form.setProject} />
      <label className="laser-picture__name" htmlFor={nameId}>
        {text.name}
        <input
          className="laser-picture__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={laser.pictureTitle}
          type="text"
          value={state.name}
        />
      </label>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy(
          [!canBurn, laser.needsBitmap],
          [form.tool === null, text.needsTool],
          [!isLaser, laser.notLaser],
        )}
        primaryLabel={text.calculate}
        submit
      />
      <ImmediateOutput toolpathId={lastId} />
    </form>
  );
}
