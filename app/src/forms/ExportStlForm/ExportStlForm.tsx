import { useEffect, useState, type FormEvent, type JSX } from 'react';

import { exportsApi, type BackFace, type MeshStats } from '../../bridge/exports';
import { lengthText } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { useMeshPreviewStore } from '../../state/meshPreviewStore';
import { useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { saveTriangulation, stlExportIds } from '../../panels/ComponentsPanel/exportStl';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  backFaces,
  shownBack,
  convertExportStlUnits,
  countError,
  initialExportStlState,
  limitsTriangles,
  meshOptions,
  toleranceError,
  type ExportStlState,
  type JobKindForBacks,
  type ReduceBy,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './ExportStlForm.scss';

const text = strings.model.exportStl;

const REDUCE_BY: ReadonlyArray<Choice<ReduceBy>> = (['tolerance', 'count'] as const).map(
  (value) => ({ value, label: text.reduceBy[value] }),
);

function backChoices(kind: JobKindForBacks): ReadonlyArray<Choice<BackFace>> {
  return backFaces(kind).map((value) => ({ value, label: text.backs[value] }));
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function ExportStlForm(): JSX.Element {
  const selectedId = useModelStore((store) => store.selectedId);
  const selected = useModelStore(
    (store) => store.components.find((c) => c.id === store.selectedId) ?? null,
  );
  const hasComponents = useModelStore((store) => store.components.length > 0);
  const closeForm = useUiStore((store) => store.closeForm);
  const jobKind = useDocumentStore((store) => store.job.kind);
  const form = useFormState(initialExportStlState, convertExportStlUnits);
  const { setState, units } = form;
  // A back the job no longer offers falls back to the flat plane.
  const state: ExportStlState = { ...form.state, back: shownBack(form.state.back, jobKind) };
  const [showErrors, setShowErrors] = useState(false);
  const [stats, setStats] = useState<MeshStats | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const limited = limitsTriangles(jobKind);
  const unit = unitSuffix(units);
  const shownMesh = useMeshPreviewStore((store) => store.mesh);
  const wireframe = useMeshPreviewStore((store) => store.wireframe);

  // The calculated mesh shows only while the form is open.
  useEffect(() => () => useMeshPreviewStore.getState().clear(), []);

  const change = (patch: Partial<ExportStlState>) => {
    setStats(null);
    setFailure(null);
    useMeshPreviewStore.getState().clear();
    setState((current) => ({ ...current, ...patch }));
  };

  const calculate = async () => {
    setShowErrors(true);
    const options = meshOptions(state, units);
    if (options === null) {
      return;
    }
    setBusy(true);
    try {
      const mesh = await exportsApi.meshPreview(stlExportIds(selectedId), options);
      setStats({ triangles: mesh.triangles, deviation: mesh.deviation });
      useMeshPreviewStore.getState().show({ xyz: mesh.xyz, back: state.back });
      setFailure(null);
    } catch (error) {
      setFailure(message(error));
    } finally {
      setBusy(false);
    }
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const options = meshOptions(state, units);
    if (options !== null) {
      void saveTriangulation(options);
    }
  };

  const calculateWhy = firstWhy([busy, strings.why.busy], [!hasComponents, strings.why.needsModel]);
  return (
    <form className="export-stl" onSubmit={onSubmit}>
      <p className="export-stl__hint">{text.hint}</p>
      <p className="export-stl__target">{text.target(selected?.name ?? null)}</p>
      <fieldset className="export-stl__group" disabled={!limited}>
        <legend className="export-stl__legend">{text.reduction}</legend>
        <ChoiceField
          label={text.reduction}
          onChange={(reduceBy) => change({ reduceBy })}
          options={REDUCE_BY}
          value={state.reduceBy}
        />
        {state.reduceBy === 'tolerance' ? (
          <NumberField
            error={showErrors ? toleranceError(state, units) : undefined}
            label={text.tolerance}
            onChange={(tolerance) => change({ tolerance })}
            unit={unit}
            value={state.tolerance}
          />
        ) : (
          <NumberField
            error={showErrors ? countError(state) : undefined}
            label={text.count}
            onChange={(count) => change({ count })}
            value={state.count}
          />
        )}
        <p className="export-stl__hint">{limited ? text.reductionHint : text.rotaryFull}</p>
      </fieldset>
      <fieldset className="export-stl__group">
        <legend className="export-stl__legend">{text.back}</legend>
        <ChoiceField
          label={text.back}
          onChange={(back) => change({ back })}
          options={backChoices(jobKind)}
          value={state.back}
        />
        <p className="export-stl__hint">{text.backHints[state.back]}</p>
      </fieldset>
      <div className="export-stl__stats">
        <button
          className="export-stl__calculate"
          disabled={calculateWhy !== undefined}
          onClick={() => void calculate()}
          type="button"
          data-tip={whyTip(busy ? text.calculating : text.calculate, calculateWhy)}
        >
          <ButtonFace icon="formCalculate" label={busy ? text.calculating : text.calculate} />
        </button>
        <dl aria-live="polite" className="export-stl__result">
          <dt className="export-stl__term">{text.triangles}</dt>
          <dd className="export-stl__value">
            {stats === null ? '' : stats.triangles.toLocaleString()}
          </dd>
          <dt className="export-stl__term">{text.maxError}</dt>
          <dd className="export-stl__value">
            {stats === null ? '' : `${lengthText(stats.deviation, units)} ${unit}`}
          </dd>
        </dl>
        {stats === null ? <p className="export-stl__hint">{text.statsHint}</p> : null}
        <CheckField
          checked={!wireframe}
          disabled={shownMesh === null}
          label={text.shaded}
          onChange={(shaded) => useMeshPreviewStore.getState().setWireframe(!shaded)}
        />
        {failure === null ? null : (
          <p className="export-stl__error" role="alert">
            {failure}
          </p>
        )}
      </div>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={hasComponents ? undefined : strings.why.needsModel}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
