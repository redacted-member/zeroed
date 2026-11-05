import { useEffect, useMemo, useState, type FormEvent, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { formatLength, unitSuffix } from '../../lib/units';
import { lengthText, lengthValue } from '../../lib/formFields';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import { pickedComponents, useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertReplaceBelowUnits,
  initialReplaceBelowState,
  removesEverything,
  replaceBelowArgs,
  replaceTarget,
  validateReplaceBelow,
} from './model';

import './ReplaceBelowForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { replaceBelow as replaceBelowPicture } from '../HelperPicture/pictures/modeling';

const text = strings.model.replaceBelow;
const WITH = [
  { value: 'transparent', label: text.transparent },
  { value: 'plane', label: text.plane },
] as const;

/** Replace Below: trims or planes the chosen components (or the active
 * level's) below a height, set here or with the plane's pin in 3D. */
export function ReplaceBelowForm(): JSX.Element {
  const replaceBelow = useModelStore((store) => store.replaceBelow);
  const pickedKey = useModelStore((store) => pickedComponents(store).join(','));
  const components = useModelStore((store) => store.components);
  const levels = useModelStore((store) => store.levels);
  const activeLevel = useModelStore((store) => store.activeLevel);
  const plane = useModelStore((store) => store.cuttingPlane);
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, units } = useFormState(
    initialReplaceBelowState,
    convertReplaceBelowUnits,
  );
  const [showErrors, setShowErrors] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const errors = useMemo(() => validateReplaceBelow(state, units), [state, units]);
  const unit = unitSuffix(units);

  const picked = useMemo(
    () => (pickedKey === '' ? [] : pickedKey.split(',').map(Number)),
    [pickedKey],
  );
  const target = replaceTarget(picked, activeLevel);
  const chosen = components.filter((c) =>
    target?.kind === 'components' ? target.ids.includes(c.id) : c.level === target?.id && c.visible,
  );
  const only = chosen.length === 1 ? (chosen[0] ?? null) : null;
  const maxHeight = only?.maxHeight ?? null;
  const levelName = levels.find((l) => l.id === activeLevel)?.name ?? '';
  const describe =
    target === null
      ? null
      : target.kind === 'level'
        ? text.level(levelName)
        : only !== null
          ? text.editing(only.name)
          : text.several(chosen.length);

  // The plane in the 3D view shows the height typed here; a drag of its pin
  // there comes back into the field.
  const height = lengthValue(state.height, units);
  useEffect(() => {
    const current = useModelStore.getState().cuttingPlane;
    if (height !== null && (current === null || Math.abs(current.height - height) > 1e-9)) {
      useModelStore.getState().setCuttingPlane({ height, from: 'form' });
    }
  }, [height]);
  useEffect(() => {
    if (plane?.from === 'view') {
      setState((current) => ({ ...current, height: lengthText(plane.height, units) }));
    }
  }, [plane, setState, units]);
  useEffect(() => () => useModelStore.getState().setCuttingPlane(null), []);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowErrors(true);
    const args = replaceBelowArgs(state, units, target);
    if (args !== null) {
      void replaceBelow(args).then((ok) =>
        setFailure(ok ? null : useDocumentStore.getState().lastError),
      );
    }
  };

  return (
    <form className="replace-below" onSubmit={onSubmit}>
      <p className="replace-below__hint">{text.hint}</p>
      {describe === null ? (
        <p className="replace-below__notice">{text.needsComponent}</p>
      ) : (
        <p className="replace-below__target">{describe}</p>
      )}
      {maxHeight === null ? null : (
        <p className="replace-below__hint">
          {text.top(`${formatLength(maxHeight, units)} ${unit}`)}
        </p>
      )}
      <fieldset className="replace-below__group">
        <legend className="replace-below__legend">{text.settings}</legend>
        <HelperRow picture={<HelperPicture picture={replaceBelowPicture} />}>
          <NumberField
            error={showErrors ? errors.height : undefined}
            label={text.height}
            onChange={(next) => setState((current) => ({ ...current, height: next }))}
            unit={unit}
            value={state.height}
          />
          <ChoiceField
            label={text.with}
            onChange={(next) => setState((current) => ({ ...current, with: next }))}
            options={WITH}
            value={state.with}
          />
          <CheckField
            checked={state.lower}
            label={text.lower}
            onChange={(lower) => setState((current) => ({ ...current, lower }))}
          />
        </HelperRow>
      </fieldset>
      <p className="replace-below__hint">{text.pinHint}</p>
      {removesEverything(state, units, maxHeight) ? (
        <p className="replace-below__notice">{text.removesAll}</p>
      ) : null}
      {failure === null ? null : (
        <p className="replace-below__notice" role="alert">
          {failure}
        </p>
      )}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={target === null ? text.needsComponent : undefined}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
