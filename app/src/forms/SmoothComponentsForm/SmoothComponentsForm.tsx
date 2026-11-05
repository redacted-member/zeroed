import { useEffect, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/api';
import { strings } from '../../lib/formStrings';
import { usePreviewSession } from '../../lib/usePreviewSession';
import { useDocumentStore } from '../../state/documentStore';
import { useModelStore } from '../../state/modelStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { FormButtons } from '../FormButtons/FormButtons';
import { SliderField } from '../SliderField/SliderField';
import {
  afterBake,
  initialSmoothComponentsState,
  smoothArgs,
  type SmoothComponentsState,
} from './model';

import './SmoothComponentsForm.scss';

const text = strings.model.refine.smooth;
const common = strings.model.refine;

export function SmoothComponentsForm(): JSX.Element {
  const component = useModelStore(
    (store) => store.components.find((c) => c.id === store.selectedId) ?? null,
  );
  const lastError = useDocumentStore((store) => store.lastError);
  const closeForm = useUiStore((store) => store.closeForm);
  const [state, setState] = useState<SmoothComponentsState>(initialSmoothComponentsState);
  const [status, setStatus] = useState<string | null>(null);
  const session = usePreviewSession('keep');
  const componentId = component?.id ?? null;
  const shownFor = useRef(componentId);

  // Choosing another component keeps what the last one got.
  useEffect(() => {
    if (shownFor.current !== componentId) {
      shownFor.current = componentId;
      void session.keep();
    }
  }, [componentId, session]);

  const preview = (next: SmoothComponentsState) => {
    setStatus(null);
    void session.preview((replace) => {
      const args = smoothArgs(next, componentId, replace);
      if (args === null) {
        return Promise.reject(new Error(common.needsComponent));
      }
      return api.model.refine.smooth(args);
    });
  };

  const bake = async () => {
    if (!session.previewing()) {
      preview(state);
    }
    await session.keep();
    setState(afterBake);
    setStatus(text.baked);
  };

  return (
    <form className="smooth-components" onSubmit={(event) => event.preventDefault()}>
      <p className="smooth-components__hint">{text.hint}</p>
      <p className={component === null ? 'smooth-components__notice' : 'smooth-components__target'}>
        {component === null ? common.needsComponent : common.component(component.name)}
      </p>
      <fieldset className="smooth-components__group">
        <legend className="smooth-components__legend">{common.settings}</legend>
        <SliderField
          disabled={component === null}
          label={text.amount}
          max={100}
          min={0}
          onChange={(amount) => setState((current) => ({ ...current, amount }))}
          onCommit={(amount) => preview({ ...state, amount })}
          unit={common.percent}
          value={state.amount}
        />
        <CheckField
          checked={state.preserveTransparency}
          disabled={component === null}
          label={text.preserve}
          onChange={(preserveTransparency) => {
            const next = { ...state, preserveTransparency };
            setState(next);
            if (session.previewing()) {
              preview(next);
            }
          }}
        />
        <p className="smooth-components__hint">{text.preserveHint}</p>
      </fieldset>
      {status === null ? null : (
        <p className="smooth-components__hint" role="status">
          {status}
        </p>
      )}
      {lastError === null ? null : (
        <p className="smooth-components__notice" role="alert">
          {lastError}
        </p>
      )}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => void bake()}
        primaryWhy={component === null ? common.needsComponent : undefined}
        primaryLabel={text.bake}
      />
    </form>
  );
}
