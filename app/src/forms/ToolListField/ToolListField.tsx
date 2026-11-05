import { useEffect, useId, useState, type JSX, type ReactNode } from 'react';

import { toolDiameter, toolForToolpath, type Tool, type ToolKind } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { useToolDatabase } from '../../lib/camData';
import { strings } from '../../lib/formStrings';
import { formatLength, unitSuffix } from '../../lib/units';
import { useUiStore } from '../../state/uiStore';
import { ToolEditDialog } from '../ToolEditDialog/ToolEditDialog';
import { addTool, moveTool, removeTool, replaceTool } from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';
import { moveWhy } from '../ButtonFace/why';

import './ToolListField.scss';

interface ToolListFieldProps {
  /** The tools in cutting order. */
  tools: readonly Tool[];
  onChange: (tools: Tool[]) => void;
  units: Units;
  /** Heading of the list; "Tools" by default. */
  label?: string;
  /** Fill an empty list with a tool from the Tool Database, for a list that
   * must have one: the first tool of kind `prefer`, or the first tool. */
  autoPick?: boolean;
  prefer?: ToolKind;
  /** Keep at least this many tools: Remove is off at that count. */
  minimum?: number;
  /** The selected row, when the form keeps it (for per tool options). */
  selected?: number;
  onSelect?: (index: number) => void;
  /** Shown under the list for the selected tool, such as its own options. */
  renderSelected?: (tool: Tool, index: number) => ReactNode;
  /** Why the whole list is off, in plain words; it is on while this is not given. */
  offWhy?: string;
}

/**
 * An ordered list of the tools a toolpath cuts with, in cutting order: a row
 * per tool with Select ... to add one from the Tool Database, Remove, Edit ...
 * for this toolpath's copy of the selected tool, and up and down arrows.
 */
export function ToolListField({
  tools,
  onChange,
  units,
  label,
  autoPick = false,
  prefer,
  minimum = 0,
  selected,
  onSelect,
  renderSelected,
  offWhy,
}: ToolListFieldProps): JSX.Element {
  const text = strings.toolpaths.toolList;
  const database = useToolDatabase();
  const headingId = useId();
  const [ownSelected, setOwnSelected] = useState(0);
  const [editing, setEditing] = useState(false);
  const current = Math.min(selected ?? ownSelected, tools.length - 1);
  const select = (index: number) => {
    setOwnSelected(index);
    onSelect?.(index);
  };
  const tool = current >= 0 ? tools[current] : undefined;
  const upWhy = offWhy ?? moveWhy(current, tools.length, -1);
  const downWhy = offWhy ?? moveWhy(current, tools.length, 1);
  const removeWhy =
    offWhy ??
    firstWhy(
      [tool === undefined, strings.why.pickInList],
      [tools.length <= minimum, strings.why.keepTools(minimum)],
    );
  const editWhy = offWhy ?? (tool === undefined ? strings.why.pickInList : undefined);

  useEffect(() => {
    if (!autoPick || tools.length > 0 || database === null) {
      return;
    }
    const first =
      database.tools.find((t) => prefer !== undefined && t.toolType.kind === prefer) ??
      database.tools[0];
    if (first !== undefined) {
      onChange([toolForToolpath(first)]);
    }
  }, [autoPick, database, onChange, prefer, tools.length]);

  const apply = (next: { tools: Tool[]; index: number }) => {
    onChange(next.tools);
    select(Math.max(next.index, 0));
  };
  const unit = unitSuffix(units);
  const nameOf = (t: Tool) =>
    database?.tools.find((listed) => listed.id === t.id)?.displayName ?? t.displayName ?? t.name;

  return (
    <div className="tool-list">
      <span className="tool-list__label" id={headingId}>
        {label ?? text.label}
      </span>
      <div className="tool-list__body">
        {tools.length === 0 ? (
          <p className="tool-list__empty">{database === null ? text.loading : text.empty}</p>
        ) : (
          <ol aria-labelledby={headingId} className="tool-list__rows">
            {tools.map((t, index) => (
              <li className="tool-list__item" key={`${t.id}-${index}`}>
                <button
                  aria-pressed={index === current}
                  className={
                    index === current ? 'tool-list__row tool-list__row--selected' : 'tool-list__row'
                  }
                  data-tip={offWhy}
                  disabled={offWhy !== undefined}
                  onClick={() => select(index)}
                  onDoubleClick={() => {
                    select(index);
                    setEditing(true);
                  }}
                  type="button"
                >
                  <span className="tool-list__name">{nameOf(t)}</span>
                  <span className="tool-list__size">
                    {`${formatLength(toolDiameter(t), units)} ${unit}`}
                  </span>
                </button>
              </li>
            ))}
          </ol>
        )}
        <div className="tool-list__arrows">
          <button
            aria-label={text.up}
            className="tool-list__arrow"
            disabled={upWhy !== undefined}
            onClick={() => apply(moveTool(tools, current, -1))}
            data-tip={whyTip(text.up, upWhy)}
            type="button"
          >
            <ButtonFace icon="arrowUp" />
          </button>
          <button
            aria-label={text.down}
            className="tool-list__arrow"
            disabled={downWhy !== undefined}
            onClick={() => apply(moveTool(tools, current, 1))}
            data-tip={whyTip(text.down, downWhy)}
            type="button"
          >
            <ButtonFace icon="arrowDown" />
          </button>
        </div>
      </div>
      <div className="tool-list__buttons">
        <button
          className="tool-list__button"
          disabled={offWhy !== undefined}
          onClick={() =>
            useUiStore.getState().openToolPicker((picked) => apply(addTool(tools, picked)))
          }
          type="button"
          data-tip={whyTip(text.select, offWhy)}
        >
          <ButtonFace icon="toolDatabase" label={text.select} />
        </button>
        <button
          className="tool-list__button"
          disabled={removeWhy !== undefined}
          onClick={() => apply(removeTool(tools, current))}
          type="button"
          data-tip={whyTip(text.remove, removeWhy)}
        >
          <ButtonFace icon="itemRemove" label={text.remove} />
        </button>
        <button
          className="tool-list__button"
          disabled={editWhy !== undefined}
          onClick={() => setEditing(true)}
          type="button"
          data-tip={whyTip(text.edit, editWhy)}
        >
          <ButtonFace icon="itemEdit" label={text.edit} />
        </button>
      </div>
      {tool === undefined ? null : (
        <p className="tool-list__summary">
          {strings.toolpaths.toolSummary(
            formatLength(toolDiameter(tool), units),
            formatLength(tool.passDepth, units),
            formatLength(tool.stepover, units),
            unit,
          )}
        </p>
      )}
      {tool === undefined ? null : renderSelected?.(tool, current)}
      {editing && tool !== undefined ? (
        <ToolEditDialog
          onClose={(edited) => {
            setEditing(false);
            if (edited !== null) {
              onChange(replaceTool(tools, current, edited));
            }
          }}
          tool={tool}
          units={units}
        />
      ) : null}
    </div>
  );
}
