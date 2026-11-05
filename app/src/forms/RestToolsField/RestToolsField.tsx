import type { JSX, ReactNode } from 'react';

import { toolDiameter, type Tool } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import { formatLength, unitSuffix } from '../../lib/units';
import { OptionalToolPicker } from '../OptionalToolPicker/OptionalToolPicker';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';
import { moveWhy } from '../ButtonFace/why';

import './RestToolsField.scss';

interface RestToolsFieldProps {
  /** The tools after the first, in cutting order. */
  tools: readonly Tool[];
  onAdd: (tool: Tool) => void;
  onRemove: (index: number) => void;
  onMove: (index: number, by: -1 | 1) => void;
  units: Units;
  /** Shown while the list is empty; the rest machining hint by default. */
  emptyText?: string;
  /** Accessible name of the list. */
  listLabel?: string;
  /** Extra controls for a row, after its name. */
  renderExtra?: (tool: Tool, index: number) => ReactNode;
}

/** The later tools of a multi tool toolpath: a list in cutting order with a
 * way to add, remove and reorder them. */
export function RestToolsField({
  tools,
  onAdd,
  onRemove,
  onMove,
  units,
  emptyText,
  listLabel,
  renderExtra,
}: RestToolsFieldProps): JSX.Element {
  const text = strings.toolpaths.threeD;
  return (
    <div className="rest-tools">
      {tools.length === 0 ? (
        <p className="rest-tools__empty">{emptyText ?? text.noLaterTools}</p>
      ) : (
        <ol aria-label={listLabel ?? text.laterTools} className="rest-tools__list">
          {tools.map((tool, index) => (
            <li className="rest-tools__row" key={tool.id}>
              <span className="rest-tools__name">
                {tool.name}
                <span className="rest-tools__size">
                  {`${formatLength(toolDiameter(tool), units)} ${unitSuffix(units)}`}
                </span>
              </span>
              {renderExtra?.(tool, index)}
              <button
                aria-label={text.moveToolUp(tool.name)}
                data-tip={whyTip(text.moveToolUp(tool.name), moveWhy(index, tools.length, -1))}
                className="rest-tools__button"
                disabled={moveWhy(index, tools.length, -1) !== undefined}
                onClick={() => onMove(index, -1)}
                type="button"
              >
                <ButtonFace icon="arrowUp" />
              </button>
              <button
                aria-label={text.moveToolDown(tool.name)}
                data-tip={whyTip(text.moveToolDown(tool.name), moveWhy(index, tools.length, 1))}
                className="rest-tools__button"
                disabled={moveWhy(index, tools.length, 1) !== undefined}
                onClick={() => onMove(index, 1)}
                type="button"
              >
                <ButtonFace icon="arrowDown" />
              </button>
              <button
                aria-label={text.removeTool(tool.name)}
                className="rest-tools__button"
                onClick={() => onRemove(index)}
                type="button"
                data-tip={text.remove}
              >
                <ButtonFace icon="itemRemove" label={text.remove} />
              </button>
            </li>
          ))}
        </ol>
      )}
      <OptionalToolPicker
        label={text.addTool}
        onChange={(tool) => {
          if (tool !== null) {
            onAdd(tool);
          }
        }}
        tool={null}
        units={units}
      />
    </div>
  );
}
