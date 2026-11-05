import { useId, useState, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { formatDuration } from '../../lib/toolpathFields';
import { formatLength, unitSuffix } from '../../lib/units';
import { useDialogStore } from '../../state/dialogStore';
import { useDocumentStore } from '../../state/documentStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import { machineTiming, scaleValue, timeRows } from './model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './MachiningTimeDialog.scss';

const text = strings.toolpathShell.list;

/** Toolpaths > Machining Time: each visible toolpath's estimate, the total,
 * and the scale factor that calibrates them. */
export function MachiningTimeDialog(): JSX.Element {
  const open = useDialogStore((state) => state.open === 'machiningTime');
  const close = useDialogStore((state) => state.close);
  const show = useDialogStore((state) => state.show);
  const toolpaths = useToolpathStore((store) => store.toolpaths);
  const prefs = usePrefsStore((state) => state.prefs);
  const units = useDocumentStore((store) => store.job.units);
  const [scale, setScale] = useState(String(prefs.toolpaths.estimateScale));
  const scaleId = useId();
  const errorId = useId();
  const { rows, total } = timeRows(toolpaths);
  const typed = scaleValue(scale);
  const applyWhy = typed.value === null ? strings.why.fixValues : undefined;
  const machine = machineTiming(prefs.machines);

  const apply = async () => {
    if (typed.value === null) {
      return;
    }
    const ok = await usePrefsStore.getState().update({ toolpaths: { estimateScale: typed.value } });
    if (ok) {
      await useToolpathStore.getState().refresh();
    }
  };

  return (
    <ModalDialog
      className="machining-time"
      closeLabel={text.close}
      footer={
        <button
          className="modal-dialog__button"
          disabled={applyWhy !== undefined}
          onClick={() => void apply()}
          type="button"
          data-tip={whyTip(text.apply, applyWhy)}
        >
          <ButtonFace icon="formApply" label={text.apply} />
        </button>
      }
      onClose={close}
      open={open}
      title={text.estimateTitle}
      titleId="machining-time-title"
    >
      <div className="machining-time__body">
        <table className="machining-time__table">
          <thead>
            <tr>
              <th scope="col">{text.estimateToolpath}</th>
              <th scope="col">{text.estimateTime}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.id}>
                <td>{row.name}</td>
                <td className="machining-time__time">{formatDuration(row.seconds)}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr>
              <th scope="row">{text.estimateTotal}</th>
              <td className="machining-time__time">{formatDuration(total)}</td>
            </tr>
          </tfoot>
        </table>
        <div className="machining-time__row">
          <label className="machining-time__label" htmlFor={scaleId}>
            {text.scaleFactor}
          </label>
          <input
            aria-describedby={typed.error === undefined ? undefined : errorId}
            aria-invalid={typed.error !== undefined}
            className="machining-time__field"
            id={scaleId}
            inputMode="decimal"
            onChange={(event) => setScale(event.target.value)}
            value={scale}
          />
        </div>
        {typed.error === undefined ? null : (
          <p className="machining-time__error" id={errorId} role="alert">
            {typed.error}
          </p>
        )}
        <p className="machining-time__hint">{text.scaleHint}</p>
        {machine === null ? null : (
          <>
            <p className="machining-time__hint">
              {text.rapidRate(
                `${formatLength(machine.rapidRate, units)} ${unitSuffix(units)}/min`,
                machine.name,
              )}
            </p>
            <p className="machining-time__hint">
              {machine.accelerated ? text.accelerationOn : text.accelerationOff}
            </p>
          </>
        )}
        <button
          className="machining-time__link"
          onClick={() => show('machineConfiguration')}
          type="button"
          data-tip={text.machineSettings}
        >
          <ButtonFace icon="machine" label={text.machineSettings} />
        </button>
      </div>
    </ModalDialog>
  );
}
