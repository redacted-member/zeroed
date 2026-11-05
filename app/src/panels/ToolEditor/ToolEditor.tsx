import { useEffect, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { CuttingData, NameFormats, Tool, ToolKind, ToolVariable } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { CheckField } from '../../forms/CheckField/CheckField';
import { ChoiceField, type Choice } from '../../forms/ChoiceField/ChoiceField';
import { NumberField } from '../../forms/NumberField/NumberField';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import {
  EDITABLE_KINDS,
  FEED_UNITS,
  GEOMETRY,
  HOLDER_FIELDS,
  chipLoad,
  insertVariable,
  isAngle,
  notesLinks,
  savePlan,
  setGeometry,
  setHolder,
  setStepoverDistance,
  setStepoverPercent,
  switchKind,
  switchUnits,
  toolFormState,
  toolFromState,
  validateTool,
  type CuttingContext,
  type FeedUnit,
  type GeometryField,
  type ToolField,
  type ToolFormState,
} from '../ToolDatabase/model';
import { ToolNameFormat } from '../ToolNameFormat/ToolNameFormat';
import { ToolVariables } from '../ToolVariables/ToolVariables';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { HelperPicture } from '../../forms/HelperPicture/HelperPicture';
import { toolPicture } from '../../forms/HelperPicture/pictures/toolpaths';
import { HelperRow } from '../../forms/HelperRow/HelperRow';

import './ToolEditor.scss';

const text = strings.toolDatabase;

const UNIT_CHOICES: ReadonlyArray<Choice<Units>> = [
  { value: 'mm', label: strings.units.mmLong },
  { value: 'inch', label: strings.units.inchLong },
];

interface ToolEditorProps {
  /** The stored tool, or a new one with id 0. */
  tool: Tool;
  /** The machine and material the cutting fields are for. */
  context: CuttingContext;
  formats: NameFormats | null;
  onSave: (tool: Tool, cutting: CuttingData | null) => void;
  /** After a default name format changed. */
  onFormatsChanged: () => void;
}

/** The Tool Info side of the tool database: one tool's fields. */
export function ToolEditor(props: ToolEditorProps): JSX.Element {
  const { tool, context, formats, onSave, onFormatsChanged } = props;
  const [state, setState] = useState<ToolFormState>(() =>
    toolFormState(tool, tool.resolved ?? tool),
  );
  const [variables, setVariables] = useState<ToolVariable[]>(() => tool.variables ?? []);
  const [showErrors, setShowErrors] = useState(false);
  const [panel, setPanel] = useState<'none' | 'format' | 'variables'>('none');
  const [preview, setPreview] = useState<string | null>(null);
  const [profileNote, setProfileNote] = useState<string | null>(null);
  const errors = useMemo(() => validateTool(state), [state]);
  const ids = {
    name: useId(),
    type: useId(),
    feed: useId(),
    notes: useId(),
    insert: useId(),
    preview: useId(),
  };
  const unit = unitSuffix(state.units);
  const feedUnit = text.feedUnitNames[state.feedUnits];
  const errorFor = (field: ToolField | GeometryField) => (showErrors ? errors[field] : undefined);
  const set = (
    field:
      | 'name'
      | 'number'
      | 'flutes'
      | 'passDepth'
      | 'spindleSpeed'
      | 'feedRate'
      | 'plungeRate'
      | 'notes',
    value: string,
  ) => setState((current) => ({ ...current, [field]: value }));
  const kinds: readonly ToolKind[] = EDITABLE_KINDS.includes(state.kind)
    ? EDITABLE_KINDS
    : [state.kind, ...EDITABLE_KINDS];
  const links = notesLinks(state.notes);
  const load = chipLoad(state);
  const shownPreview = state.name.includes('{') ? preview : null;

  // The name as it shows in the list, filled in by the kernel.
  useEffect(() => {
    if (!state.name.includes('{')) {
      return;
    }
    const built = toolFromState(state, tool) ?? { ...tool, name: state.name };
    let live = true;
    const timer = window.setTimeout(() => {
      void api.tools
        .previewName({ ...built, name: state.name, variables })
        .then((reply) => {
          if (live) {
            setPreview(reply.name);
          }
        })
        .catch(() => undefined);
    }, 200);
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
  }, [state, tool, variables]);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const plan = savePlan(state, { ...tool, variables }, context);
    if (plan !== null) {
      onSave({ ...plan.tool, variables }, plan.cuttingData);
    }
  };

  const pickSelectedVector = async () => {
    setProfileNote(null);
    try {
      const reply = await api.tools.formProfile();
      setState((current) => ({ ...current, profile: reply.profile }));
      setProfileNote(text.formProfileSet);
    } catch (error) {
      setProfileNote(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <form className="tool-editor" onSubmit={onSubmit}>
      <label className="tool-editor__label" htmlFor={ids.name}>
        {text.name}
      </label>
      <div className="tool-editor__row">
        <input
          aria-describedby={shownPreview === null ? undefined : ids.preview}
          aria-invalid={errorFor('name') !== undefined}
          className="tool-editor__input tool-editor__grow"
          id={ids.name}
          onChange={(e) => set('name', e.target.value)}
          value={state.name}
        />
        <button
          aria-expanded={panel === 'format'}
          className="tool-editor__button"
          onClick={() => setPanel((p) => (p === 'format' ? 'none' : 'format'))}
          type="button"
          data-tip={text.nameFormat}
        >
          <ButtonFace icon="rename" label={text.nameFormat} />
        </button>
      </div>
      <label className="tool-editor__visually-hidden" htmlFor={ids.insert}>
        {text.insertVariable}
      </label>
      <select
        className="tool-editor__input"
        id={ids.insert}
        onChange={(e) => {
          const variable = e.target.value;
          if (variable !== '') {
            setState((current) => ({ ...current, name: insertVariable(current.name, variable) }));
          }
        }}
        value=""
      >
        <option value="">{text.insertVariableNone}</option>
        {(formats?.variables ?? []).map((name) => (
          <option key={name} value={name}>
            {name}
          </option>
        ))}
        {variables.length === 0 ? null : (
          <optgroup label={text.customVariables}>
            {variables.map((v) => (
              <option key={v.name} value={v.name}>
                {v.name}
              </option>
            ))}
          </optgroup>
        )}
      </select>
      {shownPreview === null ? null : (
        <p className="tool-editor__hint" id={ids.preview}>
          {text.displayedAs(shownPreview)}
        </p>
      )}
      {errorFor('name') === undefined ? null : (
        <p className="tool-editor__error">{errorFor('name')}</p>
      )}
      {panel === 'format' ? (
        <ToolNameFormat
          format={formats?.formats[state.kind] ?? ''}
          kind={state.kind}
          onRestore={(format) => setState((current) => ({ ...current, name: format }))}
          onSaved={onFormatsChanged}
          variables={formats?.variables ?? []}
        />
      ) : null}
      <label className="tool-editor__label" htmlFor={ids.type}>
        {text.type}
      </label>
      <select
        className="tool-editor__input"
        id={ids.type}
        onChange={(e) =>
          setState((current) => switchKind(current, e.target.value as ToolKind, formats?.formats))
        }
        value={state.kind}
      >
        {kinds.map((kind) => (
          <option key={kind} value={kind}>
            {text.kinds[kind]}
          </option>
        ))}
      </select>
      <label className="tool-editor__label" htmlFor={ids.notes}>
        {text.notes}
      </label>
      <textarea
        className="tool-editor__input tool-editor__notes"
        id={ids.notes}
        onChange={(e) => set('notes', e.target.value)}
        rows={3}
        value={state.notes}
      />
      {links.length === 0 ? null : (
        <nav aria-label={text.links} className="tool-editor__links">
          <ul className="tool-editor__link-list">
            {links.map((link) => (
              <li key={link}>
                <a className="tool-editor__link" href={link} rel="noreferrer" target="_blank">
                  {link}
                </a>
              </li>
            ))}
          </ul>
        </nav>
      )}
      <div className="tool-editor__row">
        <button
          aria-expanded={panel === 'variables'}
          className="tool-editor__button"
          onClick={() => setPanel((p) => (p === 'variables' ? 'none' : 'variables'))}
          type="button"
          data-tip={text.variables}
        >
          <ButtonFace icon="variables" label={text.variables} />
        </button>
      </div>
      {panel === 'variables' ? (
        <ToolVariables name={state.name} onChange={setVariables} variables={variables} />
      ) : null}
      <fieldset className="tool-editor__group-box">
        <legend className="tool-editor__legend">{text.geometry}</legend>
        <ChoiceField
          label={text.units}
          onChange={(units) => setState((current) => switchUnits(current, units))}
          options={UNIT_CHOICES}
          value={state.units}
        />
        <HelperRow picture={<HelperPicture picture={toolPicture(state.kind)} />}>
          {(GEOMETRY[state.kind] ?? []).map((field) => (
            <NumberField
              error={errorFor(field)}
              key={field}
              label={text.fields[field]}
              onChange={(value) => setState((current) => setGeometry(current, field, value))}
              unit={
                isAngle(field)
                  ? strings.units.degrees
                  : field === 'power'
                    ? strings.text.percent
                    : field === 'speed' || field === 'maxBurnRate'
                      ? text.feedUnitNames.mmPerMin
                      : unit
              }
              value={state.geometry[field] ?? ''}
            />
          ))}
        </HelperRow>
        {state.kind === 'formTool' ? (
          <div className="tool-editor__profile">
            <p className="tool-editor__hint">{text.formProfileHint}</p>
            <button
              className="tool-editor__button"
              onClick={() => void pickSelectedVector()}
              type="button"
              data-tip={text.formProfileUse}
            >
              <ButtonFace icon="vectorSelection" label={text.formProfileUse} />
            </button>
            <p className="tool-editor__hint" role="status">
              {profileNote ??
                (state.profile === null || state.profile === undefined
                  ? (errorFor('profile') ?? '')
                  : text.formProfileSet)}
            </p>
          </div>
        ) : null}
        <NumberField
          error={errorFor('flutes')}
          label={text.flutes}
          onChange={(v) => set('flutes', v)}
          step={1}
          value={state.flutes}
        />
      </fieldset>
      <fieldset className="tool-editor__group-box">
        <legend className="tool-editor__legend">{text.holderGroup}</legend>
        <p className="tool-editor__hint">{text.holderHint}</p>
        {HOLDER_FIELDS.map((field) => (
          <NumberField
            error={errorFor(field)}
            key={field}
            label={text[field]}
            onChange={(v) => setState((current) => setHolder(current, field, v))}
            unit={unit}
            value={state.holder[field]}
          />
        ))}
        <CheckField
          checked={state.centerCutting}
          label={text.centerCutting}
          onChange={(centerCutting) => setState((current) => ({ ...current, centerCutting }))}
        />
      </fieldset>
      <fieldset className="tool-editor__group-box">
        <legend className="tool-editor__legend">{text.cutting}</legend>
        <p className="tool-editor__hint">{text.contextHint(context.machine, context.material)}</p>
        <NumberField
          error={errorFor('passDepth')}
          label={text.passDepth}
          onChange={(v) => set('passDepth', v)}
          unit={unit}
          value={state.passDepth}
        />
        <NumberField
          error={errorFor('stepoverPercent')}
          label={text.stepover}
          onChange={(v) => setState((current) => setStepoverPercent(current, v))}
          unit={strings.text.percent}
          value={state.stepoverPercent}
        />
        <NumberField
          label={text.stepoverAbsolute}
          onChange={(v) => setState((current) => setStepoverDistance(current, v))}
          unit={unit}
          value={state.stepover}
        />
      </fieldset>
      <fieldset className="tool-editor__group-box">
        <legend className="tool-editor__legend">{text.feeds}</legend>
        <NumberField
          error={errorFor('spindleSpeed')}
          label={text.spindle}
          onChange={(v) => set('spindleSpeed', v)}
          unit={text.rpm}
          value={state.spindleSpeed}
        />
        <label className="tool-editor__label" htmlFor={ids.feed}>
          {text.feedUnits}
        </label>
        <select
          className="tool-editor__input"
          id={ids.feed}
          onChange={(e) =>
            setState((current) => ({ ...current, feedUnits: e.target.value as FeedUnit }))
          }
          value={state.feedUnits}
        >
          {FEED_UNITS.map((units) => (
            <option key={units} value={units}>
              {text.feedUnitNames[units]}
            </option>
          ))}
        </select>
        <NumberField
          error={errorFor('feedRate')}
          label={text.feedRate}
          onChange={(v) => set('feedRate', v)}
          unit={feedUnit}
          value={state.feedRate}
        />
        <NumberField
          error={errorFor('plungeRate')}
          label={text.plungeRate}
          onChange={(v) => set('plungeRate', v)}
          unit={feedUnit}
          value={state.plungeRate}
        />
        <p className="tool-editor__readout">
          <span className="tool-editor__label">{text.chipLoad}</span>{' '}
          <output className="tool-editor__value">
            {load === null ? text.chipLoadNone : `${load.toFixed(4)} ${unit}`}
          </output>
        </p>
        <NumberField
          error={errorFor('number')}
          label={text.number}
          onChange={(v) => set('number', v)}
          step={1}
          value={state.number}
        />
      </fieldset>
      <div className="tool-editor__actions">
        <button
          className="tool-editor__button tool-editor__button--primary"
          type="submit"
          data-tip={text.apply}
        >
          <ButtonFace icon="formApply" label={text.apply} />
        </button>
      </div>
    </form>
  );
}
