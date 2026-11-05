import { useState, type JSX } from 'react';

import { api } from '../../bridge/api';
import { strings } from '../../lib/formStrings';
import { formatLength, unitSuffix } from '../../lib/units';
import { usePreviewSession } from '../../lib/usePreviewSession';
import { useDocumentStore } from '../../state/documentStore';
import { useModelStore } from '../../state/modelStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { FormButtons } from '../FormButtons/FormButtons';
import { SliderField } from '../SliderField/SliderField';
import {
  embossArgs,
  embossedTop,
  initialEmbossComponentState,
  lowRelief,
  type EmbossComponentState,
} from './model';

import './EmbossComponentForm.scss';

const text = strings.model.refine.emboss;
const common = strings.model.refine;

export function EmbossComponentForm(): JSX.Element {
  const component = useModelStore(
    (store) => store.components.find((c) => c.id === store.selectedId) ?? null,
  );
  const units = useDocumentStore((store) => store.job.units);
  const lastError = useDocumentStore((store) => store.lastError);
  const closeForm = useUiStore((store) => store.closeForm);
  const warnLowRelief = usePrefsStore((store) => store.prefs.confirmations.embossWarning);
  const [state, setState] = useState<EmbossComponentState>(initialEmbossComponentState);
  // The top before this form's preview, so the estimate does not compound.
  const [startTop, setStartTop] = useState<number | null>(null);
  const session = usePreviewSession('discard');
  const componentId = component?.id ?? null;

  const preview = (next: EmbossComponentState) => {
    if (!session.previewing()) {
      setStartTop(component?.maxHeight ?? null);
    }
    void session.preview((replace) => {
      const args = embossArgs(next, componentId, replace);
      if (args === null) {
        return Promise.reject(new Error(common.needsComponent));
      }
      return api.model.refine.emboss(args);
    });
  };

  const apply = async () => {
    if (!session.previewing()) {
      preview(state);
    }
    await session.keep();
  };

  const top = embossedTop(session.previewing() ? startTop : (component?.maxHeight ?? null), state);

  return (
    <form className="emboss-component" onSubmit={(event) => event.preventDefault()}>
      <p className="emboss-component__hint">{text.hint}</p>
      <p className={component === null ? 'emboss-component__notice' : 'emboss-component__target'}>
        {component === null ? common.needsComponent : common.component(component.name)}
      </p>
      {component !== null && warnLowRelief && lowRelief(component) ? (
        <p className="emboss-component__notice" role="alert">
          {text.lowRelief}
        </p>
      ) : null}
      <fieldset className="emboss-component__group">
        <legend className="emboss-component__legend">{common.settings}</legend>
        <SliderField
          disabled={component === null}
          label={text.scale}
          max={100}
          min={1}
          onChange={(scale) => setState((current) => ({ ...current, scale }))}
          onCommit={(scale) => preview({ ...state, scale })}
          unit={common.percent}
          value={state.scale}
        />
        {top === null ? null : (
          <p className="emboss-component__hint">
            {text.top(`${formatLength(top, units)} ${unitSuffix(units)}`)}
          </p>
        )}
        <SliderField
          disabled={component === null}
          label={text.detail}
          max={100}
          min={0}
          onChange={(detailSmoothing) => setState((current) => ({ ...current, detailSmoothing }))}
          onCommit={(detailSmoothing) => preview({ ...state, detailSmoothing })}
          unit={common.percent}
          value={state.detailSmoothing}
        />
        <p className="emboss-component__hint">{text.detailHint}</p>
      </fieldset>
      {lastError === null ? null : (
        <p className="emboss-component__notice" role="alert">
          {lastError}
        </p>
      )}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => void apply()}
        primaryWhy={component === null ? common.needsComponent : undefined}
        primaryLabel={common.apply}
      />
    </form>
  );
}
