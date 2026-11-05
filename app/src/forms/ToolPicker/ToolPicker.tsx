import { useEffect, useId, useState, type JSX } from 'react';

import { toolDiameter, toolForToolpath, type Tool, type ToolKind } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { useToolDatabase } from '../../lib/camData';
import { strings } from '../../lib/formStrings';
import { formatLength, unitSuffix } from '../../lib/units';
import { useUiStore } from '../../state/uiStore';
import { ToolEditDialog } from '../ToolEditDialog/ToolEditDialog';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './ToolPicker.scss';

interface ToolPickerProps {
  tool: Tool | null;
  onChange: (tool: Tool) => void;
  units: Units;
  /** Picked when nothing is chosen yet: the first tool of this kind. */
  prefer?: Tool['toolType']['kind'];
  /** Picked first when nothing is chosen yet: a tool this wide, mm. */
  preferDiameter?: number;
  /** The only kinds of tool the form cuts with; Select ... lists just these. */
  kinds?: readonly ToolKind[];
}

/**
 * The tool row of a toolpath form: the tool's name, Select ... to pick one
 * from the Tool Database and Edit ... to change this toolpath's copy of it.
 */
export function ToolPicker({
  tool,
  onChange,
  units,
  prefer,
  preferDiameter,
  kinds,
}: ToolPickerProps): JSX.Element {
  const database = useToolDatabase();
  const labelId = useId();
  const [editing, setEditing] = useState(false);

  useEffect(() => {
    if (tool !== null || database === null) {
      return;
    }
    const offered =
      kinds === undefined
        ? database.tools
        : database.tools.filter((t) => kinds.includes(t.toolType.kind));
    const ofKind = (t: Tool) => prefer === undefined || t.toolType.kind === prefer;
    const sized =
      preferDiameter === undefined
        ? undefined
        : offered.find((t) => ofKind(t) && Math.abs(toolDiameter(t) - preferDiameter) < 1e-3);
    const first =
      sized ??
      offered.find((t) => prefer !== undefined && t.toolType.kind === prefer) ??
      offered[0];
    if (first !== undefined) {
      onChange(toolForToolpath(first));
    }
  }, [database, kinds, onChange, prefer, preferDiameter, tool]);

  const unit = unitSuffix(units);
  const text = strings.toolpaths;
  const listed = tool === null ? undefined : database?.tools.find((t) => t.id === tool.id);
  const name =
    tool === null
      ? database === null
        ? text.loadingTools
        : text.toolRow.none
      : (listed?.displayName ?? tool.displayName ?? tool.name);

  const editWhy = tool === null ? text.needsTool : undefined;
  return (
    <div className="tool-picker">
      <span className="tool-picker__label" id={labelId}>
        {text.tool}
      </span>
      <output aria-labelledby={labelId} className="tool-picker__name">
        {name}
      </output>
      <div className="tool-picker__buttons">
        <button
          className="tool-picker__button"
          onClick={() => useUiStore.getState().openToolPicker(onChange, kinds)}
          type="button"
          data-tip={text.toolRow.select}
        >
          <ButtonFace icon="toolDatabase" label={text.toolRow.select} />
        </button>
        <button
          className="tool-picker__button"
          disabled={editWhy !== undefined}
          onClick={() => setEditing(true)}
          type="button"
          data-tip={whyTip(text.toolRow.edit, editWhy)}
        >
          <ButtonFace icon="itemEdit" label={text.toolRow.edit} />
        </button>
      </div>
      {tool === null ? null : (
        <p className="tool-picker__summary">
          {text.toolSummary(
            formatLength(toolDiameter(tool), units),
            formatLength(tool.passDepth, units),
            formatLength(tool.stepover, units),
            unit,
          )}
        </p>
      )}
      {editing && tool !== null ? (
        <ToolEditDialog
          onClose={(edited) => {
            setEditing(false);
            if (edited !== null) {
              onChange(edited);
            }
          }}
          tool={tool}
          units={units}
        />
      ) : null}
    </div>
  );
}
