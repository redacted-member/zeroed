import { useId, useState, type FormEvent, type JSX } from 'react';

import type { Sheet, SheetMaterial } from '../../bridge/document';
import type { Units, XyOrigin, ZZero } from '../../bridge/types';
import { FormButtons } from '../../forms/FormButtons/FormButtons';
import { NumberField } from '../../forms/NumberField/NumberField';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import {
  materialFromState,
  stateFromMaterial,
  validate,
  type SheetEditorField,
  type SheetEditorState,
} from './model';

import './SheetEditor.scss';

const text = strings.sheets.editor;

const Z_ZEROES: ReadonlyArray<{ value: ZZero; label: string }> = [
  { value: 'top', label: strings.jobSetup.zZeroTop },
  { value: 'bottom', label: strings.jobSetup.zZeroBottom },
];

const ORIGINS: ReadonlyArray<{ value: XyOrigin; label: string }> = [
  { value: 'bottomLeft', label: strings.jobSetup.originBottomLeft },
  { value: 'bottomRight', label: strings.jobSetup.originBottomRight },
  { value: 'topLeft', label: strings.jobSetup.originTopLeft },
  { value: 'topRight', label: strings.jobSetup.originTopRight },
  { value: 'centre', label: strings.jobSetup.originCentre },
];

interface SheetEditorProps {
  /** The sheets being edited; the fields start from the first one. */
  sheets: readonly Sheet[];
  material: SheetMaterial;
  units: Units;
  onApply: (material: SheetMaterial, scaleDesign: boolean) => void;
  onClose: () => void;
}

/** The Edit Sheet form: one size, thickness and datum for the chosen sheets. */
export function SheetEditor({
  sheets,
  material,
  units,
  onApply,
  onClose,
}: SheetEditorProps): JSX.Element {
  const id = useId();
  const [state, setState] = useState<SheetEditorState>(() => stateFromMaterial(material, units));
  const [touched, setTouched] = useState(false);
  const [scaleDesign, setScaleDesign] = useState(false);
  const errors = touched ? validate(state) : {};
  const unit = unitSuffix(units);

  const field = (name: SheetEditorField, label: string) => (
    <NumberField
      error={errors[name]}
      label={label}
      onChange={(value) => setState((current) => ({ ...current, [name]: value }))}
      unit={unit}
      value={state[name]}
    />
  );

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setTouched(true);
    const next = materialFromState(state, units);
    if (next !== null) {
      onApply(next, scaleDesign);
    }
  };

  return (
    <form aria-labelledby={`${id}-title`} className="sheet-editor" onSubmit={onSubmit}>
      <h2 className="sheet-editor__title" id={`${id}-title`}>
        {text.title(sheets.length)}
      </h2>
      <p className="sheet-editor__hint">{text.sheetNames(sheets.map((s) => s.name).join(', '))}</p>

      <fieldset className="sheet-editor__group">
        <legend className="sheet-editor__legend">{text.size}</legend>
        {field('width', text.width)}
        {field('height', text.height)}
        {field('thickness', text.thickness)}
      </fieldset>

      <fieldset className="sheet-editor__group">
        <legend className="sheet-editor__legend">{text.scaling}</legend>
        <label className="sheet-editor__choice">
          <input
            aria-describedby={`${id}-scale-hint`}
            checked={scaleDesign}
            onChange={(event) => setScaleDesign(event.target.checked)}
            type="checkbox"
          />
          {text.scaleDesign}
        </label>
        <p className="sheet-editor__hint" id={`${id}-scale-hint`}>
          {text.scaleDesignHint}
        </p>
      </fieldset>

      <fieldset className="sheet-editor__group">
        <legend className="sheet-editor__legend">{text.zZero}</legend>
        <div className="sheet-editor__choices">
          {Z_ZEROES.map(({ value, label }) => (
            <label className="sheet-editor__choice" key={value}>
              <input
                checked={state.zZero === value}
                name={`${id}-z-zero`}
                onChange={() => setState((current) => ({ ...current, zZero: value }))}
                type="radio"
                value={value}
              />
              {label}
            </label>
          ))}
        </div>
      </fieldset>

      <fieldset className="sheet-editor__group">
        <legend className="sheet-editor__legend">{text.xyOrigin}</legend>
        <div className="sheet-editor__row">
          <label className="sheet-editor__label" htmlFor={`${id}-origin`}>
            {text.xyOrigin}
          </label>
          <select
            className="sheet-editor__select"
            id={`${id}-origin`}
            onChange={(event) =>
              setState((current) => ({ ...current, xyOrigin: event.target.value as XyOrigin }))
            }
            value={state.xyOrigin}
          >
            {ORIGINS.map(({ value, label }) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </div>
        {field('offsetX', text.offsetX)}
        {field('offsetY', text.offsetY)}
      </fieldset>

      <FormButtons
        onClose={onClose}
        onPrimary={() => undefined}
        primaryLabel={strings.form.ok}
        submit
      />
    </form>
  );
}
