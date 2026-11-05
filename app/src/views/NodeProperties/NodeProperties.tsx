import { useEffect, useRef, useState, type FormEvent, type JSX, type KeyboardEvent } from 'react';

import type { Point } from '../../bridge/document';
import type { Units } from '../../bridge/types';

import { sendToolCommand } from '../../lib/toolCommands';
import { strings } from '../../lib/strings';
import { unitSuffix } from '../../lib/units';
import { useDocumentStore } from '../../state/documentStore';
import { useNodeToolStore } from '../../state/nodeToolStore';
import { moveCommand, nodeFields, type NodeFields } from './model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './NodeProperties.scss';

const m = strings.contextMenu;

/** The node Properties box of node editing (X on one node, or Properties
 * on the node menu): type the node's X and Y. */
export function NodeProperties(): JSX.Element | null {
  const at = useNodeToolStore((state) => state.properties);
  const units = useDocumentStore((state) => state.job.units);
  if (at === null) {
    return null;
  }
  return <NodePropertiesBox at={at} key={`${at.x},${at.y},${units}`} units={units} />;
}

function NodePropertiesBox({ at, units }: { at: Point; units: Units }): JSX.Element {
  const close = useNodeToolStore((state) => state.closeProperties);
  const [fields, setFields] = useState<NodeFields>(() => nodeFields(at, units));
  const first = useRef<HTMLInputElement | null>(null);
  useEffect(() => {
    first.current?.select();
  }, []);

  const command = moveCommand(fields, units);
  const okWhy = command === null ? m.fixValues : undefined;
  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (command !== null) {
      sendToolCommand(command);
      close();
    }
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Escape') {
      close();
    }
  };
  const unit = unitSuffix(units);
  return (
    <form
      aria-label={m.nodePropertiesTitle}
      className="node-properties"
      onSubmit={onSubmit}
      role="dialog"
    >
      <h2 className="node-properties__title">{m.nodePropertiesTitle}</h2>
      <label className="node-properties__row">
        <span className="node-properties__label">{m.nodeX}</span>
        <input
          className="node-properties__input"
          onChange={(event) => setFields({ ...fields, x: event.target.value })}
          onKeyDown={onKeyDown}
          ref={first}
          value={fields.x}
        />
        <span className="node-properties__unit">{unit}</span>
      </label>
      <label className="node-properties__row">
        <span className="node-properties__label">{m.nodeY}</span>
        <input
          className="node-properties__input"
          onChange={(event) => setFields({ ...fields, y: event.target.value })}
          onKeyDown={onKeyDown}
          value={fields.y}
        />
        <span className="node-properties__unit">{unit}</span>
      </label>
      <div className="node-properties__buttons">
        <button
          className="node-properties__button"
          disabled={okWhy !== undefined}
          type="submit"
          data-tip={whyTip(m.ok, okWhy)}
        >
          <ButtonFace icon="formApply" label={m.ok} />
        </button>
        <button
          className="node-properties__button"
          onClick={close}
          type="button"
          data-tip={m.cancel}
        >
          <ButtonFace icon="dismiss" label={m.cancel} />
        </button>
      </div>
    </form>
  );
}
