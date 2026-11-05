import { useId, useState, type JSX, type MouseEvent } from 'react';

import type { Units } from '../../bridge/types';
import { lengthText } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import type { PassState } from '../../lib/passFields';
import { formatLength, unitSuffix } from '../../lib/units';
import { ModalDialog } from '../../panels/ModalDialog/ModalDialog';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { NumberField } from '../NumberField/NumberField';
import {
  addPassAt,
  applyDepth,
  clearPasses,
  deletePass,
  diagramFraction,
  editorResult,
  nearestPass,
  openPassEditor,
  selectPass,
  setAssociated,
  setLastPass,
  type PassMethod,
  type PassSpan,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './EditPassesDialog.scss';

const text = strings.toolpaths.passes;

/** Diagram size in its own units: the cut runs from TOP to TOP + HEIGHT. */
const TOP = 8;
const HEIGHT = 184;

interface EditPassesDialogProps {
  passes: PassState;
  span: PassSpan;
  units: Units;
  /** The edited passes on OK, null on Cancel. */
  onClose: (passes: PassState | null) => void;
}

/** The Edit Passes dialog: a pass list with its diagram and the list utilities. Mount it to open it. */
export function EditPassesDialog({
  passes,
  span,
  units,
  onClose,
}: EditPassesDialogProps): JSX.Element {
  const titleId = useId();
  const listId = useId();
  const [state, setState] = useState(() => openPassEditor(passes, span, units));
  const unit = unitSuffix(units);
  const last = state.depths.length - 1;
  const passWhy = firstWhy(
    [state.selected === null, strings.why.pickInList],
    [state.selected === last, strings.why.lastPass],
  );

  const fractionAt = (event: MouseEvent<SVGSVGElement>): number => {
    const box = event.currentTarget.getBoundingClientRect();
    const y = ((event.clientY - box.top) / Math.max(1, box.height)) * (HEIGHT + 2 * TOP);
    return (y - TOP) / HEIGHT;
  };

  const methods: ReadonlyArray<Choice<PassMethod>> = [
    { value: 'tool', label: text.byTool(formatLength(span.toolPassDepth, units)) },
    { value: 'count', label: text.byNumber },
  ];

  return (
    <ModalDialog
      className="edit-passes"
      closeLabel={text.cancel}
      footer={
        <button
          className="edit-passes__ok"
          onClick={() => onClose(editorResult(state, passes))}
          type="button"
          data-tip={text.ok}
        >
          <ButtonFace icon="formApply" label={text.ok} />
        </button>
      }
      onClose={() => onClose(null)}
      open
      title={text.dialogTitle}
      titleId={titleId}
    >
      <div className="edit-passes__body">
        <fieldset className="edit-passes__group">
          <legend className="edit-passes__legend">{text.passDepths}</legend>
          <div className="edit-passes__lists">
            <select
              aria-label={text.list}
              className="edit-passes__list"
              id={listId}
              onChange={(event) =>
                setState((current) => selectPass(current, Number(event.target.value), units))
              }
              size={8}
              value={state.selected === null ? '' : String(state.selected)}
            >
              {state.depths.map((depth, i) => (
                <option key={`${i}-${depth}`} value={i}>
                  {text.listItem(i + 1, `${lengthText(depth, units)} ${unit}`)}
                </option>
              ))}
            </select>
            <svg
              aria-label={text.diagram}
              className="edit-passes__diagram"
              onClick={(event) => {
                const index = nearestPass(state, fractionAt(event), span);
                setState((current) => selectPass(current, index, units));
              }}
              onDoubleClick={(event) => {
                const depth = fractionAt(event) * span.span;
                setState((current) => addPassAt(current, depth, span, units));
              }}
              role="img"
              viewBox={`0 0 100 ${HEIGHT + 2 * TOP}`}
            >
              <rect className="edit-passes__material" height={HEIGHT} width={100} x={0} y={TOP} />
              {state.depths.map((depth, i) => {
                const y = TOP + diagramFraction(depth, span) * HEIGHT;
                return (
                  <line
                    className={
                      i === state.selected
                        ? 'edit-passes__pass edit-passes__pass--selected'
                        : 'edit-passes__pass'
                    }
                    key={`${i}-${depth}`}
                    x1={0}
                    x2={100}
                    y1={y}
                    y2={y}
                  />
                );
              })}
            </svg>
          </div>
          {state.custom ? <p className="edit-passes__note">{text.custom}</p> : null}
          <div className="edit-passes__row">
            <NumberField
              disabled={state.selected === null || state.selected === last}
              label={text.depth}
              onChange={(depthText) => setState((current) => ({ ...current, depthText }))}
              onEnter={() => setState((current) => applyDepth(current, span, units))}
              unit={unit}
              value={state.depthText}
            />
            <button
              className="edit-passes__button"
              disabled={passWhy !== undefined}
              onClick={() => setState((current) => applyDepth(current, span, units))}
              type="button"
              data-tip={whyTip(text.apply, passWhy)}
            >
              <ButtonFace icon="formApply" label={text.apply} />
            </button>
          </div>
          <div className="edit-passes__row">
            <button
              className="edit-passes__button"
              disabled={passWhy !== undefined}
              onClick={() => setState((current) => deletePass(current, units))}
              type="button"
              data-tip={whyTip(text.remove, passWhy)}
            >
              <ButtonFace icon="itemRemove" label={text.remove} />
            </button>
            <button
              className="edit-passes__button"
              onClick={() => setState((current) => clearPasses(current, span, units))}
              type="button"
              data-tip={text.clearAll}
            >
              <ButtonFace icon="editDelete" label={text.clearAll} />
            </button>
          </div>
          <CheckField
            checked={state.useLastPass}
            label={text.lastPass}
            onChange={(useLastPass) =>
              setState((current) => setLastPass(current, { useLastPass }, span, units))
            }
          />
          <NumberField
            disabled={!state.useLastPass}
            label={text.lastPassThickness}
            onChange={(lastText) =>
              setState((current) => setLastPass(current, { lastText }, span, units))
            }
            unit={unit}
            value={state.lastText}
          />
        </fieldset>
        <fieldset className="edit-passes__group">
          <legend className="edit-passes__legend">{text.utilities}</legend>
          <ChoiceField
            label={text.method}
            onChange={(method) => setState((current) => ({ ...current, method }))}
            options={methods}
            value={state.method}
          />
          <CheckField
            checked={state.exact}
            disabled={state.method !== 'tool'}
            label={text.exact}
            onChange={(exact) => setState((current) => ({ ...current, exact }))}
          />
          <NumberField
            disabled={state.method !== 'count'}
            label={text.byNumber}
            onChange={(countText) => setState((current) => ({ ...current, countText }))}
            step={1}
            value={state.countText}
          />
          <button
            className="edit-passes__button"
            onClick={() => setState((current) => setAssociated(current, span, units))}
            type="button"
            data-tip={text.setAssociated}
          >
            <ButtonFace icon="setValue" label={text.setAssociated} />
          </button>
        </fieldset>
      </div>
    </ModalDialog>
  );
}
