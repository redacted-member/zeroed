import { useEffect, useMemo, useState, type FormEvent, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { selectionFrame, type MoveMode } from '../../lib/transformMath';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { AnchorPicker } from '../AnchorPicker/AnchorPicker';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertMoveUnits,
  initialMoveState,
  moveMatrixFromState,
  moveStateFromSelection,
  switchMoveMode,
  validateMove,
  type MoveField,
} from './model';

import './MoveForm.scss';

const MODES: ReadonlyArray<{ value: MoveMode; label: string }> = [
  { value: 'absolute', label: strings.transform.move.absolute },
  { value: 'relative', label: strings.transform.move.relative },
];

export function MoveForm(): JSX.Element {
  const selection = useDocumentStore((store) => store.selection);
  const transformSelection = useDocumentStore((store) => store.transformSelection);
  const closeForm = useUiStore((store) => store.closeForm);
  const text = strings.transform.move;
  const { state, setState, units, unitsRef } = useFormState(initialMoveState, convertMoveUnits);
  const [showErrors, setShowErrors] = useState(false);
  const frame = useMemo(() => selectionFrame(selection), [selection]);

  // Absolute mode shows where the anchor is now; refresh when the selection moves.
  useEffect(() => {
    setState((current) => moveStateFromSelection(current, unitsRef.current, frame));
  }, [frame, setState, unitsRef]);

  const errors = useMemo(() => validateMove(state, units), [state, units]);

  const apply = async () => {
    setShowErrors(true);
    if (frame === null) {
      return;
    }
    const matrix = moveMatrixFromState(state, units, frame);
    if (matrix !== null) {
      await transformSelection(matrix);
    }
  };

  const setField = (field: MoveField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: MoveField) => (showErrors ? errors[field] : undefined);
  const unit = unitSuffix(units);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void apply();
  };

  return (
    <form className="move-form" onSubmit={onSubmit}>
      {frame === null ? <p className="move-form__notice">{strings.transform.noSelection}</p> : null}
      {/* A relative move has no anchor to place. */}
      <fieldset className="move-form__group" disabled={state.mode === 'relative'}>
        <legend className="move-form__legend">{strings.form.anchor}</legend>
        <AnchorPicker
          label={strings.form.anchor}
          onChange={(anchor) =>
            setState((current) =>
              moveStateFromSelection({ ...current, anchor }, unitsRef.current, frame),
            )
          }
          positions="nine"
          value={state.anchor}
        />
      </fieldset>
      <fieldset className="move-form__group">
        <legend className="move-form__legend">{text.mode}</legend>
        <div className="move-form__choices">
          {MODES.map((mode) => (
            <label className="move-form__choice" key={mode.value}>
              <input
                checked={state.mode === mode.value}
                name="moveMode"
                onChange={() =>
                  setState((current) =>
                    switchMoveMode(current, unitsRef.current, mode.value, frame),
                  )
                }
                type="radio"
                value={mode.value}
              />
              {mode.label}
            </label>
          ))}
        </div>
        <NumberField
          error={errorFor('x')}
          label={text.x}
          onChange={(v) => setField('x', v)}
          unit={unit}
          value={state.x}
        />
        <NumberField
          error={errorFor('y')}
          label={text.y}
          onChange={(v) => setField('y', v)}
          unit={unit}
          value={state.y}
        />
      </fieldset>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => void apply()}
        primaryWhy={frame === null ? strings.transform.noSelection : undefined}
        primaryLabel={strings.form.apply}
        submit
      />
    </form>
  );
}
