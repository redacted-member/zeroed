import { useId, type JSX } from 'react';

import type { Combine } from '../../bridge/model';
import { strings } from '../../lib/formStrings';
import { ChoiceField } from '../ChoiceField/ChoiceField';
import { combinePicture } from '../HelperPicture/pictures/modeling';
import { MODELING_COMBINES } from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './ModelingActions.scss';

const text = strings.model.actions;

const COMBINE_CHOICES = MODELING_COMBINES.map((choice) => ({
  ...choice,
  picture: combinePicture(choice.value),
}));

interface ModelingActionsProps {
  name: string;
  onName: (name: string) => void;
  combine: Combine;
  onCombine: (combine: Combine) => void;
  /**
   * What Apply (a submit button of the enclosing form) waits for, in plain
   * words: it is off while this is given, and its tooltip says it.
   */
  applyWhy?: string;
  onReset: () => void;
  onStartNew: () => void;
  onClose: () => void;
}

/** The options every modeling tool ends with: name and combine mode, then
 * Reset, Apply, Start New Component and Close. */
export function ModelingActions({
  name,
  onName,
  combine,
  onCombine,
  applyWhy,
  onReset,
  onStartNew,
  onClose,
}: ModelingActionsProps): JSX.Element {
  const nameId = useId();
  return (
    <>
      <fieldset className="modeling-actions__group">
        <legend className="modeling-actions__legend">{text.combineWith}</legend>
        <label className="modeling-actions__name" htmlFor={nameId}>
          {text.name}
          <input
            className="modeling-actions__input"
            id={nameId}
            onChange={(event) => onName(event.target.value)}
            type="text"
            value={name}
          />
        </label>
        <ChoiceField
          label={strings.model.combine.label}
          onChange={onCombine}
          options={COMBINE_CHOICES}
          value={combine}
        />
      </fieldset>
      <div className="modeling-actions__buttons">
        <button
          className="modeling-actions__button"
          onClick={onReset}
          type="button"
          data-tip={text.reset}
        >
          <ButtonFace icon="formReset" label={text.reset} />
        </button>
        <button
          className="modeling-actions__button modeling-actions__button--primary"
          disabled={applyWhy !== undefined}
          type="submit"
          data-tip={whyTip(text.apply, applyWhy)}
        >
          <ButtonFace icon="formApply" label={text.apply} />
        </button>
        <button
          className="modeling-actions__button"
          onClick={onStartNew}
          type="button"
          data-tip={text.startNew}
        >
          <ButtonFace icon="formCreate" label={text.startNew} />
        </button>
        <button
          className="modeling-actions__button"
          onClick={onClose}
          type="button"
          data-tip={strings.form.close}
        >
          <ButtonFace icon="dismiss" label={strings.form.close} />
        </button>
      </div>
    </>
  );
}
