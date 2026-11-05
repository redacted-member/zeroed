import { useId, useState, type FormEvent, type JSX } from 'react';

import type { MergeOrder } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { useDocumentStore } from '../../state/documentStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { FormButtons } from '../FormButtons/FormButtons';
import {
  MERGE_ORDERS,
  initialMergeState,
  mergeRequest,
  mergedToolpaths,
  mixesTools,
  setOrder,
} from './model';

import './MergeToolpathsForm.scss';

const text = strings.toolpaths.merge;

const ORDER_LABELS: Record<MergeOrder, string> = {
  leftToRight: text.leftToRight,
  bottomToTop: text.bottomToTop,
  grid: text.grid,
  shortestPath: text.shortestPath,
};

export function MergeToolpathsForm(): JSX.Element {
  const toolpaths = useToolpathStore((store) => store.toolpaths);
  const merge = useToolpathStore((store) => store.merge);
  const closeForm = useUiStore((store) => store.closeForm);
  const [state, setState] = useState(initialMergeState);
  const [failure, setFailure] = useState<string | null>(null);
  const nameId = useId();
  const listId = useId();
  const request = mergeRequest(state, toolpaths);
  const mixed = mixesTools(toolpaths);
  const merged = mergedToolpaths(toolpaths);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (request === null) {
      return;
    }
    void merge(request.ids, request.name, request.options).then((ok) => {
      setFailure(ok ? null : useDocumentStore.getState().lastError);
      if (ok) {
        closeForm();
      }
    });
  };

  return (
    <form className="merge-toolpaths" onSubmit={onSubmit}>
      <p className="merge-toolpaths__hint">{text.hint}</p>
      <section aria-labelledby={listId} className="merge-toolpaths__group">
        <h3 className="merge-toolpaths__legend" id={listId}>
          {text.list}
        </h3>
        {merged.length === 0 ? (
          <p className="merge-toolpaths__hint">{text.noneVisible}</p>
        ) : (
          <ul className="merge-toolpaths__list">
            {merged.map((toolpath) => (
              <li className="merge-toolpaths__option" key={toolpath.id}>
                {toolpath.name}
                <span className="merge-toolpaths__tool">{toolpath.toolName}</span>
              </li>
            ))}
          </ul>
        )}
      </section>
      {mixed ? (
        <p className="merge-toolpaths__notice" role="alert">
          {text.oneTool}
        </p>
      ) : request === null ? (
        <p className="merge-toolpaths__hint">{text.needsTwo}</p>
      ) : null}
      <fieldset className="merge-toolpaths__group">
        <legend className="merge-toolpaths__legend">{text.ordering}</legend>
        {MERGE_ORDERS.map((order) => (
          <CheckField
            checked={state.orders.includes(order)}
            key={order}
            label={ORDER_LABELS[order]}
            onChange={(on) => setState((current) => setOrder(current, order, on))}
          />
        ))}
        <CheckField
          checked={state.byPart}
          label={text.byPart}
          onChange={(byPart) => setState((current) => ({ ...current, byPart }))}
        />
      </fieldset>
      {failure === null ? null : (
        <p className="merge-toolpaths__notice" role="alert">
          {failure}
        </p>
      )}
      <label className="merge-toolpaths__name" htmlFor={nameId}>
        {text.name}
        <input
          className="merge-toolpaths__input"
          id={nameId}
          onChange={(event) => setState((current) => ({ ...current, name: event.target.value }))}
          placeholder={text.defaultName}
          type="text"
          value={state.name}
        />
      </label>
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        primaryWhy={request !== null ? undefined : mixed ? text.oneTool : text.needsTwo}
        primaryLabel={text.action}
        submit
      />
    </form>
  );
}
