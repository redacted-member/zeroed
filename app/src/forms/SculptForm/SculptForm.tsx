import { useEffect, useId, useMemo, useState, type JSX, type MouseEvent } from 'react';

import { api } from '../../bridge/formApi';
import { getKernel } from '../../bridge/client';
import type { Brush, SculptMode } from '../../bridge/model';
import { strings } from '../../lib/formStrings';
import { lengthValue, numberValue } from '../../lib/formFields';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { pickedComponents, useModelStore } from '../../state/modelStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useSculptStore } from '../../state/sculptStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { NumberField } from '../NumberField/NumberField';
import { SliderField } from '../SliderField/SliderField';
import {
  addPreset,
  adjustSculpt,
  BRUSH_EXTENSIONS,
  brushFileName,
  BRUSHES,
  FILE_BRUSH,
  brushParams,
  buildsOrCuts,
  convertSculptUnits,
  customBrushParams,
  hasPreset,
  initialSculptState,
  MODES,
  presetSettings,
  removePreset,
  toolForKey,
  validateSculpt,
  withBrushFile,
  withPreset,
  type SculptField,
} from './model';

import './SculptForm.scss';
import { modalDialogOpen } from '../../lib/dialogKeys';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';
import { sculptPicture } from '../HelperPicture/pictures/modeling';

const text = strings.model.sculpt;

const BRUSH_CHOICES: ReadonlyArray<Choice<Brush>> = BRUSHES.map((value) => ({
  value,
  label: text[value],
}));

const MODE_CHOICES: ReadonlyArray<Choice<SculptMode>> = MODES.map((value) => ({
  value,
  label: text.modes[value],
  picture: sculptPicture(value),
}));

/** Turns sculpting on in the 3D view for as long as the form is open. */
function useSculptMode(): void {
  useEffect(() => {
    const ui = useUiStore.getState();
    const previous = ui.viewMode;
    if (previous === '2d') {
      ui.setViewMode('3d');
    }
    useSculptStore.getState().setActive(true);
    return () => {
      useSculptStore.getState().setActive(false);
      useSculptStore.getState().setOverlay(null);
      void api.model.sculptEnd().catch(() => undefined);
      if (previous === '2d' && useUiStore.getState().viewMode === '3d') {
        useUiStore.getState().setViewMode('2d');
      }
    };
  }, []);
}

/** Several components picked: they are baked into one to sculpt, after
 * asking. A bitmap among the selected vectors is laid over the model. */
function useSculptTarget(): void {
  useEffect(() => {
    const model = useModelStore.getState();
    const picked = pickedComponents(model);
    if (picked.length > 1 && window.confirm(text.bakeConfirm(picked.length))) {
      void api.model.bake(picked).then(async (reply) => {
        await useDocumentStore.getState().refresh();
        await useModelStore.getState().refresh();
        useModelStore.getState().setPicked([reply.id]);
        useModelStore.getState().select(reply.id);
      });
    }
    const doc = useDocumentStore.getState();
    const bitmap = doc.entities.find(
      (e) => doc.selection.ids.includes(e.id) && e.kind === 'bitmap',
    );
    if (bitmap !== undefined) {
      void api.entities.get(bitmap.id).then((entity) => {
        if (entity.kind === 'bitmap') {
          useSculptStore
            .getState()
            .setOverlay({ imageId: entity.imageId, quad: entity.quad, opacity: 0.5, shown: true });
        }
      });
    }
  }, []);
}

