import { useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { Path } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { usePreview } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { SliderField } from '../SliderField/SliderField';
import {
  convertVectorTextureUnits,
  initialVectorTextureState,
  validateVectorTexture,
  vectorTextureRequest,
  type VectorTextureField,
} from './model';

import './VectorTextureForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { vectorTexturePicture } from '../HelperPicture/pictures/drawing';

const text = strings.vectorTexture;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Create Vector Texture: rows of wavy lines inside the selected closed vectors or the job. */
export function VectorTextureForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const createTexture = useDocumentStore((store) => store.createVectorTexture);
  const previewTexture = useDocumentStore((store) => store.previewVectorTexture);
  const { state, setState, units } = useFormState(
    initialVectorTextureState,
    convertVectorTextureUnits,
  );
  const [showErrors, setShowErrors] = useState(false);
  const [preview, setPreview] = useState<Path[]>([]);
  const [failure, setFailure] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const layerId = useId();
  const errors = useMemo(() => validateVectorTexture(state, units), [state, units]);
  const request = useMemo(() => vectorTextureRequest(state, units), [state, units]);
  usePreview(preview);

  const setField = (field: VectorTextureField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: VectorTextureField) => (showErrors ? errors[field] : undefined);
  const unit = unitSuffix(units);

  const run = (makePreview: boolean) => {
    setShowErrors(true);
    if (request === null || busy) {
      return;
    }
    setBusy(true);
    setFailure(null);
    const work = makePreview
      ? previewTexture(request).then((paths) => setPreview(paths))
      : createTexture(request).then(() => setPreview([]));
    work.catch((caught: unknown) => setFailure(message(caught))).finally(() => setBusy(false));
  };

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    run(false);
  };

  return (
    <form className="vector-texture-form" onSubmit={onSubmit}>
      <p className="vector-texture-form__hint">{text.hint}</p>
      <fieldset className="vector-texture-form__group">
        <legend className="vector-texture-form__legend">{text.angle}</legend>
        <NumberField
          error={errorFor('angle')}
          label={text.angle}
          onChange={(v) => setField('angle', v)}
          unit={text.degrees}
          value={state.angle}
        />
      </fieldset>
      <fieldset className="vector-texture-form__group">
        <legend className="vector-texture-form__legend">{text.lineSpacing}</legend>
        <NumberField
          error={errorFor('maxSpacing')}
          label={text.maxSpacing}
          onChange={(v) => setField('maxSpacing', v)}
          unit={unit}
          value={state.maxSpacing}
        />
        <SliderField
          label={text.spacingVariation}
          max={100}
          min={0}
          onChange={(spacingVariation) => setState((c) => ({ ...c, spacingVariation }))}
          unit="%"
          value={state.spacingVariation}
        />
      </fieldset>
      <fieldset className="vector-texture-form__group">
        <legend className="vector-texture-form__legend">{text.waves}</legend>
        <HelperRow
          picture={<HelperPicture picture={vectorTexturePicture(Number(state.amplitude) > 0)} />}
        >
          <NumberField
            error={errorFor('wavelength')}
            label={text.wavelength}
            onChange={(v) => setField('wavelength', v)}
            unit={unit}
            value={state.wavelength}
          />
          <NumberField
            error={errorFor('amplitude')}
            label={text.amplitude}
            onChange={(v) => setField('amplitude', v)}
            unit={unit}
            value={state.amplitude}
          />
        </HelperRow>
      </fieldset>
      <fieldset className="vector-texture-form__group">
        <legend className="vector-texture-form__legend">{text.noise}</legend>
        <SliderField
          label={text.noise}
          max={100}
          min={0}
          onChange={(noise) => setState((c) => ({ ...c, noise }))}
          unit="%"
          value={state.noise}
        />
      </fieldset>
      <fieldset className="vector-texture-form__group">
        <legend className="vector-texture-form__legend">{text.layer}</legend>
        <CheckField
          checked={state.onLayer}
          label={text.onLayer}
          onChange={(onLayer) => setState((c) => ({ ...c, onLayer }))}
        />
        <label className="vector-texture-form__label" htmlFor={layerId}>
          {text.layerName}
        </label>
        <input
          className="vector-texture-form__input"
          disabled={!state.onLayer}
          id={layerId}
          onChange={(event) => {
            const layerName = event.target.value;
            setState((c) => ({ ...c, layerName }));
          }}
          type="text"
          value={state.layerName}
        />
      </fieldset>
      {failure === null ? null : (
        <p className="vector-texture-form__error" role="alert">
          {failure}
        </p>
      )}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        onSecondary={() => run(true)}
        primaryWhy={busy ? strings.why.busy : undefined}
        primaryLabel={text.apply}
        secondaryLabel={text.preview}
        submit
      />
    </form>
  );
}
