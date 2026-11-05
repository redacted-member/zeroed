import { lazy, Suspense, useMemo, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type {
  FlipAxis,
  JobKind,
  ModelResolution,
  RotaryAxis,
  RotaryBlank,
  RotaryZZero,
  Units,
  XyOrigin,
  ZZero,
} from '../../bridge/types';
import { rectPath } from '../../lib/geometry';
import { strings } from '../../lib/formStrings';
import { useOverlays } from '../../lib/useDrawController';
import { FormButtons } from '../FormButtons/FormButtons';
import { useDocumentStore } from '../../state/documentStore';
import type { PreviewOverlay } from '../../state/drawStore';
import { useModelStore } from '../../state/modelStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import {
  datumMarker,
  designScaleOf,
  flipsDesign,
  jobFromState,
  materialMoved,
  scaleSummary,
  stateFromJob,
  switchUnits,
  unitLabel,
  unwrappedLayout,
  validate,
  withMaterial,
  type JobSetupErrors,
  type JobSetupField,
  type JobSetupFormState,
} from './model';

import './JobSetupForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import {
  flipPicture,
  jobSize,
  jobTypePicture,
  resolution,
  rotaryAxisPicture,
  xyDatumPicture,
  zZeroPicture,
} from '../HelperPicture/pictures/job';
import { HelperRow } from '../HelperRow/HelperRow';

// The dialog loads the first time Material Settings opens it.
const MaterialAppearanceDialog = lazy(async () => ({
  default: (await import('../../panels/MaterialAppearanceDialog/MaterialAppearanceDialog'))
    .MaterialAppearanceDialog,
}));

const JOB_KINDS: ReadonlyArray<{ value: JobKind; label: string }> = [
  { value: 'singleSided', label: strings.jobSetup.singleSided },
  { value: 'doubleSided', label: strings.jobSetup.doubleSided },
  { value: 'rotary', label: strings.jobSetup.rotary },
];

const Z_ZEROES: ReadonlyArray<{ value: ZZero; label: string }> = [
  { value: 'top', label: strings.jobSetup.zZeroTop },
  { value: 'bottom', label: strings.jobSetup.zZeroBottom },
];

const FLIPS: ReadonlyArray<{ value: FlipAxis; label: string }> = [
  { value: 'horizontal', label: strings.jobSetup.flipHorizontal },
  { value: 'vertical', label: strings.jobSetup.flipVertical },
];

const ORIGINS: ReadonlyArray<{ value: XyOrigin; label: string }> = [
  { value: 'bottomLeft', label: strings.jobSetup.originBottomLeft },
  { value: 'bottomRight', label: strings.jobSetup.originBottomRight },
  { value: 'topLeft', label: strings.jobSetup.originTopLeft },
  { value: 'topRight', label: strings.jobSetup.originTopRight },
  { value: 'centre', label: strings.jobSetup.originCentre },
];

const UNITS: ReadonlyArray<{ value: Units; label: string }> = [
  { value: 'inch', label: strings.units.inchLong },
  { value: 'mm', label: strings.units.mmLong },
];

const RESOLUTIONS: ReadonlyArray<{ value: ModelResolution; label: string }> = [
  { value: 'standard', label: strings.jobSetup.resolutionStandard },
  { value: 'high', label: strings.jobSetup.resolutionHigh },
  { value: 'highest', label: strings.jobSetup.resolutionHighest },
];

const AXES: ReadonlyArray<{ value: RotaryAxis; label: string }> = [
  { value: 'x', label: strings.jobSetup.axisX },
  { value: 'y', label: strings.jobSetup.axisY },
];

const BLANKS: ReadonlyArray<{ value: RotaryBlank; label: string }> = [
  { value: 'cylinder', label: strings.jobSetup.blankCylinder },
  { value: 'square', label: strings.jobSetup.blankSquare },
];

const ROTARY_Z_ZEROES: ReadonlyArray<{ value: RotaryZZero; label: string }> = [
  { value: 'centre', label: strings.jobSetup.rotaryZCentre },
  { value: 'surface', label: strings.jobSetup.rotaryZSurface },
];