export function SculptForm(): JSX.Element {
  const component = useModelStore(
    (store) => store.components.find((c) => c.id === store.selectedId) ?? null,
  );
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(initialSculptState, convertSculptUnits);
  const errors = useMemo(() => validateSculpt(state, units), [state, units]);
  const brush = useMemo(() => brushParams(state, units), [state, units]);
  useSculptMode();
  useSculptTarget();

  useEffect(() => {
    useSculptStore.getState().setBrush(brush);
  }, [brush]);

  // The custom brush: another component's shape.
  const custom = useMemo(() => customBrushParams(state), [state]);
  useEffect(() => {
    useSculptStore.getState().setCustom(custom);
  }, [custom]);
  useEffect(() => () => useSculptStore.getState().setCustom(null), []);
  const brushChoices = useModelStore((store) => store.components);
  const brushListId = useId();
  const presetId = useId();

  // Load Brush: a clip or a picture, looked for in the brushes folder.
  const loadBrush = async () => {
    const folder = await api.model
      .brushFolder()
      .then((reply) => `${reply.path}/`)
      .catch(() => undefined);
    const path = await getKernel().chooseOpenPath(
      text.loadTitle,
      [{ name: text.brushFiles, extensions: [...BRUSH_EXTENSIONS] }],
      folder,
    );
    if (path !== null) {
      setState((current) => withBrushFile(current, path));
    }
  };

  // The mouse wheel in the 3D view sizes the brush and sets its strength.
  useEffect(
    () =>
      useSculptStore.subscribe((store, previous) => {
        const adjust = store.adjust;
        if (adjust !== null && adjust !== previous.adjust) {
          setState((current) => adjustSculpt(current, adjust, units));
        }
      }),
    [setState, units],
  );

  // 1 to 5 pick the tool, 0 toggles Preserve Transparency, O the overlay.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing =
        target !== null &&
        (target.tagName === 'INPUT' || target.tagName === 'SELECT' || target.isContentEditable);
      if (typing || modalDialogOpen() || event.ctrlKey || event.metaKey || event.altKey) {
        return;
      }
      const tool = toolForKey(event.key);
      const key = event.key.toLowerCase();
      if (tool !== null) {
        setState((current) => ({ ...current, brush: tool }));
      } else if (key === '0') {
        setState((current) => ({ ...current, preserve: !current.preserve }));
      } else if (key === 'o' && useSculptStore.getState().overlay !== null) {
        const overlay = useSculptStore.getState().overlay;
        if (overlay !== null) {
          useSculptStore.getState().setOverlay({ ...overlay, shown: !overlay.shown });
        }
      } else {
        return;
      }
      event.preventDefault();
      event.stopImmediatePropagation();
    };
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true });
  }, [setState]);

  const showVisible = useSculptStore((store) => store.showVisible);
  const overlay = useSculptStore((store) => store.overlay);

  // Presets, kept in the preferences, per tool.
  const presets = usePrefsStore((store) => store.prefs.modeling?.sculptPresets ?? []);
  const toolPresets = presets.filter((p) => p.tool === state.brush);
  const [presetName, setPresetName] = useState('');
  const removeWhy = toolPresets.some((p) => p.name === presetName)
    ? undefined
    : strings.why.pickInList;
  const savePresets = (list: typeof presets) =>
    void usePrefsStore.getState().update({ modeling: { sculptPresets: list } });
  const addNew = () => {
    const name = window.prompt(text.presetName)?.trim() ?? '';
    if (name === '') {
      return;
    }
    if (hasPreset(presets, name, state.brush) && !window.confirm(text.presetExists(name))) {
      return;
    }
    savePresets(
      addPreset(presets, { name, tool: state.brush, settings: presetSettings(state, units) }),
    );
    setPresetName(name);
  };
  const choosePreset = (name: string) => {
    if (name === '\u0000add') {
      addNew();
      return;
    }
    setPresetName(name);
    const preset = toolPresets.find((p) => p.name === name);
    if (preset !== undefined) {
      setState((current) => withPreset(current, preset, units));
    }
  };

  // Keep, Discard, OK and Cancel.
  const id = component?.id ?? null;
  const discard = (event: MouseEvent<HTMLButtonElement>) => {
    if (id !== null && (event.shiftKey || window.confirm(text.discardConfirm))) {
      void api.model.sculptRevert(id, 'kept').then(() => useModelStore.getState().refresh());
    }
  };
  const cancel = (event: MouseEvent<HTMLButtonElement>) => {
    if (!event.shiftKey && !window.confirm(text.cancelConfirm)) {
      return;
    }
    const done = id === null ? Promise.resolve() : api.model.sculptRevert(id, 'start');
    void done.then(async () => {
      await useDocumentStore.getState().refresh();
      await useModelStore.getState().refresh();
      closeForm();
    });
  };

  const unit = unitSuffix(units);
  const field = (name: SculptField, label: string, suffix: string, step?: number) => (
    <NumberField
      error={errors[name]}
      label={label}
      onChange={(value) => setState((current) => ({ ...current, [name]: value }))}
      step={step}
      unit={suffix}
      value={state[name]}
    />
  );
  const diameterMm = lengthValue(state.diameter, units) ?? 10;

  const changesWhy = id === null ? strings.why.noSculpt : undefined;
  return (
    <form className="sculpt-form" onSubmit={(event) => event.preventDefault()}>
      <p className="sculpt-form__hint">{text.hint}</p>
      {component === null ? (
        <p className="sculpt-form__notice" role="status">
          {text.needsComponent}
        </p>
      ) : (
        <p className="sculpt-form__target">{text.sculpting(component.name)}</p>
      )}
      <ChoiceField
        label={text.brush}
        onChange={(value) => setState((current) => ({ ...current, brush: value }))}
        options={BRUSH_CHOICES}
        value={state.brush}
      />
      {buildsOrCuts(state.brush) ? (
        <fieldset className="sculpt-form__group">
          <legend className="sculpt-form__legend">{text.customBrush}</legend>
          <CheckField
            checked={state.useCustom}
            label={text.useCustom}
            onChange={(useCustom) => setState((current) => ({ ...current, useCustom }))}
          />
          {state.useCustom ? (
            <>
              <label className="sculpt-form__row" htmlFor={brushListId}>
                {text.brushModel}
                <select
                  className="sculpt-form__select"
                  id={brushListId}
                  onChange={(event) =>
                    setState((current) => ({ ...current, customBrush: event.target.value }))
                  }
                  value={state.customBrush}
                >
                  <option value="">{text.chooseBrush}</option>
                  {state.brushFile === '' ? null : (
                    <optgroup label={text.files}>
                      <option value={FILE_BRUSH}>{brushFileName(state.brushFile)}</option>
                    </optgroup>
                  )}
                  <optgroup label={text.components}>
                    {brushChoices
                      .filter((c) => c.id !== component?.id)
                      .map((c) => (
                        <option key={c.id} value={String(c.id)}>
                          {c.name}
                        </option>
                      ))}
                  </optgroup>
                </select>
              </label>
              <button
                className="sculpt-form__load"
                onClick={() => void loadBrush()}
                type="button"
                data-tip={text.loadBrush}
              >
                <ButtonFace icon="fileOpen" label={text.loadBrush} />
              </button>
              {field('angle', text.angle, text.degrees, 15)}
              <CheckField
                checked={state.dynamic}
                label={text.dynamic}
                onChange={(dynamic) => setState((current) => ({ ...current, dynamic }))}
              />
              <CheckField
                checked={state.merge}
                label={text.merge}
                onChange={(merge) => setState((current) => ({ ...current, merge }))}
              />
            </>
          ) : null}
        </fieldset>
      ) : null}
      <fieldset className="sculpt-form__group">
        <legend className="sculpt-form__legend">{text.settings}</legend>
        {field('diameter', text.diameter, unit)}
        <SliderField
          label={text.diameter}
          max={Math.max(100, Math.ceil(diameterMm))}
          min={0.5}
          onChange={(mm) =>
            setState((current) => ({
              ...current,
              diameter: String(units === 'inch' ? Math.round((mm / 25.4) * 1000) / 1000 : mm),
            }))
          }
          step={0.5}
          value={diameterMm}
        />
        {field('strength', text.strength, text.percent, 5)}
        <SliderField
          label={text.strength}
          max={100}
          min={1}
          onChange={(v) => setState((current) => ({ ...current, strength: String(v) }))}
          unit={text.percent}
          value={numberValue(state.strength) ?? 50}
        />
        {field('softness', text.softness, text.percent, 5)}
        <SliderField
          label={text.softness}
          max={100}
          min={0}
          onChange={(v) => setState((current) => ({ ...current, softness: String(v) }))}
          unit={text.percent}
          value={numberValue(state.softness) ?? 50}
        />
        <CheckField
          checked={state.preserve}
          label={text.preserve}
          onChange={(preserve) => setState((current) => ({ ...current, preserve }))}
        />
        <CheckField
          checked={showVisible}
          label={text.showVisible}
          onChange={(on) => useSculptStore.getState().setShowVisible(on)}
        />
      </fieldset>
      <ChoiceField
        label={text.mode}
        onChange={(mode) => setState((current) => ({ ...current, mode }))}
        options={MODE_CHOICES}
        value={state.mode}
      />
      {overlay === null ? null : (
        <fieldset className="sculpt-form__group">
          <legend className="sculpt-form__legend">{text.overlay}</legend>
          <SliderField
            label={text.overlayOpacity}
            max={100}
            min={0}
            onChange={(v) => useSculptStore.getState().setOverlay({ ...overlay, opacity: v / 100 })}
            unit={text.percent}
            value={Math.round(overlay.opacity * 100)}
          />
          <CheckField
            checked={overlay.shown}
            label={text.showOverlay}
            onChange={(shown) => useSculptStore.getState().setOverlay({ ...overlay, shown })}
          />
        </fieldset>
      )}
      <fieldset className="sculpt-form__group">
        <legend className="sculpt-form__legend">{text.presets}</legend>
        <label className="sculpt-form__row" htmlFor={presetId}>
          {text.preset}
          <select
            className="sculpt-form__select"
            id={presetId}
            onChange={(event) => choosePreset(event.target.value)}
            value={toolPresets.some((p) => p.name === presetName) ? presetName : ''}
          >
            <option value="">{text.noPreset}</option>
            {toolPresets.map((p) => (
              <option key={p.name} value={p.name}>
                {p.name}
              </option>
            ))}
            <option value={'\u0000add'}>{text.addPresetItem}</option>
          </select>
        </label>
        <div className="sculpt-form__buttons">
          <button
            className="sculpt-form__load"
            onClick={addNew}
            type="button"
            data-tip={text.addPreset}
          >
            <ButtonFace icon="formCreate" label={text.addPreset} />
          </button>
          <button
            className="sculpt-form__load"
            disabled={removeWhy !== undefined}
            onClick={() => savePresets(removePreset(presets, presetName, state.brush))}
            type="button"
            data-tip={whyTip(text.removePreset, removeWhy)}
          >
            <ButtonFace icon="itemRemove" label={text.removePreset} />
          </button>
        </div>
      </fieldset>
      <p className="sculpt-form__hint">{text.controls}</p>
      <fieldset className="sculpt-form__group">
        <legend className="sculpt-form__legend">{text.changes}</legend>
        <div className="sculpt-form__buttons">
          <button
            className="sculpt-form__load"
            disabled={changesWhy !== undefined}
            onClick={() => id !== null && void api.model.sculptKeep(id)}
            type="button"
            data-tip={whyTip(text.keep, changesWhy)}
          >
            <ButtonFace icon="formApply" label={text.keep} />
          </button>
          <button
            className="sculpt-form__load"
            disabled={changesWhy !== undefined}
            onClick={discard}
            type="button"
            data-tip={whyTip(text.discard, changesWhy)}
          >
            <ButtonFace icon="dismiss" label={text.discard} />
          </button>
          <button
            className="sculpt-form__load sculpt-form__load--primary"
            onClick={closeForm}
            type="button"
            data-tip={text.ok}
          >
            <ButtonFace icon="formApply" label={text.ok} />
          </button>
          <button
            className="sculpt-form__load"
            onClick={cancel}
            type="button"
            data-tip={text.cancel}
          >
            <ButtonFace icon="dismiss" label={text.cancel} />
          </button>
        </div>
      </fieldset>
    </form>
  );
}
