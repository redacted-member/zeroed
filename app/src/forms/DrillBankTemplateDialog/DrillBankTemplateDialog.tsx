import { useId, useState, type JSX } from 'react';

import type { DrillBankTemplate } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { lengthText, lengthValue, numberValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { ModalDialog } from '../../panels/ModalDialog/ModalDialog';
import { CheckField } from '../CheckField/CheckField';
import { NumberField } from '../NumberField/NumberField';
import {
  drillIndex,
  drillPosition,
  isReference,
  MAX_SIDE,
  resized,
  templateProblem,
  withDrill,
  withReference,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './DrillBankTemplateDialog.scss';

const text = strings.toolpaths.drillBank;

/** Size of one position in the preview, in its own units. */
const CELL = 24;

interface DrillBankTemplateDialogProps {
  template: DrillBankTemplate;
  units: Units;
  /** OK: the edited bank. */
  onApply: (template: DrillBankTemplate) => void;
  onClose: () => void;
}

/**
 * Drill Bank Template: the grid of drills, the central (reference) drill and
 * the pitch, with a preview where clicking a drill picks it for its Drill
 * Number, Diameter and Reference Drill. Mount it to open it.
 */
export function DrillBankTemplateDialog({
  template,
  units,
  onApply,
  onClose,
}: DrillBankTemplateDialogProps): JSX.Element {
  const [bank, setBank] = useState(template);
  const [picked, setPicked] = useState(() =>
    drillIndex(template, template.reference[0], template.reference[1]),
  );
  const [pitchText, setPitchText] = useState(() => lengthText(template.pitch, units));
  // The picked drill's diameter as typed, until another drill is picked.
  const [diameterText, setDiameterText] = useState<string | null>(null);
  const pick = (index: number) => {
    setPicked(index);
    setDiameterText(null);
  };
  const id = useId();
  const unit = unitSuffix(units);
  const drill = bank.drills[picked];
  const problem = templateProblem(bank);
  const okWhy =
    problem === null
      ? undefined
      : {
          pitch: text.errorPitch,
          noDrills: text.errorNoDrills,
          diameter: text.errorDiameter,
          duplicate: text.errorDuplicate,
        }[problem];
  const width = bank.horizontal * CELL;
  const height = bank.vertical * CELL;

  const setCount = (horizontal: number, vertical: number) => {
    const next = resized(bank, horizontal, vertical);
    setBank(next);
    pick(Math.min(picked, next.drills.length - 1));
  };

  return (
    <ModalDialog
      className="drill-bank-template"
      closeLabel={strings.form.cancel}
      footer={
        <button
          className="modal-dialog__button"
          disabled={okWhy !== undefined}
          onClick={() => {
            onApply(bank);
            onClose();
          }}
          type="button"
          data-tip={whyTip(strings.form.ok, okWhy)}
        >
          <ButtonFace icon="formApply" label={strings.form.ok} />
        </button>
      }
      onClose={onClose}
      open
      title={text.templateTitle}
      titleId={`${id}-title`}
    >
      <div className="drill-bank-template__body">
        <fieldset className="drill-bank-template__group">
          <legend className="drill-bank-template__legend">{text.grid}</legend>
          <NumberField
            label={text.horizontal}
            onChange={(v) => {
              const n = numberValue(v);
              if (n !== null && n >= 1 && n <= MAX_SIDE) {
                setCount(n, bank.vertical);
              }
            }}
            step={1}
            value={String(bank.horizontal)}
          />
          <NumberField
            label={text.vertical}
            onChange={(v) => {
              const n = numberValue(v);
              if (n !== null && n >= 1 && n <= MAX_SIDE) {
                setCount(bank.horizontal, n);
              }
            }}
            step={1}
            value={String(bank.vertical)}
          />
          <NumberField
            label={text.centralX}
            onChange={(v) => {
              const n = numberValue(v);
              if (n !== null && n >= 1 && n <= bank.horizontal) {
                setBank({ ...bank, reference: [Math.round(n) - 1, bank.reference[1]] });
              }
            }}
            step={1}
            value={String(bank.reference[0] + 1)}
          />
          <NumberField
            label={text.centralY}
            onChange={(v) => {
              const n = numberValue(v);
              if (n !== null && n >= 1 && n <= bank.vertical) {
                setBank({ ...bank, reference: [bank.reference[0], Math.round(n) - 1] });
              }
            }}
            step={1}
            value={String(bank.reference[1] + 1)}
          />
          <NumberField
            error={problem === 'pitch' ? text.errorPitch : undefined}
            label={text.pitch}
            onChange={(v) => {
              setPitchText(v);
              const mm = lengthValue(v, units);
              if (mm !== null) {
                setBank({ ...bank, pitch: mm });
              }
            }}
            unit={unit}
            value={pitchText}
          />
        </fieldset>
        <svg
          aria-label={text.preview}
          className="drill-bank-template__preview"
          role="group"
          viewBox={`0 0 ${width} ${height}`}
        >
          {bank.drills.map((d, index) => {
            const [column, row] = drillPosition(bank, index);
            const cx = column * CELL + CELL / 2;
            // Rows go up from the bottom, as on the job.
            const cy = height - (row * CELL + CELL / 2);
            const classes = ['drill-bank-template__drill'];
            if (d.number === 0) {
              classes.push('drill-bank-template__drill--empty');
            }
            if (index === picked) {
              classes.push('drill-bank-template__drill--picked');
            }
            return (
              <g
                aria-label={text.drillLabel(column + 1, row + 1, d.number)}
                className={classes.join(' ')}
                key={index}
                onClick={() => pick(index)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault();
                    pick(index);
                  }
                }}
                role="button"
                tabIndex={0}
              >
                <circle cx={cx} cy={cy} r={CELL / 2 - 3} />
                {isReference(bank, index) ? (
                  <path
                    className="drill-bank-template__cross"
                    d={`M${cx - 6} ${cy}H${cx + 6}M${cx} ${cy - 6}V${cy + 6}`}
                  />
                ) : null}
              </g>
            );
          })}
        </svg>
        {drill === undefined ? null : (
          <fieldset className="drill-bank-template__group">
            <legend className="drill-bank-template__legend">
              {text.selectedDrill(
                drillPosition(bank, picked)[0] + 1,
                drillPosition(bank, picked)[1] + 1,
              )}
            </legend>
            <NumberField
              error={problem === 'duplicate' ? text.errorDuplicate : undefined}
              label={text.drillNumber}
              onChange={(v) => {
                const n = numberValue(v);
                if (n !== null && n >= 0) {
                  setBank(withDrill(bank, picked, { number: Math.round(n) }));
                }
              }}
              step={1}
              value={String(drill.number)}
            />
            <NumberField
              disabled={drill.number === 0}
              error={problem === 'diameter' ? text.errorDiameter : undefined}
              label={text.diameter}
              onChange={(v) => {
                setDiameterText(v);
                const mm = lengthValue(v, units);
                if (mm !== null) {
                  setBank(withDrill(bank, picked, { diameter: mm }));
                }
              }}
              unit={unit}
              value={diameterText ?? lengthText(drill.diameter, units)}
            />
            <CheckField
              checked={isReference(bank, picked)}
              label={text.referenceDrill}
              onChange={(on) => {
                if (on) {
                  setBank(withReference(bank, picked));
                }
              }}
            />
          </fieldset>
        )}
        {problem === 'noDrills' ? (
          <p className="drill-bank-template__notice" role="alert">
            {text.errorNoDrills}
          </p>
        ) : null}
      </div>
    </ModalDialog>
  );
}
