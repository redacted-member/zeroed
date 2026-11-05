import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import { getKernel } from '../../bridge/client';

import type { TextureParams } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDocumentStore } from '../../state/documentStore';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { ProjectField } from '../ProjectField/ProjectField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { SafeZReadout } from '../SafeZReadout/SafeZReadout';
import { SliderField } from '../SliderField/SliderField';
import { VectorSelectionField } from '../VectorSelectionField/VectorSelectionField';
import {
  MAX_STEPOVER_VARIATION,
  convertTextureUnits,
  initialTextureState,
  loadTextureSettings,
  partOf,
  textureParams,
  textureSettingsFrom,
  textureSettingsText,
  textureStateFrom,
  validateTexture,
  type TextureField,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './TextureToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { texturePattern } from '../HelperPicture/pictures/toolpaths';
import { HelperRow } from '../HelperRow/HelperRow';

const text = strings.toolpaths;
const degrees = strings.units.degrees;
const percent = text.texture.percent;
const SETTINGS_EXTENSIONS = ['json'];

function report(error: unknown): void {
  useDocumentStore.setState({
    lastError: error instanceof Error ? error.message : String(error),
  });
}

export function TextureToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(
    initialTextureState,
    convertTextureUnits,
  );
  const load = useCallback(
    (params: TextureParams, name: string) =>
      setState(textureStateFrom(params, name, unitsRef.current)),
    [setState, unitsRef],
  );
  const form = useToolpathForm('texture', load, {
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateTexture(state, units), [state, units]);
  const nameId = useId();
  const unit = unitSuffix(units);

  const scatter = !state.vectorsAsPattern;
  const field = (
    key: TextureField,
    label: string,
    suffix?: string,
    step?: number,
    disabled = false,
  ) => (
    <NumberField
      disabled={disabled}
      error={showErrors ? errors[key] : undefined}
      label={label}
      onChange={(value) => setState((current) => ({ ...current, [key]: value }))}
      step={step}
      unit={suffix}
      value={state[key]}
    />
  );

  const slider = (
    key: 'minDepth' | 'minLength' | 'overlapVariation' | 'stepoverVariation',
    label: string,
    max: number,
    readout: string,
    disabled = false,
  ) => (
    <SliderField
      disabled={disabled}
      label={label}
      max={max}
      min={key === 'minDepth' || key === 'minLength' ? 1 : 0}
      onChange={(value) => setState((current) => ({ ...current, [key]: String(value) }))}
      readout={readout}
      step={1}
      value={Number(state[key])}
    />
  );
  const lengthOf = (total: string, part: string) => {
    const length = partOf(total, part, units);
    return length === null ? '' : `${length} ${unit}`;
  };

  const saveSettings = async () => {
    const params = textureParams(state, units);
    if (params === null) {
      setShowErrors(true);
      return;
    }
    const path = await getKernel().chooseSavePath(
      text.texture.saveTitle,
      [{ name: text.texture.settingsFiles, extensions: SETTINGS_EXTENSIONS }],
      `${state.name.trim() === '' ? text.texture.title : state.name.trim()}.json`,
    );
    if (path !== null) {
      await api.io.saveText(path, textureSettingsText(params)).catch(report);
    }
  };

  const loadSettings = async () => {
    const path = await getKernel().chooseOpenPath(text.texture.loadTitle, [
      { name: text.texture.settingsFiles, extensions: SETTINGS_EXTENSIONS },
    ]);
    if (path === null) {
      return;
    }
    try {
      const params = textureSettingsFrom((await api.io.readText(path)).text);
      if (params === null) {
        report(text.texture.notSettings);
        return;
      }
      setState((current) => loadTextureSettings(current, params, units));
    } catch (error) {
      report(error);
    }
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = textureParams(state, units);
    if (params !== null) {
      void form.calculate(params, state.name);
    }
  };

  return (
    <form className="texture-toolpath" onSubmit={onSubmit}>
      <p className="texture-toolpath__hint">
        {form.editingName === null ? text.texture.hint : text.editing(form.editingName)}
      </p>
      {!form.hasVectors ? <p className="texture-toolpath__notice">{text.needsSelection}</p> : null}
      <fieldset className="texture-toolpath__group">
        <legend className="texture-toolpath__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="ballNose" tool={form.tool} units={units} />
      </fieldset>
      <fieldset className="texture-toolpath__group" aria-label={text.texture.startDepth}>
        <HelperRow picture={<HelperPicture picture={texturePattern} />}>
          {field('startDepth', text.texture.startDepth, unit)}
        </HelperRow>
      </fieldset>
      <fieldset className="texture-toolpath__group">
        <CheckField
          checked={state.vectorsAsPattern}
          label={text.texture.vectorsAsPattern}
          onChange={(vectorsAsPattern) => setState((current) => ({ ...current, vectorsAsPattern }))}
        />
        <CheckField
          checked={state.ramp}
          disabled={scatter}
          label={text.texture.ramp}
          onChange={(ramp) => setState((current) => ({ ...current, ramp }))}
        />
        <h3 className="texture-toolpath__heading">{text.texture.strokes}</h3>
        {field('depth', text.texture.depth, unit)}
        {slider('minDepth', text.texture.minDepth, 100, lengthOf(state.depth, state.minDepth))}
        {field('strokeLength', text.texture.strokeLength, unit, undefined, !scatter)}
        {slider(
          'minLength',
          text.texture.minLength,
          100,
          lengthOf(state.strokeLength, state.minLength),
          !scatter,
        )}
        {field('maxOverlap', text.texture.maxOverlap, percent, 5, !scatter)}
        {slider(
          'overlapVariation',
          text.texture.overlapVariation,
          100,
          `${state.overlapVariation} ${percent}`,
          !scatter,
        )}
        {field('spacing', text.texture.spacing, unit, undefined, !scatter)}
        {slider(
          'stepoverVariation',
          text.texture.stepoverVariation,
          MAX_STEPOVER_VARIATION,
          text.texture.plusMinus(lengthOf(state.spacing, state.stepoverVariation)),
          !scatter,
        )}
        {field('angle', text.texture.angle, degrees, 5, !scatter)}
        <div className="texture-toolpath__actions">
          <button
            className="texture-toolpath__button"
            onClick={() => void saveSettings()}
            type="button"
            data-tip={text.texture.save}
          >
            <ButtonFace icon="fileSave" label={text.texture.save} />
          </button>
          <button
            className="texture-toolpath__button"
            onClick={() => void loadSettings()}
            type="button"
            data-tip={text.texture.load}
          >
            <ButtonFace icon="fileOpen" label={text.texture.load} />
          </button>
        </div>
      </fieldset>
      <fieldset className="texture-toolpath__group">
        <legend className="texture-toolpath__legend">{text.texture.boundaryOffset}</legend>
        {field('boundaryOffset', text.texture.offset, unit, undefined, !scatter)}
      </fieldset>
      <SafeZReadout />
      <ProjectField checked={form.project} onChange={form.setProject} />
      <VectorSelectionField filter={form.selector} onChange={form.setSelector} />
      <label className="texture-toolpath__name" htmlFor={nameId}>
        {text.name}
        <input
          className="texture-toolpath__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={text.texture.title}
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
