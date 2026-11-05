import { useId, useMemo, useState, type FormEvent, type JSX } from 'react';

import type { RotaryZZero, ZZero } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import { lengthText, lengthValue } from '../../lib/formFields';
import { sidePlaneDepth } from '../../lib/modelPlane';
import { formatLength, unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { useModelStore } from '../../state/modelStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  chooseGap,
  combinedThickness,
  convertMaterialUnits,
  machineFrom,
  materialStateFrom,
  modelingPlaneZ,
  otherGap,
  sideZZero,
  surfaceZFor,
  thicknessValue,
  tooThick,
  validateMaterial,
  withMaterialRows,
  withRotaryRows,
  type MaterialField,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './MaterialSetupForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import {
  home,
  modelPosition,
  rapidGaps,
  rotaryZZeroPicture,
  zZeroPicture,
} from '../HelperPicture/pictures/job';
import { HelperRow } from '../HelperRow/HelperRow';

const text = strings.toolpaths.material;

const Z_ZEROS: ReadonlyArray<Choice<ZZero>> = [
  { value: 'top', label: text.materialSurface },
  { value: 'bottom', label: text.machineBed },
];

const ROTARY_Z_ZEROS: ReadonlyArray<Choice<RotaryZZero>> = [
  { value: 'surface', label: text.cylinderSurface },
  { value: 'centre', label: text.cylinderCentre },
];

