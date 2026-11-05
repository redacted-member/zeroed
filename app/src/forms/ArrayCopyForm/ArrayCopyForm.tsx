import { useMemo, useState, type FormEvent, type JSX } from 'react';

import type { ArrayCopyArgs, BlockTurn } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { whyTip } from '../../shell/Tooltip/model';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { SelectionSizeField } from '../SelectionSizeField/SelectionSizeField';
import {
  arrayCopyArgs,
  convertArrayCopyUnits,
  initialArrayCopyState,
  nextTurn,
  validateArrayCopy,
  type ArrayCopyField,
} from './model';

import './ArrayCopyForm.scss';

const text = strings.editObjects.arrayCopy;

const MODES: ReadonlyArray<Choice<ArrayCopyArgs['mode']>> = [
  { value: 'gap', label: text.gap },
  { value: 'offset', label: text.offset },
];

/** A shape that shows how a symmetry place is turned. */
function TurnGlyph({ turn }: { turn: BlockTurn }): JSX.Element {
  return (
    <svg
      aria-hidden="true"
      className={`array-copy-form__glyph array-copy-form__glyph--${turn}`}
      viewBox="0 0 16 16"
    >
      <path d="M4 14V2h8M4 8h5" />
    </svg>
  );
}

export function ArrayCopyForm(): JSX.Element {
  // Vectors, components or both: the box covers whatever is selected.
  const hasSelection = useDocumentStore((store) => store.selection.bbox !== null);
  const arrayCopySelection = useDocumentStore((store) => store.arrayCopySelection);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(initialArrayCopyState, convertArrayCopyUnits);
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateArrayCopy(state, units), [state, units]);
  const unit = unitSuffix(units);

  const setField = (field: ArrayCopyField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: ArrayCopyField) => (showErrors ? errors[field] : undefined);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const args = arrayCopyArgs(state, units);
    if (args !== null && hasSelection) {
      void arrayCopySelection(args);
    }
  };

  return (
    <form className="array-copy-form" onSubmit={onSubmit}>
      <p className="array-copy-form__hint">{text.hint}</p>
      {hasSelection ? null : (
        <p className="array-copy-form__notice">{strings.editObjects.needsObjects}</p>
      )}
      <SelectionSizeField units={units} />
      <fieldset className="array-copy-form__group">
        <legend className="array-copy-form__legend">{text.rowsColumns}</legend>
        <NumberField
          error={errorFor('cols')}
          label={text.columns}
          onChange={(v) => setField('cols', v)}
          step={1}
          value={state.cols}
        />
        <NumberField
          error={errorFor('rows')}
          label={text.rows}
          onChange={(v) => setField('rows', v)}
          step={1}
          value={state.rows}
        />
      </fieldset>
      <fieldset className="array-copy-form__group">
        <legend className="array-copy-form__legend">{text.spacing}</legend>
        <ChoiceField
          label={text.measure}
          onChange={(mode) => setState((current) => ({ ...current, mode }))}
          options={MODES}
          value={state.mode}
        />
        <NumberField
          error={errorFor('gapX')}
          label={text.x}
          onChange={(v) => setField('gapX', v)}
          unit={unit}
          value={state.gapX}
        />
        <NumberField
          error={errorFor('gapY')}
          label={text.y}
          onChange={(v) => setField('gapY', v)}
          unit={unit}
          value={state.gapY}
        />
      </fieldset>
      <fieldset className="array-copy-form__group">
        <legend className="array-copy-form__legend">{text.symmetry}</legend>
        <p className="array-copy-form__hint">{text.symmetryHint}</p>
        <div className="array-copy-form__block">
          {/* The block drawn as it lies: the second row above the first. */}
          {[2, 3, 0, 1].map((index) => {
            const turn = state.symmetry[index] ?? 'none';
            const row = Math.floor(index / 2) + 1;
            const column = (index % 2) + 1;
            const firstWhy = index === 0 ? strings.why.firstCopy : undefined;
            return (
              <button
                aria-label={`${text.place(row, column)}: ${text.turns[turn]}`}
                className="array-copy-form__turn"
                disabled={firstWhy !== undefined}
                key={index}
                onClick={() =>
                  setState((current) => ({
                    ...current,
                    symmetry: nextTurn(current.symmetry, index),
                  }))
                }
                data-tip={whyTip(text.turns[turn], firstWhy)}
                type="button"
              >
                <TurnGlyph turn={turn} />
              </button>
            );
          })}
        </div>
      </fieldset>
      <fieldset className="array-copy-form__group">
        <legend className="array-copy-form__legend">{text.displacement}</legend>
        <NumberField
          error={errorFor('rowX')}
          label={text.rowX}
          onChange={(v) => setField('rowX', v)}
          unit={unit}
          value={state.rowX}
        />
        <NumberField
          error={errorFor('rowY')}
          label={text.rowY}
          onChange={(v) => setField('rowY', v)}
          unit={unit}
          value={state.rowY}
        />
        <NumberField
          error={errorFor('colX')}
          label={text.columnX}
          onChange={(v) => setField('colX', v)}
          unit={unit}
          value={state.colX}
        />
        <NumberField
          error={errorFor('colY')}
          label={text.columnY}
          onChange={(v) => setField('colY', v)}
          unit={unit}
          value={state.colY}
        />
      </fieldset>
      <CheckField
        checked={state.group}
        label={text.group}
        onChange={(group) => setState((current) => ({ ...current, group }))}
      />
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={hasSelection ? undefined : strings.editObjects.needsObjects}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
