import { useId, type JSX } from 'react';

import { toolDiameter, toolForToolpath, type Tool } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { toolGroups, useToolDatabase } from '../../lib/camData';
import { strings } from '../../lib/formStrings';
import { formatLength, unitSuffix } from '../../lib/units';

import './OptionalToolPicker.scss';

interface OptionalToolPickerProps {
  label: string;
  tool: Tool | null;
  onChange: (tool: Tool | null) => void;
  units: Units;
  disabled?: boolean;
  /** Shown under the list, for example why it is disabled. */
  hint?: string;
  /** A problem with the chosen tool that does not stop calculating. */
  warning?: string;
}

/** Chooses a second tool from the database, or none. Never picks one itself. */
export function OptionalToolPicker({
  label,
  tool,
  onChange,
  units,
  disabled,
  hint,
  warning,
}: OptionalToolPickerProps): JSX.Element {
  const database = useToolDatabase();
  const id = useId();
  const hintId = useId();
  const text = strings.toolpaths;
  const groups = toolGroups(database);
  const described = hint !== undefined || warning !== undefined;

  return (
    <div className="optional-tool-picker">
      <label className="optional-tool-picker__label" htmlFor={id}>
        {label}
      </label>
      <select
        aria-describedby={described ? hintId : undefined}
        className="optional-tool-picker__select"
        disabled={disabled === true || database === null}
        id={id}
        onChange={(event) => {
          const picked = database?.tools.find((t) => t.id === Number(event.target.value));
          onChange(picked === undefined ? null : toolForToolpath(picked));
        }}
        value={tool?.id ?? ''}
      >
        <option value="">{database === null ? text.loadingTools : text.noTool}</option>
        {groups.map(({ group, tools }) => (
          <optgroup key={group.id} label={group.label}>
            {tools.map((t) => (
              <option key={t.id} value={t.id}>
                {t.displayName ?? t.name}
              </option>
            ))}
          </optgroup>
        ))}
      </select>
      {tool === null || disabled === true ? null : (
        <p className="optional-tool-picker__summary">
          {text.toolSummary(
            formatLength(toolDiameter(tool), units),
            formatLength(tool.passDepth, units),
            formatLength(tool.stepover, units),
            unitSuffix(units),
          )}
        </p>
      )}
      {described ? (
        <p className="optional-tool-picker__hint" id={hintId}>
          {warning === undefined ? null : (
            <span className="optional-tool-picker__warning" role="status">
              {warning}
            </span>
          )}
          {warning !== undefined && hint !== undefined ? ' ' : null}
          {hint}
        </p>
      ) : null}
    </div>
  );
}
