import { useMemo, useState, type FormEvent, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import { guidePositionError, guidePositionMm, guideStateFrom } from './model';

import './GuidePositionForm.scss';

export function GuidePositionForm(): JSX.Element {
  const units = useDocumentStore((store) => store.job.units);
  const closeForm = useUiStore((store) => store.closeForm);
  const guides = useViewStore((store) => store.guides);
  const editingGuideId = useViewStore((store) => store.editingGuideId);
  const moveGuide = useViewStore((store) => store.moveGuide);
  const setGuideLocked = useViewStore((store) => store.setGuideLocked);
  const removeGuide = useViewStore((store) => store.removeGuide);
  const guide = guides.find((g) => g.id === editingGuideId) ?? null;
  const text = strings.guidePosition;
  const [state, setState] = useState(() =>
    guide === null ? { position: '0', locked: false } : guideStateFrom(guide, units),
  );
  const [showErrors, setShowErrors] = useState(false);
  const error = useMemo(() => guidePositionError(state, units), [state, units]);

  const apply = () => {
    setShowErrors(true);
    if (guide === null) {
      closeForm();
      return;
    }
    const position = guidePositionMm(state, units);
    if (position === null) {
      return;
    }
    setGuideLocked(guide.id, false);
    moveGuide(guide.id, position);
    setGuideLocked(guide.id, state.locked);
    closeForm();
  };

  const remove = () => {
    if (guide !== null) {
      removeGuide(guide.id);
    }
    closeForm();
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    apply();
  };

  return (
    <form className="guide-position" onSubmit={onSubmit}>
      {guide === null ? (
        <p className="guide-position__notice">{text.missing}</p>
      ) : (
        <fieldset className="guide-position__group">
          <legend className="guide-position__legend">
            {guide.axis === 'x' ? text.vertical : text.horizontal}
          </legend>
          <NumberField
            error={showErrors ? error : undefined}
            label={text.position}
            onChange={(position) => setState((current) => ({ ...current, position }))}
            unit={unitSuffix(units)}
            value={state.position}
          />
          <label className="guide-position__choice">
            <input
              checked={state.locked}
              onChange={(event) =>
                setState((current) => ({ ...current, locked: event.target.checked }))
              }
              type="checkbox"
            />
            {text.locked}
          </label>
        </fieldset>
      )}
      <FormButtons
        onClose={closeForm}
        onPrimary={apply}
        onSecondary={remove}
        primaryWhy={guide === null ? text.missing : undefined}
        primaryLabel={strings.form.ok}
        secondaryLabel={text.delete}
        submit
      />
    </form>
  );
}
