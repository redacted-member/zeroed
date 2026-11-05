import { lazy, Suspense, useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { NestParams } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useLabels } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import { isDefaultPlan } from '../../panels/CustomSheetSelection/model';
import { useDocumentStore } from '../../state/documentStore';
import type { PreviewLabel } from '../../state/drawStore';
import { pickedComponents, useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertNestUnits,
  initialNestState,
  nestMarkers,
  nestOptions,
  nestParams,
  partCopiesError,
  partMark,
  partPropertiesOf,
  validateNest,
  type NestField,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './NestForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { nestCornerPicture } from '../HelperPicture/pictures/drawing';

// The dialog loads the first time Customize opens it.
const CustomSheetSelection = lazy(async () => ({
  default: (await import('../../panels/CustomSheetSelection/CustomSheetSelection'))
    .CustomSheetSelection,
}));

const text = strings.editObjects.nest;

const CORNERS: ReadonlyArray<Choice<NestParams['startCorner']>> = [
  { value: 'bottomLeft', label: text.bottomLeft },
  { value: 'bottomRight', label: text.bottomRight },
  { value: 'topLeft', label: text.topLeft },
  { value: 'topRight', label: text.topRight },
];

const DIRECTIONS: ReadonlyArray<Choice<NestParams['direction']>> = [
  { value: 'x', label: text.alongX },
  { value: 'y', label: text.alongY },
];

export function NestForm(): JSX.Element {
  const selection = useDocumentStore((store) => store.selection.ids);
  const entities = useDocumentStore((store) => store.entities);
  const refresh = useDocumentStore((store) => store.refresh);
  const doubleSided = useDocumentStore((store) => store.job.kind === 'doubleSided');
  const sheets = useDocumentStore((store) => store.sheets);
  const setActiveSheet = useDocumentStore((store) => store.setActiveSheet);
  const activeSheet = sheets.find((sheet) => sheet.active)?.id ?? sheets[0]?.id ?? 0;
  const sheetId = useId();
  const boundaryId = useId();
  const layers = useDocumentStore((store) => store.layers);
  const selectedId = useModelStore((store) => store.selectedId);
  const picked = useModelStore((store) => store.picked);
  const components = useMemo(() => pickedComponents({ selectedId, picked }), [selectedId, picked]);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(initialNestState, convertNestUnits);
  const [showErrors, setShowErrors] = useState(false);
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const errors = useMemo(() => validateNest(state, units), [state, units]);
  const unit = unitSuffix(units);
  const [customOpen, setCustomOpen] = useState(false);
  const [applied, setApplied] = useState<number | null>(null);
  const [showPartError, setShowPartError] = useState(false);

  // The selection, and each new one, shows the copies and filler mark its
  // objects keep.
  const selectionKey = selection.join(',');
  const [seenSelection, setSeenSelection] = useState<string | null>(null);
  if (seenSelection !== selectionKey) {
    setSeenSelection(selectionKey);
    setApplied(null);
    setShowPartError(false);
    setState((current) => ({ ...current, ...partPropertiesOf(entities, selection) }));
  }

  // Green copies and filler marks on the objects in the 2D view.
  const labels = useMemo<PreviewLabel[]>(
    () =>
      nestMarkers(entities).map((marker) => ({
        at: marker.at,
        text: marker.text,
        tone: 'second',
      })),
    [entities],
  );
  useLabels(labels);

  const onApply = async () => {
    setShowPartError(true);
    const mark = partMark(state);
    if (mark === null || selection.length === 0) {
      return;
    }
    try {
      await api.ops.nestMarks(selection, mark.copies, mark.filler);
      setApplied(selection.length);
      setFailure(null);
    } catch (error) {
      setFailure(error instanceof Error ? error.message : String(error));
    } finally {
      await refresh();
    }
  };

  const setField = (field: NestField, value: string) =>
    setState((current) => ({ ...current, [field]: value }));
  const errorFor = (field: NestField) => (showErrors ? errors[field] : undefined);

  const onSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const params = nestParams(state, units);
    const options = nestOptions(state, components, doubleSided);
    if (params === null || (selection.length === 0 && options.components.length === 0) || busy) {
      return;
    }
    setBusy(true);
    setFailure(null);
    try {
      const reply = await api.ops.nest(selection, params, {}, [], options);
      setOutcome(text.result(reply.ids.length, reply.sheetCount, reply.unplaced.length));
    } catch (error) {
      setOutcome(null);
      setFailure(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
      await refresh();
    }
  };

  const applyWhy = selection.length === 0 ? strings.editObjects.needsSelection : undefined;
  return (
    <>
      <form className="nest-form" onSubmit={(event) => void onSubmit(event)}>
        <p className="nest-form__hint">{text.hint}</p>
        {selection.length === 0 ? (
          <p className="nest-form__notice">{strings.editObjects.needsSelection}</p>
        ) : null}
        <fieldset className="nest-form__group">
          <legend className="nest-form__legend">{text.spacing}</legend>
          <NumberField
            error={errorFor('toolDiameter')}
            label={text.toolDiameter}
            onChange={(v) => setField('toolDiameter', v)}
            unit={unit}
            value={state.toolDiameter}
          />
          <NumberField
            error={errorFor('gap')}
            label={text.gap}
            onChange={(v) => setField('gap', v)}
            unit={unit}
            value={state.gap}
          />
          <NumberField
            error={errorFor('border')}
            label={text.border}
            onChange={(v) => setField('border', v)}
            unit={unit}
            value={state.border}
          />
        </fieldset>
        <fieldset className="nest-form__group">
          <legend className="nest-form__legend">{text.parts}</legend>
          <CheckField
            checked={state.rotate}
            label={text.rotation}
            onChange={(rotate) => setState((current) => ({ ...current, rotate }))}
          />
          <NumberField
            disabled={!state.rotate}
            error={errorFor('rotationStep')}
            label={text.rotationStep}
            onChange={(v) => setField('rotationStep', v)}
            unit={strings.units.degrees}
            value={state.rotationStep}
          />
          <CheckField
            checked={state.allowMirror}
            label={text.allowMirror}
            onChange={(allowMirror) => setState((current) => ({ ...current, allowMirror }))}
          />
          <CheckField
            checked={state.nestInHoles}
            label={text.nestInHoles}
            onChange={(nestInHoles) => setState((current) => ({ ...current, nestInHoles }))}
          />
          <CheckField
            checked={state.removeOriginals}
            label={text.removeOriginals}
            onChange={(removeOriginals) => setState((current) => ({ ...current, removeOriginals }))}
          />
          {doubleSided ? (
            <>
              <CheckField
                checked={state.twoSided}
                label={text.twoSided}
                onChange={(twoSided) => setState((current) => ({ ...current, twoSided }))}
              />
              <p className="nest-form__hint">{text.twoSidedHint}</p>
            </>
          ) : null}
        </fieldset>
        <fieldset className="nest-form__group">
          <legend className="nest-form__legend">{text.sheetOptions}</legend>
          <HelperRow picture={<HelperPicture picture={nestCornerPicture(state.startCorner)} />}>
            <ChoiceField
              label={text.startCorner}
              onChange={(startCorner) => setState((current) => ({ ...current, startCorner }))}
              options={CORNERS}
              value={state.startCorner}
            />
            <ChoiceField
              label={text.direction}
              onChange={(direction) => setState((current) => ({ ...current, direction }))}
              options={DIRECTIONS}
              value={state.direction}
            />
          </HelperRow>
        </fieldset>
        <fieldset className="nest-form__group">
          <legend className="nest-form__legend">{text.boundary}</legend>
          <label className="nest-form__row" htmlFor={boundaryId}>
            {text.boundaryLayer}
            <select
              className="nest-form__select"
              id={boundaryId}
              onChange={(event) =>
                setState((current) => ({
                  ...current,
                  boundaryLayer: event.target.value === '' ? null : Number(event.target.value),
                }))
              }
              value={state.boundaryLayer === null ? '' : String(state.boundaryLayer)}
            >
              <option value="">{text.noBoundary}</option>
              {layers.map((layer) => (
                <option key={layer.id} value={String(layer.id)}>
                  {layer.name}
                </option>
              ))}
            </select>
          </label>
        </fieldset>
        <fieldset className="nest-form__group">
          <legend className="nest-form__legend">{text.partProperties}</legend>
          <p className="nest-form__hint">{text.partHint}</p>
          <NumberField
            error={showPartError ? partCopiesError(state) : undefined}
            label={text.partCopies}
            onChange={(partCopies) => setState((current) => ({ ...current, partCopies }))}
            step={1}
            value={state.partCopies}
          />
          <div className="nest-form__apply">
            <CheckField
              checked={state.partFiller}
              label={text.filler}
              onChange={(partFiller) => setState((current) => ({ ...current, partFiller }))}
            />
            <button
              className="nest-form__button"
              disabled={applyWhy !== undefined}
              onClick={() => void onApply()}
              type="button"
              data-tip={whyTip(text.apply, applyWhy)}
            >
              <ButtonFace icon="formApply" label={text.apply} />
            </button>
          </div>
          {applied !== null ? (
            <p className="nest-form__outcome" role="status">
              {text.applied(applied)}
            </p>
          ) : null}
        </fieldset>
        <div className="nest-form__group">
          <label className="nest-form__row" htmlFor={sheetId}>
            {text.activeSheet}
            <select
              className="nest-form__select"
              id={sheetId}
              onChange={(event) => void setActiveSheet(Number(event.target.value))}
              value={String(activeSheet)}
            >
              {sheets.map((sheet) => (
                <option key={sheet.id} value={String(sheet.id)}>
                  {sheet.name}
                </option>
              ))}
            </select>
          </label>
          <div className="nest-form__row">
            <span>{text.nestingSheets}</span>
            <span className="nest-form__value">
              {state.sheetPlan === null ? text.useActive : text.customSheets}
            </span>
            <button
              aria-haspopup="dialog"
              className="nest-form__button"
              onClick={() => setCustomOpen(true)}
              type="button"
              data-tip={text.customize}
            >
              <ButtonFace icon="options" label={text.customize} />
            </button>
          </div>
        </div>
        {outcome !== null ? (
          <p className="nest-form__outcome" role="status">
            {outcome}
          </p>
        ) : null}
        {failure !== null ? (
          <p className="nest-form__error" role="alert">
            {failure}
          </p>
        ) : null}
        <FormButtons
          onClose={closeForm}
          onPrimary={() => undefined}
          primaryWhy={firstWhy(
            [busy, strings.why.busy],
            [selection.length === 0 && components.length === 0, strings.editObjects.needsSelection],
          )}
          primaryLabel={text.action}
          submit
        />
      </form>
      {customOpen ? (
        <Suspense fallback={null}>
          <CustomSheetSelection
            active={activeSheet}
            onApply={(plan) =>
              setState((current) => ({
                ...current,
                sheetPlan: isDefaultPlan(plan, activeSheet) ? null : plan,
              }))
            }
            onClose={() => setCustomOpen(false)}
            plan={state.sheetPlan}
            sheets={sheets}
          />
        </Suspense>
      ) : null}
    </>
  );
}
