import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { PhotoStrategy, PhotoCarveParams } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { ProjectField } from '../ProjectField/ProjectField';
import { SliderBoxField } from '../SliderBoxField/SliderBoxField';
import { SliderField } from '../SliderField/SliderField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import {
  convertPhotoUnits,
  LINE_SPACING,
  initialPhotoState,
  photoParams,
  photoStateFrom,
  validatePhoto,
  type PhotoField,
} from './model';
import { firstWhy } from '../../shell/Tooltip/model';

import './PhotoCarveToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { photoPicture } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;
const photo = strings.toolpaths.photo;

const STRATEGIES: ReadonlyArray<Choice<PhotoStrategy>> = [
  { value: 'raster', label: photo.raster },
  { value: 'hatch', label: photo.hatch },
  { value: 'vectors', label: photo.vectors },
];

export function PhotoCarveToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(initialPhotoState, convertPhotoUnits);
  const load = useCallback(
    (params: PhotoCarveParams, name: string) =>
      setState(photoStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('photoCarve', load, {
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const hasBitmap = useDocumentStore((store) =>
    store.selection.ids.some((id) => store.entities.find((e) => e.id === id)?.kind === 'bitmap'),
  );
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validatePhoto(state, units), [state, units]);
  const nameId = useId();
  const unit = unitSuffix(units);
  const set = (field: PhotoField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: PhotoField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = photoParams(state, units);
    if (params !== null && hasBitmap) {
      void form.calculate(params, state.name);
    }
  };

  return (
    <form className="photo-carve" onSubmit={onSubmit}>
      <p className="photo-carve__hint">
        {form.editingName === null ? photo.hint : text.editing(form.editingName)}
      </p>
      {hasBitmap ? null : <p className="photo-carve__notice">{photo.needsBitmap}</p>}
      <fieldset className="photo-carve__group">
        <legend className="photo-carve__legend">{text.depths}</legend>
        <NumberField
          error={errorFor('startDepth')}
          label={text.startDepth}
          onChange={(v) => set('startDepth', v)}
          unit={unit}
          value={state.startDepth}
        />
        <NumberField
          error={errorFor('maxDepth')}
          label={photo.maxDepth}
          onChange={(v) => set('maxDepth', v)}
          unit={unit}
          value={state.maxDepth}
        />
        <NumberField
          error={errorFor('stepoverRetract')}
          label={photo.stepoverRetract}
          onChange={(v) => set('stepoverRetract', v)}
          unit={unit}
          value={state.stepoverRetract}
        />
      </fieldset>
      <fieldset className="photo-carve__group">
        <legend className="photo-carve__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="vBit" tool={form.tool} units={units} />
      </fieldset>
      <fieldset className="photo-carve__group" aria-label={photo.strategy}>
        <HelperRow picture={<HelperPicture picture={photoPicture(state.strategy)} />}>
          <ChoiceField
            label={photo.strategy}
            onChange={(strategy) => setState((current) => ({ ...current, strategy }))}
            options={STRATEGIES}
            value={state.strategy}
          />
        </HelperRow>
        <SliderBoxField
          disabled={state.strategy === 'vectors'}
          endLabel={photo.sparse}
          label={photo.lineSpacing}
          max={LINE_SPACING.max}
          min={LINE_SPACING.min}
          onChange={(v) => set('lineSpacing', String(v))}
          startLabel={photo.dense}
          step={1}
          unit="%"
          value={Number(state.lineSpacing)}
        />
        <SliderField
          disabled={state.strategy === 'vectors'}
          label={photo.lineAngle}
          max={90}
          min={-90}
          onChange={(lineAngle) => setState((current) => ({ ...current, lineAngle }))}
          step={1}
          unit={strings.units.degrees}
          value={state.lineAngle}
        />
        <p className="photo-carve__hint">{photo.lineSpacingHint}</p>
      </fieldset>
      <SafeZReadout />
      <ProjectField checked={form.project} onChange={form.setProject} />
      <label className="photo-carve__name" htmlFor={nameId}>
        {text.name}
        <input
          className="photo-carve__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={photo.title}
          type="text"
          value={state.name}
        />
      </label>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy([!hasBitmap, photo.needsBitmap], [form.tool === null, text.needsTool])}
        primaryLabel={text.calculate}
        submit
      />
    </form>
  );
}