export function MaterialSetupForm(): JSX.Element {
  const job = useDocumentStore((store) => store.job);
  const applyJob = useDocumentStore((store) => store.applyJob);
  const closeForm = useUiStore((store) => store.closeForm);
  const openForm = useUiStore((store) => store.openForm);
  const { state, setState, units } = useFormState(
    (u) => materialStateFrom(job.machine, u),
    convertMaterialUnits,
  );
  const [showErrors, setShowErrors] = useState(false);
  const side = useDocumentStore((store) => store.activeSide);
  const sheetName = useDocumentStore((store) => store.sheets.find((s) => s.active)?.name ?? null);
  const detailedPref = usePrefsStore((store) => store.prefs.toolpaths.detailedMaterialSummary);
  const [detailed, setDetailed] = useState(detailedPref);
  const flat = job.kind !== 'rotary';
  const [sizeText, setSizeText] = useState(() =>
    lengthText(flat ? job.thickness : job.rotary.diameter, units),
  );
  const [zZero, setZZero] = useState<ZZero>(() => sideZZero(job, side));
  const [rotaryZZero, setRotaryZZero] = useState<RotaryZZero>(() => job.rotary.zZero ?? 'surface');
  const size = thicknessValue(sizeText, units);
  // A rotary job's material thickness is the radius: a cut reaches the axis.
  const thickness = size === null ? null : flat ? size : size / 2;
  const materialThickness = thickness ?? job.thickness;
  const activePeak = useModelStore((store) => store.composite?.max ?? 0);
  const otherPeak = useModelStore((store) => store.composite?.otherPeak ?? 0);
  // On a double sided job the gaps measure the whole two sided model, both
  // sides standing on one modelling plane, whichever side is active.
  const modelHeight = combinedThickness(activePeak, otherPeak, job);
  const errors = useMemo(
    () => validateMaterial(state, units, materialThickness),
    [state, units, materialThickness],
  );
  const gap = lengthValue(state.modelGap, units);
  const other =
    gap === null ? '' : lengthText(otherGap(gap, materialThickness, modelHeight), units);
  const unit = unitSuffix(units);
  const gapName = useId();

  const bottomSide = job.kind === 'doubleSided' && side === 'bottom';
  const planeZ = (() => {
    const machine = machineFrom(state, units, materialThickness) ?? job.machine;
    const planeJob = { kind: job.kind, thickness: materialThickness, machine };
    const topPeak = bottomSide ? otherPeak : activePeak;
    const bottomPeak = bottomSide ? activePeak : otherPeak;
    const depth = sidePlaneDepth(planeJob, topPeak, bottomPeak, side);
    const surface = surfaceZFor(flat ? zZero : rotaryZZero, materialThickness);
    return modelingPlaneZ(surface, depth);
  })();

  const field = (name: MaterialField, label: string) => (
    <NumberField
      error={showErrors ? errors[name] : undefined}
      label={label}
      onChange={(value) => setState((current) => ({ ...current, [name]: value }))}
      unit={unit}
      value={state[name]}
    />
  );

  const gapRow = (below: boolean, label: string) => {
    const chosen = state.gapBelow === below;
    const inputId = `${gapName}-${below ? 'below' : 'above'}`;
    const error = chosen && showErrors ? errors.modelGap : undefined;
    return (
      <div className="material-setup__gap">
        <input
          aria-label={label}
          checked={chosen}
          className="material-setup__radio"
          name={gapName}
          onChange={() =>
            setState((current) => chooseGap(current, below, units, materialThickness, modelHeight))
          }
          type="radio"
        />
        <label className="material-setup__gap-label" htmlFor={inputId}>
          {label}
        </label>
        <span className="material-setup__gap-value">
          <input
            aria-invalid={error !== undefined}
            className="material-setup__input"
            disabled={!chosen}
            id={inputId}
            inputMode="decimal"
            onChange={(event) =>
              setState((current) => ({ ...current, modelGap: event.target.value }))
            }
            type="text"
            value={chosen ? state.modelGap : other}
          />
          <span className="material-setup__unit">{unit}</span>
        </span>
        {error === undefined ? null : (
          <p className="material-setup__notice" role="alert">
            {error}
          </p>
        )}
      </div>
    );
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const machine = machineFrom(state, units, materialThickness);
    if (machine !== null && size !== null) {
      const next = flat
        ? withMaterialRows({ ...job, machine }, side, size, zZero)
        : withRotaryRows({ ...job, machine }, size, rotaryZZero);
      if (detailed !== detailedPref) {
        void usePrefsStore.getState().update({ toolpaths: { detailedMaterialSummary: detailed } });
      }
      void applyJob(next).then(() => {
        closeForm();
        void useToolpathStore.getState().followJobChange(job);
      });
    }
  };

  return (
    <form className="material-setup" onSubmit={onSubmit}>
      <p className="material-setup__hint">{text.hint}</p>
      {sheetName === null ? null : (
        <p className="material-setup__readout">
          {text.activeSheet}: {sheetName}
        </p>
      )}
      <HelperRow
        picture={
          <HelperPicture picture={flat ? zZeroPicture(zZero) : rotaryZZeroPicture(rotaryZZero)} />
        }
      >
        <NumberField
          error={
            showErrors && size === null
              ? flat
                ? text.errorThickness
                : text.errorDiameter
              : undefined
          }
          label={flat ? text.thickness : text.diameter}
          onChange={setSizeText}
          unit={unit}
          value={sizeText}
        />
        <CheckField checked={detailed} label={text.detailed} onChange={setDetailed} />
        {flat ? (
          <ChoiceField
            label={text.zZero(
              job.kind === 'doubleSided' ? (side === 'top' ? text.sideTop : text.sideBottom) : null,
            )}
            onChange={setZZero}
            options={Z_ZEROS}
            value={zZero}
          />
        ) : (
          <ChoiceField
            label={text.zZero(null)}
            onChange={setRotaryZZero}
            options={ROTARY_Z_ZEROS}
            value={rotaryZZero}
          />
        )}
      </HelperRow>
      <fieldset className="material-setup__group">
        <legend className="material-setup__legend">{text.position}</legend>
        <HelperRow picture={<HelperPicture picture={modelPosition} />}>
          {gapRow(false, flat ? text.gapAbove : text.gapOutside)}
          {gapRow(true, flat ? text.gapBelow : text.gapInside)}
          <div className="material-setup__thickness">
            <p className="material-setup__readout">
              {text.modelThickness}: {formatLength(modelHeight, units)} {unit}
            </p>
            <button
              className="material-setup__set"
              onClick={() => openForm('scaleModelHeight')}
              data-tip={text.setThicknessTitle}
              type="button"
            >
              <ButtonFace icon="setValue" label={text.setThickness} />
            </button>
          </div>
          <p className="material-setup__readout">
            {text.modelingPlaneZ}: {formatLength(planeZ, units)} {unit}
          </p>
          {tooThick(modelHeight, materialThickness) ? (
            <p className="material-setup__notice" role="alert">
              {text.tooThick}
            </p>
          ) : null}
        </HelperRow>
      </fieldset>
      <fieldset className="material-setup__group">
        <legend className="material-setup__legend">{text.rapid}</legend>
        <HelperRow picture={<HelperPicture picture={rapidGaps} />}>
          {field('safeZ', text.safeZ)}
          {field('plungeGap', text.plungeGap)}
        </HelperRow>
      </fieldset>
      <fieldset className="material-setup__group">
        <legend className="material-setup__legend">{text.home}</legend>
        <HelperRow picture={<HelperPicture picture={home} />}>
          {field('homeX', text.homeX)}
          {field('homeY', text.homeY)}
          {field('homeZ', text.homeZ)}
        </HelperRow>
      </fieldset>
      <FormButtons
        closeLabel={text.cancel}
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryLabel={text.apply}
        submit
      />
    </form>
  );
}
