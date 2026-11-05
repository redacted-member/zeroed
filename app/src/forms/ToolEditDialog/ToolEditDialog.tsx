import { useId, useMemo, useState, type JSX } from 'react';

import type { Tool } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { ModalDialog } from '../../panels/ModalDialog/ModalDialog';
import { NumberField } from '../NumberField/NumberField';
import {
  editedTool,
  setStepover,
  setStepoverPercent,
  toolEditState,
  validateToolEdit,
  type ToolEditField,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './ToolEditDialog.scss';

const text = strings.toolpaths.toolRow;
const labels = strings.toolDatabase;

interface ToolEditDialogProps {
  tool: Tool;
  units: Units;
  /** The edited copy on OK, null on Cancel. */
  onClose: (tool: Tool | null) => void;
}

/** Edit ... on a toolpath form: this toolpath's copy of the tool. Mount it to open it. */
export function ToolEditDialog({ tool, units, onClose }: ToolEditDialogProps): JSX.Element {
  const titleId = useId();
  const [state, setState] = useState(() => toolEditState(tool, units));
  const [showErrors, setShowErrors] = useState(false);
  const errors = useMemo(() => validateToolEdit(state, units), [state, units]);
  const unit = unitSuffix(units);
  const feedUnit =
    (labels.feedUnitNames as Record<string, string | undefined>)[tool.feedUnits] ?? tool.feedUnits;
  const errorFor = (field: ToolEditField) => (showErrors ? errors[field] : undefined);
  const set = (field: ToolEditField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));

  const ok = () => {
    setShowErrors(true);
    const edited = editedTool(tool, state, units);
    if (edited !== null) {
      onClose(edited);
    }
  };

  return (
    <ModalDialog
      className="tool-edit"
      closeLabel={text.cancel}
      footer={
        <button className="tool-edit__ok" onClick={ok} type="button" data-tip={text.ok}>
          <ButtonFace icon="formApply" label={text.ok} />
        </button>
      }
      onClose={() => onClose(null)}
      open
      title={text.editTitle}
      titleId={titleId}
    >
      <div className="tool-edit__body">
        <p className="tool-edit__name">{tool.displayName ?? tool.name}</p>
        <p className="tool-edit__hint">{text.editHint}</p>
        <fieldset className="tool-edit__group">
          <legend className="tool-edit__legend">{labels.cutting}</legend>
          <NumberField
            error={errorFor('passDepth')}
            label={labels.passDepth}
            onChange={(v) => set('passDepth', v)}
            unit={unit}
            value={state.passDepth}
          />
          <NumberField
            error={errorFor('stepover')}
            label={labels.stepover}
            onChange={(v) => setState((current) => setStepover(current, v, tool, units))}
            unit={unit}
            value={state.stepover}
          />
          <NumberField
            error={errorFor('stepoverPercent')}
            label={text.stepoverPercent}
            onChange={(v) => setState((current) => setStepoverPercent(current, v, tool, units))}
            unit="%"
            value={state.stepoverPercent}
          />
        </fieldset>
        <fieldset className="tool-edit__group">
          <legend className="tool-edit__legend">{labels.feeds}</legend>
          <NumberField
            error={errorFor('spindleSpeed')}
            label={labels.spindle}
            onChange={(v) => set('spindleSpeed', v)}
            unit={labels.rpm}
            value={state.spindleSpeed}
          />
          <NumberField
            error={errorFor('feedRate')}
            label={labels.feedRate}
            onChange={(v) => set('feedRate', v)}
            unit={feedUnit}
            value={state.feedRate}
          />
          <NumberField
            error={errorFor('plungeRate')}
            label={labels.plungeRate}
            onChange={(v) => set('plungeRate', v)}
            unit={feedUnit}
            value={state.plungeRate}
          />
        </fieldset>
        <NumberField
          error={errorFor('number')}
          label={labels.number}
          onChange={(v) => set('number', v)}
          step={1}
          value={state.number}
        />
      </div>
    </ModalDialog>
  );
}
