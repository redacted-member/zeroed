import { useEffect, useState, type FormEvent, type JSX } from 'react';

import { DEFAULT_ORIENTATION, TOP_FACES, type TopFace } from '../../bridge/meshImport';
import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { chooseMesh } from '../../lib/importModel';
import { isRotary } from '../../lib/rotary';
import { useImportStep } from '../../lib/useImportStep';
import { useDocumentStore } from '../../state/documentStore';
import { scaleFor, useImportStore, type RotaryModelType } from '../../state/importStore';
import { pendingPath, usePendingFileStore } from '../../state/pendingFileStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { NumberField } from '../NumberField/NumberField';
import { RotaryImportFields } from '../RotaryImportFields/RotaryImportFields';
import {
  angleError,
  orientateState,
  orientationPatch,
  scaleByInch,
  sizeError,
  sizeValue,
  switchUnits,
  turnZ,
  typeSize,
  withSize,
  withTurns,
  type Axis,
  type OrientateState,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './OrientateModelForm.scss';

const text = strings.model.importModel;

const FACES: ReadonlyArray<Choice<TopFace>> = TOP_FACES.map((face) => ({
  value: face,
  label: text.faces[face],
}));

const UNITS: ReadonlyArray<Choice<Units>> = [
  { value: 'mm', label: text.mm },
  { value: 'inch', label: text.inches },
];

const SIZE_LABELS = [text.sizeX, text.sizeY, text.sizeZ] as const;

const MODEL_TYPES: ReadonlyArray<Choice<RotaryModelType>> = [
  { value: 'full', label: text.rotary.full },
  { value: 'flat', label: text.rotary.flat },
];

/** Asks for a model file; null when the dialog is cancelled. */
function shownName(path: string | null): string | null {
  if (path === null) {
    return null;
  }
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

export function OrientateModelForm(): JSX.Element {
  const jobUnits = useDocumentStore((store) => store.job.units);
  const rotaryJob = useDocumentStore((store) => isRotary(store.job));
  const modelType = useImportStore((store) => store.modelType);
  const onAxis = rotaryJob && modelType === 'full';
  const [modelUnits, setModelUnits] = useState<Units>(jobUnits);
  const openForm = useUiStore((store) => store.openForm);
  const closeForm = useUiStore((store) => store.closeForm);
  const path = useImportStore((store) => store.path);
  const info = useImportStore((store) => store.info);
  const busy = useImportStore((store) => store.busy);
  const error = useImportStore((store) => store.error);
  const [state, setState] = useState<OrientateState>(() => {
    const flow = useImportStore.getState();
    return flow.active
      ? orientateState(flow.orient, flow.info?.size ?? null, jobUnits)
      : orientateState(DEFAULT_ORIENTATION, null, jobUnits);
  });
  // File > Import (or File > Open with a model) starts a new flow; Back
  // and Reimport come here with one under way. Declared before the step
  // hook so the file is set before the first preview is asked for.
  useEffect(() => {
    const flow = useImportStore.getState();
    const pending = pendingPath('mesh');
    const units = useDocumentStore.getState().job.units;
    if (!flow.active || pending !== null) {
      flow.begin(pending, units);
      if (pending === null) {
        //, Import Component asks for the file first and
        // leaves again when none is chosen.
        void chooseMesh().then((chosen) => {
          const now = useImportStore.getState();
          if (chosen === null) {
            now.clear();
            if (useUiStore.getState().activeForm === 'importModel') {
              useUiStore.getState().closeForm();
            }
            return;
          }
          now.begin(chosen, units);
          now.setView({ step: 'orientate' });
          void now.refresh();
        });
      }
    }
    usePendingFileStore.getState().clear();
  }, []);
  useImportStep('orientate');

  // Turns made with the rings in the 3D view fill the angle fields.
  const orient = useImportStore((store) => store.orient);
  const turnsKey = `${orient.rotX},${orient.rotY},${orient.rotZ}`;
  const [shownTurns, setShownTurns] = useState(turnsKey);
  if (turnsKey !== shownTurns) {
    setShownTurns(turnsKey);
    setState(withTurns(state, orient));
  }

  // A new size from the kernel (a turn changes the extents) fills the fields.
  const size = info?.size ?? null;
  const [shownSize, setShownSize] = useState(size);
  if (size !== shownSize) {
    setShownSize(size);
    if (size !== null) {
      setState(withSize(state, size));
    }
  }

  const store = useImportStore.getState;

  /** Sends what the fields say to the flow and redraws the preview. */
  const commit = (next: OrientateState, withSizes = true): boolean => {
    const patch = orientationPatch(next);
    if (patch === null) {
      return false;
    }
    store().setOrient(patch);
    const wanted = sizeValue(next);
    const natural = store().info?.natural ?? null;
    if (withSizes && wanted !== null && natural !== null) {
      store().setScale(scaleFor(natural, wanted));
    }
    void store().refresh();
    return true;
  };

  const update = (next: OrientateState, now = false) => {
    setState(next);
    if (now) {
      commit(next, false);
    }
  };

  const choose = async () => {
    const chosen = await chooseMesh();
    if (chosen !== null) {
      store().begin(chosen, jobUnits);
      store().setView({ step: 'orientate' });
      setState(orientateState(store().orient, null, state.units));
      void store().refresh();
    }
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    commit(state);
  };

  const goTo = (form: 'positionModel' | 'segmentModel') => {
    if (commit(state)) {
      openForm(form);
    }
  };

  const cancel = () => {
    store().clear();
    closeForm();
  };

  const chooseType = (type: RotaryModelType) => {
    store().setModelType(type);
    void store().refresh();
  };

  const chooseModelUnits = (units: Units) => {
    setModelUnits(units);
    const unit = units === 'inch' ? 25.4 : 1;
    store().setScale([unit, unit, unit]);
    void store().refresh();
  };

  const importOnAxis = async () => {
    if (commit(state, false) && (await store().importRotary())) {
      closeForm();
    }
  };

  const ready = path !== null && info !== null;
  const readyWhy = firstWhy(
    [path === null, strings.why.chooseFile],
    [info === null, strings.why.loading],
  );
  const importWhy = firstWhy([busy, strings.why.busy]) ?? readyWhy;
  const suffix = unitSuffix(state.units);

  return (
    <form className="orientate-model" onSubmit={onSubmit}>
      <p className="orientate-model__hint">{text.hint}</p>
      <fieldset className="orientate-model__group">
        <legend className="orientate-model__legend">{text.file}</legend>
        <p className="orientate-model__hint">{shownName(path) ?? text.none}</p>
        {info !== null ? (
          <p className="orientate-model__hint">{text.triangles(info.triangles)}</p>
        ) : null}
        {busy ? <p className="orientate-model__hint">{text.reading}</p> : null}
        {error !== null ? (
          <p className="orientate-model__error" role="alert">
            {error}
          </p>
        ) : null}
        <button
          className="orientate-model__button"
          onClick={() => void choose()}
          type="button"
          data-tip={text.choose}
        >
          <ButtonFace icon="fileOpen" label={text.choose} />
        </button>
      </fieldset>
      {rotaryJob ? (
        <fieldset className="orientate-model__group">
          <legend className="orientate-model__legend">{text.rotary.modelType}</legend>
          <ChoiceField
            label={text.rotary.modelType}
            onChange={chooseType}
            options={MODEL_TYPES}
            value={modelType}
          />
        </fieldset>
      ) : null}
      <fieldset className="orientate-model__group" disabled={!ready}>
        <legend className="orientate-model__legend">{text.initial}</legend>
        <ChoiceField
          label={text.topFace}
          onChange={(top) => update({ ...state, top }, true)}
          options={FACES}
          value={state.top}
        />
        <div aria-label={text.turnsLabel} className="orientate-model__row" role="group">
          {text.turns.map((turn) => (
            <button
              className="orientate-model__button"
              key={turn.label}
              onClick={() => update(turnZ(state, turn.delta), true)}
              type="button"
              data-tip={turn.label}
            >
              <ButtonFace icon="rotate" label={turn.label} />
            </button>
          ))}
        </div>
      </fieldset>
      <fieldset className="orientate-model__group" disabled={!ready}>
        <legend className="orientate-model__legend">{text.rotation}</legend>
        <NumberField
          error={angleError(state.rotX)}
          label={text.rotX}
          onChange={(rotX) => setState({ ...state, rotX })}
          onEnter={() => commit(state, false)}
          onLeave={() => commit(state, false)}
          step={1}
          unit={text.degrees}
          value={state.rotX}
        />
        <NumberField
          error={angleError(state.rotY)}
          label={text.rotY}
          onChange={(rotY) => setState({ ...state, rotY })}
          onEnter={() => commit(state, false)}
          onLeave={() => commit(state, false)}
          step={1}
          unit={text.degrees}
          value={state.rotY}
        />
        <NumberField
          error={angleError(state.rotZ)}
          label={text.rotZ}
          onChange={(rotZ) => setState({ ...state, rotZ })}
          onEnter={() => commit(state, false)}
          onLeave={() => commit(state, false)}
          step={1}
          unit={text.degrees}
          value={state.rotZ}
        />
        <button
          className="orientate-model__button"
          onClick={() => update({ ...state, rotX: '0', rotY: '0', rotZ: '0' }, true)}
          type="button"
          data-tip={text.resetRotation}
        >
          <ButtonFace icon="formReset" label={text.resetRotation} />
        </button>
      </fieldset>
      {onAxis ? (
        <>
          <fieldset className="orientate-model__group" disabled={!ready}>
            <legend className="orientate-model__legend">{text.rotary.modelUnits}</legend>
            <ChoiceField
              label={text.rotary.modelUnits}
              onChange={chooseModelUnits}
              options={UNITS}
              value={modelUnits}
            />
          </fieldset>
          {ready ? <RotaryImportFields units={jobUnits} /> : null}
        </>
      ) : (
        <>
          <fieldset className="orientate-model__group" disabled={!ready}>
            <legend className="orientate-model__legend">{text.size}</legend>
            {([0, 1, 2] as const).map((axis: Axis) => (
              <NumberField
                error={sizeError(state, axis)}
                key={axis}
                label={SIZE_LABELS[axis]}
                onChange={(value) => setState(typeSize(state, axis, value, size))}
                onEnter={() => commit(state)}
                unit={suffix}
                value={state.size[axis]}
              />
            ))}
            <CheckField
              checked={state.lockRatio}
              label={text.lockRatio}
              onChange={(lockRatio) => setState({ ...state, lockRatio })}
            />
            <ChoiceField
              label={text.units}
              onChange={(units) => setState(switchUnits(state, units))}
              options={UNITS}
              value={state.units}
            />
            <div className="orientate-model__row">
              <button
                className="orientate-model__button"
                onClick={() => setState(scaleByInch(state, true))}
                type="button"
                data-tip={text.toMm}
              >
                <ButtonFace icon="setSize" label={text.toMm} />
              </button>
              <button
                className="orientate-model__button"
                onClick={() => setState(scaleByInch(state, false))}
                type="button"
                data-tip={text.toInches}
              >
                <ButtonFace icon="setSize" label={text.toInches} />
              </button>
            </div>
            <button
              className="orientate-model__button orientate-model__button--primary"
              type="submit"
              data-tip={text.apply}
            >
              <ButtonFace icon="formApply" label={text.apply} />
            </button>
          </fieldset>
          <fieldset className="orientate-model__group" disabled={!ready}>
            <button
              className="orientate-model__button"
              onClick={() => {
                store().setOrient({ centre: null });
                void store().refresh();
              }}
              type="button"
              data-tip={text.centre}
            >
              <ButtonFace icon="alignCenter" label={text.centre} />
            </button>
            <CheckField
              checked={state.perspectiveOn}
              label={text.perspective}
              onChange={(perspectiveOn) => update({ ...state, perspectiveOn }, true)}
            />
            <label className="orientate-model__slider">
              <span>{text.perspectiveAmount}</span>
              <input
                className="orientate-model__range"
                disabled={!state.perspectiveOn}
                max={100}
                min={0}
                onChange={(event) =>
                  setState({ ...state, perspective: Number(event.target.value) })
                }
                onKeyUp={() => commit(state, false)}
                onPointerUp={() => commit(state, false)}
                type="range"
                value={state.perspective}
              />
            </label>
          </fieldset>
        </>
      )}
      {onAxis ? (
        <div className="orientate-model__actions">
          <button
            className="orientate-model__button orientate-model__button--primary"
            disabled={importWhy !== undefined}
            onClick={() => void importOnAxis()}
            type="button"
            data-tip={whyTip(text.rotary.ok, importWhy)}
          >
            <ButtonFace icon="formApply" label={text.rotary.ok} />
          </button>
          <button
            className="orientate-model__button"
            onClick={cancel}
            type="button"
            data-tip={text.cancel}
          >
            <ButtonFace icon="dismiss" label={text.cancel} />
          </button>
        </div>
      ) : (
        <div className="orientate-model__actions">
          <button
            className="orientate-model__button orientate-model__button--primary"
            disabled={readyWhy !== undefined}
            onClick={() => goTo('positionModel')}
            type="button"
            data-tip={whyTip(text.positionImport, readyWhy)}
          >
            <ButtonFace icon="importModel" label={text.positionImport} />
          </button>
          <button
            className="orientate-model__button"
            disabled={readyWhy !== undefined}
            onClick={() => goTo('segmentModel')}
            type="button"
            data-tip={whyTip(text.segment, readyWhy)}
          >
            <ButtonFace icon="arrowRight" label={text.segment} />
          </button>
          <button
            className="orientate-model__button"
            onClick={cancel}
            type="button"
            data-tip={text.cancel}
          >
            <ButtonFace icon="dismiss" label={text.cancel} />
          </button>
        </div>
      )}
    </form>
  );
}
