import { useCallback, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { InlayParams, InlayPart, PlugBoundary } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useToolpathForm } from '../../lib/useToolpathForm';
import { useDocumentStore } from '../../state/documentStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { ClearanceToolsField } from '../ClearanceToolsField/ClearanceToolsField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import {
  activeSheetOf,
  convertInlayUnits,
  inlayJobs,
  plugSheetFor,
  inlayStateFrom,
  inlayToolWarning,
  initialInlayState,
  validateInlay,
  type InlayField,
} from './model';
import { firstWhy } from '../../shell/Tooltip/model';

import './InlayToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { inlayDepths } from '../HelperPicture/pictures/toolpaths';

const text = strings.toolpaths;

const BOUNDARIES: ReadonlyArray<Choice<PlugBoundary>> = [
  { value: 'sheetLimits', label: text.inlay.sheetLimits },
  { value: 'vectorOffset', label: text.inlay.vectorOffset },
];

export function InlayToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units, unitsRef } = useFormState(initialInlayState, convertInlayUnits);
  const [editingPart, setEditingPart] = useState<InlayPart | null>(null);
  const load = useCallback(
    (params: InlayParams, name: string) => {
      setEditingPart(params.part);
      setState(inlayStateFrom(params, name, unitsRef.current));
    },
    [setState, unitsRef],
  );
  const form = useToolpathForm('inlay', load, {
    setName: (name: string) => setState((current) => ({ ...current, name })),
  });
  const [showErrors, setShowErrors] = useState(false);
  const [busy, setBusy] = useState(false);
  const errors = useMemo(() => validateInlay(state, units), [state, units]);
  const nameId = useId();
  const sheetId = useId();
  const sheets = useDocumentStore((store) => store.sheets);
  const activeSheet = useDocumentStore((store) => activeSheetOf(store.sheets));
  const unit = unitSuffix(units);
  const toolWarning = inlayToolWarning(form.tool);
  const editing = form.editingName !== null ? editingPart : null;

  const errorFor = (field: InlayField) => (showErrors ? errors[field] : undefined);
  const setText = (field: InlayField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));

  const onSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const taken = useToolpathStore.getState().toolpaths.map((t) => t.name);
    const jobs = inlayJobs(state, units, taken, editing);
    if (jobs.length === 0 || busy) {
      return;
    }
    setBusy(true);
    try {
      for (const [index, job] of jobs.entries()) {
        const last = index === jobs.length - 1;
        if (job.part === 'male' && editing === null) {
          // The plug machines mirrored copies of the vectors on its own sheet.
          const home = activeSheetOf(useDocumentStore.getState().sheets);
          const sheet = plugSheetFor(state, home);
          const made = await api.cam.inlayPlugVectors(form.selectedIds, sheet ?? undefined);
          await useDocumentStore.getState().refresh();
          if (sheet !== null && home !== null && sheet !== home) {
            await useDocumentStore.getState().setActiveSheet(sheet);
          }
          const ok = await form.calculate(job.params, job.name, { ids: made.ids, keepOpen: !last });
          if (sheet !== null && home !== null && sheet !== home) {
            await useDocumentStore.getState().setActiveSheet(home);
          }
          if (!ok) {
            break;
          }
          continue;
        }
        if (!(await form.calculate(job.params, job.name, { keepOpen: !last }))) {
          break;
        }
      }
    } catch (error) {
      useDocumentStore.setState({
        lastError: error instanceof Error ? error.message : String(error),
      });
    }
    setBusy(false);
  };

  return (
    <form className="inlay-toolpath" onSubmit={(event) => void onSubmit(event)}>
      <p className="inlay-toolpath__hint">
        {form.editingName === null ? text.inlay.hint : text.editing(form.editingName)}
      </p>
      {form.selectedCount === 0 ? (
        <p className="inlay-toolpath__notice">{text.needsSelection}</p>
      ) : null}
      <fieldset className="inlay-toolpath__group">
        <legend className="inlay-toolpath__legend">{text.depths}</legend>
        <HelperRow picture={<HelperPicture picture={inlayDepths} />}>
          <NumberField
            error={errorFor('pocketDepth')}
            label={text.inlay.pocketDepth}
            onChange={(v) => setText('pocketDepth', v)}
            unit={unit}
            value={state.pocketDepth}
          />
          <NumberField
            error={errorFor('glueGap')}
            label={text.inlay.glueGap}
            onChange={(v) => setText('glueGap', v)}
            unit={unit}
            value={state.glueGap}
          />
          <p className="inlay-toolpath__hint">{text.inlay.glueGapHint}</p>
          <NumberField
            error={errorFor('surfaceGap')}
            label={text.inlay.surfaceGap}
            onChange={(v) => setText('surfaceGap', v)}
            unit={unit}
            value={state.surfaceGap}
          />
        </HelperRow>
        <p className="inlay-toolpath__hint">{text.inlay.surfaceGapHint}</p>
      </fieldset>
      <fieldset className="inlay-toolpath__group">
        <legend className="inlay-toolpath__legend">{text.tool}</legend>
        <ToolPicker onChange={form.setTool} prefer="vBit" tool={form.tool} units={units} />
        {toolWarning === undefined ? null : (
          <p className="inlay-toolpath__notice" role="status">
            {toolWarning}
          </p>
        )}
      </fieldset>
      <ClearanceToolsField
        enabled={state.useClearance}
        entries={state.clearanceTools}
        onChange={(clearanceTools) => setState((current) => ({ ...current, clearanceTools }))}
        onEnable={(useClearance) => setState((current) => ({ ...current, useClearance }))}
        options={false}
        showErrors={showErrors}
        units={units}
      />
      <label className="inlay-toolpath__row" htmlFor={sheetId}>
        {text.inlay.plugSheet}
        <select
          className="inlay-toolpath__select"
          disabled={editing !== null}
          id={sheetId}
          onChange={(event) =>
            setState((current) => ({ ...current, plugSheet: Number(event.target.value) }))
          }
          value={plugSheetFor(state, activeSheet) ?? ''}
        >
          {sheets.map((sheet) => (
            <option key={sheet.id} value={sheet.id}>
              {sheet.name}
            </option>
          ))}
        </select>
      </label>
      <fieldset className="inlay-toolpath__group">
        <legend className="inlay-toolpath__legend">{text.inlay.plugBoundary}</legend>
        <ChoiceField
          label={text.inlay.plugBoundary}
          onChange={(plugBoundary) => setState((current) => ({ ...current, plugBoundary }))}
          options={BOUNDARIES}
          value={state.plugBoundary}
        />
        <NumberField
          disabled={state.plugBoundary !== 'vectorOffset'}
          error={errorFor('boundaryOffset')}
          label={text.inlay.boundaryOffset}
          onChange={(v) => setText('boundaryOffset', v)}
          unit={unit}
          value={state.boundaryOffset}
        />
      </fieldset>
      {editing !== null ? <p className="inlay-toolpath__hint">{text.inlay.editingPart}</p> : null}
      <label className="inlay-toolpath__name" htmlFor={nameId}>
        {text.name}
        <input
          className="inlay-toolpath__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={text.inlay.defaultName}
          type="text"
          value={state.name}
        />
      </label>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={firstWhy(
          [busy, strings.why.busy],
          [form.selectedCount === 0, text.needsSelection],
          [form.tool === null, text.needsTool],
        )}
        primaryLabel={text.calculate}
        submit
      />
    </form>
  );
}
