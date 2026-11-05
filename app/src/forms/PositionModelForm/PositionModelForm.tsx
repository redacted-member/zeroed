import { useState, type FormEvent, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useImportStep } from '../../lib/useImportStep';
import { useDocumentStore } from '../../state/documentStore';
import { useImportStore } from '../../state/importStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { NumberField } from '../NumberField/NumberField';
import {
  canMakeBothSides,
  PLANE_STEPS,
  planeAtStep,
  planeError,
  planeText,
  planeValue,
  stepOfPlane,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';

import './PositionModelForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { zeroPlane } from '../HelperPicture/pictures/modeling';

const text = strings.model.positionModel;

export function PositionModelForm(): JSX.Element {
  const job = useDocumentStore((store) => store.job);
  const units = job.units;
  const openForm = useUiStore((store) => store.openForm);
  const closeForm = useUiStore((store) => store.closeForm);
  const place = useImportStore((store) => store.place);
  const view = useImportStore((store) => store.view);
  const info = useImportStore((store) => store.info);
  const error = useImportStore((store) => store.error);
  const busy = useImportStore((store) => store.busy);
  const [plane, setPlane] = useState(() => planeText(place.zeroPlane, units));
  const [importing, setImporting] = useState(false);
  const actionWhy = firstWhy(
    [importing || busy, strings.why.busy],
    [info === null, strings.why.loading],
  );
  useImportStep('position');

  const height = info?.size[2] ?? 0;
  const store = useImportStore.getState;
  const bothSidesAllowed = canMakeBothSides(job);

  const movePlane = (millimetres: number) => {
    setPlane(planeText(millimetres, units));
    store().setPlace({ zeroPlane: millimetres });
  };

  const typePlane = (value: string) => {
    setPlane(value);
    const mm = planeValue(value, units, height);
    if (mm !== null) {
      store().setPlace({ zeroPlane: mm });
    }
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (planeValue(plane, units, height) === null) {
      return;
    }
    setImporting(true);
    void store()
      .importPlaced()
      .then((ok) => {
        setImporting(false);
        if (ok) {
          closeForm();
        }
      });
  };

  const cancel = () => {
    store().clear();
    closeForm();
  };

  return (
    <form className="position-model" onSubmit={onSubmit}>
      <p className="position-model__hint">{text.hint}</p>
      <fieldset className="position-model__group">
        <legend className="position-model__legend">{text.zeroPlane}</legend>
        <HelperRow picture={<HelperPicture picture={zeroPlane} />}>
          <label className="position-model__slider">
            <span>{text.zeroPlane}</span>
            <input
              className="position-model__range"
              disabled={height <= 0}
              max={PLANE_STEPS}
              min={0}
              onChange={(event) => movePlane(planeAtStep(Number(event.target.value), height))}
              type="range"
              value={stepOfPlane(place.zeroPlane, height)}
            />
          </label>
          <NumberField
            error={planeError(plane, units, height)}
            label={text.zeroPlane}
            onChange={typePlane}
            unit={unitSuffix(units)}
            value={plane}
          />
          <div className="position-model__row">
            <button
              className="position-model__button"
              onClick={() => movePlane(height / 2)}
              type="button"
              data-tip={text.middle}
            >
              <ButtonFace icon="zeroPlane" label={text.middle} />
            </button>
            <button
              className="position-model__button"
              onClick={() => movePlane(0)}
              type="button"
              data-tip={text.bottom}
            >
              <ButtonFace icon="zeroPlane" label={text.bottom} />
            </button>
          </div>
        </HelperRow>
      </fieldset>
      <fieldset className="position-model__group">
        <CheckField
          checked={place.bothSides && bothSidesAllowed}
          disabled={!bothSidesAllowed}
          label={text.bothSides}
          onChange={(bothSides) => store().setPlace({ bothSides })}
        />
        <p className="position-model__hint">{text.bothSidesHint}</p>
        <CheckField
          checked={place.discardBelow || (place.bothSides && bothSidesAllowed)}
          disabled={place.bothSides && bothSidesAllowed}
          label={text.discard}
          onChange={(discardBelow) => store().setPlace({ discardBelow })}
        />
        <CheckField
          checked={view.undercuts}
          label={text.undercuts}
          onChange={(undercuts) => store().setView({ undercuts })}
        />
      </fieldset>
      {error !== null ? (
        <p className="position-model__error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="position-model__actions">
        <button
          className="position-model__button"
          onClick={() => openForm('importModel')}
          type="button"
          data-tip={text.back}
        >
          <ButtonFace icon="arrowLeft" label={text.back} />
        </button>
        <button
          className="position-model__button position-model__button--primary"
          disabled={actionWhy !== undefined}
          type="submit"
          data-tip={whyTip(text.action, actionWhy)}
        >
          <ButtonFace icon="importModel" label={text.action} />
        </button>
        <button
          className="position-model__button"
          onClick={cancel}
          type="button"
          data-tip={strings.model.importModel.cancel}
        >
          <ButtonFace icon="dismiss" label={strings.model.importModel.cancel} />
        </button>
      </div>
    </form>
  );
}
