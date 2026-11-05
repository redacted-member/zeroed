import { useId, useState, type FormEvent, type JSX, type KeyboardEvent } from 'react';

import type { NestSheetUse, Sheet } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import {
  addRow,
  deleteRow,
  draftFromPlan,
  editRow,
  isEditing,
  moveRow,
  planFromDraft,
  quantityError,
  startEdit,
  stopEdit,
  type SheetField,
  type SheetPlanDraft,
} from './model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';
import { moveWhy } from '../../forms/ButtonFace/why';

import './CustomSheetSelection.scss';

const text = strings.editObjects.nest.custom;

interface CustomSheetSelectionProps {
  sheets: readonly Sheet[];
  /** The current plan; null is the default (the active sheet, No Limit). */
  plan: readonly NestSheetUse[] | null;
  active: number;
  onApply: (plan: NestSheetUse[]) => void;
  onClose: () => void;
}

/** Custom Sheet Selection: the sheets a nest fills, in order, and how many of each. */
export function CustomSheetSelection(props: CustomSheetSelectionProps): JSX.Element {
  const { sheets, plan, active, onApply, onClose } = props;
  const [draft, setDraft] = useState<SheetPlanDraft>(() => draftFromPlan(plan, active));
  const id = useId();
  const result = planFromDraft(draft);
  const okWhy = result === null ? strings.why.fixValues : undefined;
  const selected = draft.selected;
  const upWhy = moveWhy(selected ?? -1, draft.rows.length, -1);
  const downWhy = moveWhy(selected ?? -1, draft.rows.length, 1);
  const removeWhy =
    selected === null
      ? strings.why.pickInList
      : draft.rows.length <= 1
        ? strings.why.keepOneRow
        : undefined;

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (result === null) {
      return;
    }
    onApply(result);
    onClose();
  };

  return (
    <ModalDialog
      className="custom-sheets"
      closeLabel={strings.form.cancel}
      footer={
        <button
          className="modal-dialog__button"
          disabled={okWhy !== undefined}
          form={`${id}-form`}
          type="submit"
          data-tip={whyTip(strings.form.ok, okWhy)}
        >
          <ButtonFace icon="formApply" label={strings.form.ok} />
        </button>
      }
      onClose={onClose}
      open
      title={text.title}
      titleId={`${id}-title`}
    >
      <form className="custom-sheets__form" id={`${id}-form`} onSubmit={onSubmit}>
        <p className="custom-sheets__hint">{text.hint}</p>
        <div className="custom-sheets__body">
          <div className="custom-sheets__list">
            <table className="custom-sheets__table">
              <thead>
                <tr>
                  <th className="custom-sheets__index" scope="col">
                    <span className="custom-sheets__hidden">{text.row}</span>
                  </th>
                  <th scope="col">{text.sheetName}</th>
                  <th scope="col">{text.quantity}</th>
                </tr>
              </thead>
              <tbody>
                {draft.rows.map((row, index) => {
                  const name = sheets.find((sheet) => sheet.id === row.sheet)?.name ?? '';
                  const invalid = quantityError(row.quantity) !== undefined;
                  const opener = (field: SheetField) => ({
                    onClick: () => setDraft((current) => ({ ...current, selected: index })),
                    onDoubleClick: () => setDraft((current) => startEdit(current, index, field)),
                    onKeyDown: (event: KeyboardEvent<HTMLButtonElement>) => {
                      if (event.key === 'Enter' || event.key === 'F2') {
                        event.preventDefault();
                        setDraft((current) => startEdit(current, index, field));
                      }
                    },
                  });
                  const closer = {
                    autoFocus: true,
                    onBlur: () => setDraft(stopEdit),
                    onKeyDown: (event: KeyboardEvent<HTMLElement>) => {
                      if (event.key === 'Enter' || event.key === 'Escape') {
                        event.preventDefault();
                        event.stopPropagation();
                        setDraft(stopEdit);
                      }
                    },
                  };
                  return (
                    <tr
                      aria-selected={selected === index}
                      className="custom-sheets__row"
                      key={index}
                    >
                      <td className="custom-sheets__index">{index + 1}</td>
                      <td>
                        {isEditing(draft, index, 'sheet') ? (
                          <select
                            {...closer}
                            aria-label={text.sheetOf(index + 1)}
                            className="custom-sheets__field"
                            onChange={(event) =>
                              setDraft((current) =>
                                editRow(current, index, { sheet: Number(event.target.value) }),
                              )
                            }
                            value={String(row.sheet)}
                          >
                            {sheets.map((sheet) => (
                              <option key={sheet.id} value={String(sheet.id)}>
                                {sheet.name}
                              </option>
                            ))}
                          </select>
                        ) : (
                          <button
                            {...opener('sheet')}
                            aria-label={text.sheetCell(index + 1, name)}
                            className="custom-sheets__cell"
                            type="button"
                          >
                            {name}
                          </button>
                        )}
                      </td>
                      <td>
                        {isEditing(draft, index, 'quantity') ? (
                          <input
                            {...closer}
                            aria-invalid={invalid ? true : undefined}
                            aria-label={text.quantityOf(index + 1)}
                            className="custom-sheets__field custom-sheets__quantity"
                            inputMode="numeric"
                            onChange={(event) =>
                              setDraft((current) =>
                                editRow(current, index, { quantity: event.target.value }),
                              )
                            }
                            placeholder={text.noLimit}
                            type="text"
                            value={row.quantity}
                          />
                        ) : (
                          <button
                            {...opener('quantity')}
                            aria-label={text.quantityCell(
                              index + 1,
                              row.quantity.trim() === '' ? text.noLimit : row.quantity,
                            )}
                            className={
                              invalid
                                ? 'custom-sheets__cell custom-sheets__quantity custom-sheets__cell--invalid'
                                : 'custom-sheets__cell custom-sheets__quantity'
                            }
                            type="button"
                          >
                            {row.quantity.trim() === '' ? text.noLimit : row.quantity}
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          <div className="custom-sheets__order">
            <button
              aria-label={text.moveUp}
              className="custom-sheets__icon"
              disabled={upWhy !== undefined}
              onClick={() => setDraft((current) => moveRow(current, -1))}
              data-tip={whyTip(text.moveUp, upWhy)}
              type="button"
            >
              <svg aria-hidden="true" viewBox="0 0 16 16">
                <path d="M8 2 3 7h3v7h4V7h3Z" />
              </svg>
            </button>
            <button
              aria-label={text.moveDown}
              className="custom-sheets__icon"
              disabled={downWhy !== undefined}
              onClick={() => setDraft((current) => moveRow(current, 1))}
              data-tip={whyTip(text.moveDown, downWhy)}
              type="button"
            >
              <svg aria-hidden="true" viewBox="0 0 16 16">
                <path d="M8 14 3 9h3V2h4v7h3Z" />
              </svg>
            </button>
          </div>
        </div>
        <div className="custom-sheets__edit">
          <button
            aria-label={text.add}
            className="custom-sheets__icon"
            onClick={() =>
              setDraft((current) =>
                addRow(
                  current,
                  sheets.map((sheet) => sheet.id),
                ),
              )
            }
            data-tip={text.add}
            type="button"
          >
            <svg aria-hidden="true" viewBox="0 0 16 16">
              <path d="M7 3h2v4h4v2H9v4H7V9H3V7h4Z" />
            </svg>
          </button>
          <button
            aria-label={text.remove}
            className="custom-sheets__icon"
            disabled={removeWhy !== undefined}
            onClick={() => setDraft((current) => deleteRow(current))}
            data-tip={whyTip(text.remove, removeWhy)}
            type="button"
          >
            <svg aria-hidden="true" viewBox="0 0 16 16">
              <path d="M5 2h6v1h3v2H2V3h3Zm-2 4h10l-1 8H4Z" />
            </svg>
          </button>
        </div>
      </form>
    </ModalDialog>
  );
}