/**
 * Job Setup starts again from the document's job whenever another job takes
 * the open one's place (New, Open, a vector file opened as a job) while the
 * form is showing, so it never offers, or applies, the old job's sizes.
 */
export function JobSetupForm(): JSX.Element {
  const generation = useDocumentStore((state) => state.generation);
  return <JobSetupFields key={generation} />;
}

function JobSetupFields(): JSX.Element {
  const job = useDocumentStore((state) => state.job);
  const applyJob = useDocumentStore((state) => state.applyJob);
  const closeForm = useUiStore((state) => state.closeForm);

  const [state, setState] = useState<JobSetupFormState>(() => stateFromJob(job));
  const [showErrors, setShowErrors] = useState(false);
  const [kernelError, setKernelError] = useState<string | null>(null);
  const [materialOpen, setMaterialOpen] = useState(false);

  const errors = useMemo<JobSetupErrors>(() => validate(state), [state]);
  // Design Scaling only matters for a job with something in it.
  const hasVectors = useDocumentStore((store) => store.entities.length > 0);
  const hasComponents = useModelStore((store) => store.components.length > 0);
  const hasDesign = hasVectors || hasComponents;
  // A red square marks the datum the form asks for while it is open.
  const pxPerMm = useViewStore((store) => store.camera.pxPerMm);
  const marker = datumMarker(job, state);
  const markerX = marker?.x ?? null;
  const markerY = marker?.y ?? null;
  const overlays = useMemo<PreviewOverlay[]>(() => {
    if (markerX === null || markerY === null) {
      return [];
    }
    const half = 5 / pxPerMm;
    return [
      {
        path: rectPath(markerX - half, markerY - half, half * 2, half * 2),
        tone: 'first',
        fill: true,
      },
    ];
  }, [markerX, markerY, pxPerMm]);
  useOverlays(overlays);
  const scale = useMemo(() => designScaleOf(state, job), [state, job]);

  const setField = (field: JobSetupField, value: string) => {
    setState((current) => ({ ...current, [field]: value }));
  };

  const errorFor = (field: JobSetupField): string | undefined =>
    showErrors ? errors[field] : undefined;

  const onSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const next = jobFromState(state);
    if (next === null) {
      return;
    }
    try {
      if (flipsDesign(state, job)) {
        // One undo step changes the axis and turns the design with the material.
        await api.job.setFlipped(next);
        await useDocumentStore.getState().refresh();
      } else if (designScaleOf(state, job) !== null) {
        // One undo step changes the job and stretches the design with it.
        await api.job.setScaled(next);
        await useDocumentStore.getState().refresh();
      } else {
        await applyJob(next);
      }
      setKernelError(null);
      closeForm();
      const stored = useDocumentStore.getState().job;
      if (materialMoved(job, stored)) {
        useViewStore.getState().zoomToJob(stored);
      }
      void useToolpathStore.getState().followJobChange(job);
    } catch (error) {
      console.error('job setup was refused', error);
      setKernelError(strings.jobSetup.errorKernel);
    }
  };

  const rotary = state.kind === 'rotary';
  const layout = rotary ? unwrappedLayout(state) : null;

  const lengthField = (field: JobSetupField, label: string, disabled = false) => {
    const id = `job-setup-${field}`;
    const message = errorFor(field);
    return (
      <div className="job-setup__row" key={field}>
        <label className="job-setup__label" htmlFor={id}>
          {label}
        </label>
        <span className="job-setup__input">
          <input
            aria-describedby={message === undefined ? undefined : `${id}-error`}
            aria-invalid={message !== undefined}
            className="job-setup__field"
            disabled={disabled}
            id={id}
            inputMode="decimal"
            onChange={(event) => setField(field, event.target.value)}
            type="text"
            value={state[field]}
          />
          <span className="job-setup__unit">{unitLabel(state.units)}</span>
        </span>
        {message === undefined ? null : (
          <p className="job-setup__error" id={`${id}-error`}>
            {message}
          </p>
        )}
      </div>
    );
  };

  return (
    <>
      <form className="job-setup" noValidate onSubmit={onSubmit}>
        <fieldset className="job-setup__group">
          <legend className="job-setup__legend">{strings.jobSetup.jobType}</legend>
          <HelperRow picture={<HelperPicture picture={jobTypePicture(state.kind)} />}>
            {JOB_KINDS.map(({ value, label }) => (
              <label className="job-setup__choice" key={value}>
                <input
                  checked={state.kind === value}
                  name="job-setup-kind"
                  onChange={() => setState((current) => ({ ...current, kind: value }))}
                  type="radio"
                  value={value}
                />
                {label}
              </label>
            ))}
          </HelperRow>
        </fieldset>

        <fieldset className="job-setup__group">
          <legend className="job-setup__legend">{strings.jobSetup.jobSize}</legend>
          {rotary ? null : (
            <HelperRow picture={<HelperPicture picture={jobSize} />}>
              {lengthField('width', strings.jobSetup.width)}
              {lengthField('height', strings.jobSetup.height)}
              {lengthField('thickness', strings.jobSetup.thickness)}
            </HelperRow>
          )}
          <div className="job-setup__row">
            <span className="job-setup__label" id="job-setup-units-label">
              {strings.jobSetup.units}
            </span>
            <span
              aria-labelledby="job-setup-units-label"
              className="job-setup__choices"
              role="group"
            >
              {UNITS.map(({ value, label }) => (
                <label className="job-setup__choice" key={value}>
                  <input
                    checked={state.units === value}
                    name="job-setup-units"
                    onChange={() => setState((current) => switchUnits(current, value))}
                    type="radio"
                    value={value}
                  />
                  {label}
                </label>
              ))}
            </span>
          </div>
        </fieldset>

        {state.kind === 'rotary' ? (
          <fieldset className="job-setup__group">
            <legend className="job-setup__legend">{strings.jobSetup.rotarySection}</legend>
            <HelperRow picture={<HelperPicture picture={rotaryAxisPicture(state.rotaryAxis)} />}>
              <div className="job-setup__row">
                <span className="job-setup__label" id="job-setup-blank-label">
                  {strings.jobSetup.rotaryBlank}
                </span>
                <span
                  aria-labelledby="job-setup-blank-label"
                  className="job-setup__choices"
                  role="group"
                >
                  {BLANKS.map(({ value, label }) => (
                    <label className="job-setup__choice" key={value}>
                      <input
                        checked={state.rotaryBlank === value}
                        name="job-setup-rotary-blank"
                        onChange={() => setState((current) => ({ ...current, rotaryBlank: value }))}
                        type="radio"
                        value={value}
                      />
                      {label}
                    </label>
                  ))}
                </span>
              </div>
              {lengthField('rotaryLength', strings.jobSetup.rotaryLength)}
              {lengthField(
                'rotaryDiameter',
                state.rotaryBlank === 'square'
                  ? strings.jobSetup.rotarySide
                  : strings.jobSetup.rotaryDiameter,
              )}
              <div className="job-setup__row">
                <label className="job-setup__label" htmlFor="job-setup-axis">
                  {strings.jobSetup.rotaryAxis}
                </label>
                <select
                  className="job-setup__field"
                  id="job-setup-axis"
                  onChange={(event) =>
                    setState((current) => ({
                      ...current,
                      rotaryAxis: event.target.value as RotaryAxis,
                    }))
                  }
                  value={state.rotaryAxis}
                >
                  {AXES.map(({ value, label }) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </div>
              {job.kind === 'rotary' ? (
                <label className="job-setup__choice">
                  <input
                    checked={state.flipDesign}
                    disabled={state.rotaryAxis === job.rotary.axis}
                    onChange={(event) =>
                      setState((current) => ({ ...current, flipDesign: event.target.checked }))
                    }
                    type="checkbox"
                  />
                  {strings.jobSetup.flipDesign}
                </label>
              ) : null}
              <div className="job-setup__row">
                <span className="job-setup__label" id="job-setup-rotary-z-label">
                  {strings.jobSetup.rotaryZZero}
                </span>
                <span
                  aria-labelledby="job-setup-rotary-z-label"
                  className="job-setup__choices"
                  role="group"
                >
                  {ROTARY_Z_ZEROES.map(({ value, label }) => (
                    <label className="job-setup__choice" key={value}>
                      <input
                        checked={state.rotaryZZero === value}
                        name="job-setup-rotary-z-zero"
                        onChange={() => setState((current) => ({ ...current, rotaryZZero: value }))}
                        type="radio"
                        value={value}
                      />
                      {label}
                    </label>
                  ))}
                </span>
              </div>
              {layout === null ? null : (
                <p className="job-setup__note">
                  <span className="job-setup__note-label">{strings.jobSetup.unwrappedSize}</span>{' '}
                  <output>
                    {strings.jobSetup.unwrappedValue(
                      layout.width,
                      layout.height,
                      unitLabel(state.units),
                    )}
                  </output>
                </p>
              )}
            </HelperRow>
          </fieldset>
        ) : null}

        {state.kind === 'doubleSided' ? (
          <fieldset className="job-setup__group">
            <legend className="job-setup__legend">{strings.jobSetup.doubleSidedSection}</legend>
            <HelperRow picture={<HelperPicture picture={flipPicture(state.flip)} />}>
              <div className="job-setup__row">
                <span className="job-setup__label" id="job-setup-flip-label">
                  {strings.jobSetup.flipDirection}
                </span>
                <span
                  aria-labelledby="job-setup-flip-label"
                  className="job-setup__choices"
                  role="group"
                >
                  {FLIPS.map(({ value, label }) => (
                    <label className="job-setup__choice" key={value}>
                      <input
                        checked={state.flip === value}
                        name="job-setup-flip"
                        onChange={() => setState((current) => ({ ...current, flip: value }))}
                        type="radio"
                        value={value}
                      />
                      {label}
                    </label>
                  ))}
                </span>
              </div>
              <div className="job-setup__row">
                <span className="job-setup__label" id="job-setup-bottom-z-label">
                  {strings.jobSetup.bottomZZero}
                </span>
                <span
                  aria-labelledby="job-setup-bottom-z-label"
                  className="job-setup__choices"
                  role="group"
                >
                  {Z_ZEROES.map(({ value, label }) => (
                    <label className="job-setup__choice" key={value}>
                      <input
                        checked={state.bottomZZero === value}
                        name="job-setup-bottom-z-zero"
                        onChange={() => setState((current) => ({ ...current, bottomZZero: value }))}
                        type="radio"
                        value={value}
                      />
                      {label}
                    </label>
                  ))}
                </span>
              </div>
            </HelperRow>
          </fieldset>
        ) : null}

        <fieldset className="job-setup__group" hidden={rotary}>
          <legend className="job-setup__legend">
            {state.kind === 'doubleSided' ? strings.jobSetup.topZZero : strings.jobSetup.zZero}
          </legend>
          <HelperRow picture={<HelperPicture picture={zZeroPicture(state.zZero)} />}>
            {Z_ZEROES.map(({ value, label }) => (
              <label className="job-setup__choice" key={value}>
                <input
                  checked={state.zZero === value}
                  name="job-setup-z-zero"
                  onChange={() => setState((current) => ({ ...current, zZero: value }))}
                  type="radio"
                  value={value}
                />
                {label}
              </label>
            ))}
          </HelperRow>
        </fieldset>

        <fieldset className="job-setup__group">
          <legend className="job-setup__legend">{strings.jobSetup.xyOrigin}</legend>
          <HelperRow picture={<HelperPicture picture={xyDatumPicture(state.xyOrigin)} />}>
            <div className="job-setup__row">
              <label className="job-setup__label" htmlFor="job-setup-origin">
                {strings.jobSetup.xyOrigin}
              </label>
              <select
                className="job-setup__field"
                id="job-setup-origin"
                onChange={(event) =>
                  setState((current) => ({ ...current, xyOrigin: event.target.value as XyOrigin }))
                }
                value={state.xyOrigin}
              >
                {ORIGINS.map(({ value, label }) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </div>
            <label className="job-setup__choice">
              <input
                checked={state.useOffset}
                onChange={(event) =>
                  setState((current) => ({ ...current, useOffset: event.target.checked }))
                }
                type="checkbox"
              />
              {strings.jobSetup.useOffset}
            </label>
            {lengthField('offsetX', strings.jobSetup.offsetX, !state.useOffset)}
            {lengthField('offsetY', strings.jobSetup.offsetY, !state.useOffset)}
          </HelperRow>
        </fieldset>

        {!hasDesign ? null : (
          <fieldset className="job-setup__group">
            <legend className="job-setup__legend">{strings.jobSetup.designScaling}</legend>
            <div className="job-setup__scale">
              <label className="job-setup__choice">
                <input
                  aria-describedby="job-setup-scale-hint"
                  checked={state.scaleDesign}
                  onChange={(event) =>
                    setState((current) => ({ ...current, scaleDesign: event.target.checked }))
                  }
                  type="checkbox"
                />
                {strings.jobSetup.scaleDesign}
              </label>
              <p className="job-setup__hint" id="job-setup-scale-hint">
                {scale === null ? strings.jobSetup.scaleDesignHint : scaleSummary(scale)}
              </p>
            </div>
          </fieldset>
        )}

        <fieldset aria-describedby="job-setup-resolution-hint" className="job-setup__group">
          <legend className="job-setup__legend">{strings.jobSetup.resolution}</legend>
          <HelperRow picture={<HelperPicture picture={resolution} />}>
            {RESOLUTIONS.map(({ value, label }) => (
              <label className="job-setup__choice" key={value}>
                <input
                  checked={state.resolution === value}
                  name="job-setup-resolution"
                  onChange={() => setState((current) => ({ ...current, resolution: value }))}
                  type="radio"
                  value={value}
                />
                {label}
              </label>
            ))}
            <p className="job-setup__hint" id="job-setup-resolution-hint">
              {strings.jobSetup.resolutionHint}
            </p>
          </HelperRow>
        </fieldset>

        <fieldset className="job-setup__group">
          <legend className="job-setup__legend">{strings.jobSetup.appearance}</legend>
          <button
            aria-haspopup="dialog"
            className="job-setup__button job-setup__material"
            data-tip={strings.jobSetup.materialTip}
            onClick={() => setMaterialOpen(true)}
            type="button"
          >
            <svg aria-hidden="true" className="job-setup__palette" viewBox="0 0 16 16">
              <path d="M8 1.5a6.5 6.5 0 1 0 0 13c1 0 1.5-.6 1.5-1.3 0-.8-.6-1.1-.6-1.8 0-.7.6-1.2 1.3-1.2h1.4a3 3 0 0 0 2.9-3C14.5 3.9 11.6 1.5 8 1.5Z" />
              <circle cx="4.6" cy="7.4" r="1.1" />
              <circle cx="6.8" cy="4.5" r="1.1" />
              <circle cx="10.2" cy="4.6" r="1.1" />
            </svg>
            {strings.jobSetup.materialSettings}
          </button>
        </fieldset>

        {kernelError === null ? null : (
          <p className="job-setup__error" role="alert">
            {kernelError}
          </p>
        )}

        <FormButtons
          closeLabel={strings.form.cancel}
          onClose={() => closeForm()}
          primaryLabel={strings.form.ok}
          submit
        />
      </form>
      {materialOpen ? (
        <Suspense fallback={null}>
          <MaterialAppearanceDialog
            colour={state.solidColour}
            material={state.material}
            texture={state.texture}
            onApply={(draft) =>
              setState((current) => ({
                ...withMaterial(current, draft.material, draft.colour),
                texture: draft.texture ?? 0,
              }))
            }
            onClose={() => setMaterialOpen(false)}
          />
        </Suspense>
      ) : null}
    </>
  );
}
