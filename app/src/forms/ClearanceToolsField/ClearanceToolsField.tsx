import { useState, type JSX } from 'react';

import type { Tool } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { validateClearance } from '../../lib/clearanceFields';
import { strings } from '../../lib/formStrings';
import { CheckField } from '../CheckField/CheckField';
import { ClearanceOptionsFields } from '../ClearanceOptionsFields/ClearanceOptionsFields';
import { ToolListField } from '../ToolListField/ToolListField';
import { canSharpen, firstToolWarning, syncEntries, type ClearanceEntry } from './model';

import './ClearanceToolsField.scss';

const text = strings.toolpaths.vgroove;

interface ClearanceToolsFieldProps {
  /** Use Clearance Tools is ticked. */
  enabled: boolean;
  onEnable: (enabled: boolean) => void;
  entries: readonly ClearanceEntry[];
  onChange: (entries: ClearanceEntry[]) => void;
  units: Units;
  /** Offer Corner Sharpen for a later V shaped tool. */
  sharpen?: boolean;
  /** Show the selected tool's own options under the list. */
  options?: boolean;
  showErrors?: boolean;
}

/**
 * Use Clearance Tools: the ordered list of clearance tools and, under it,
 * the selected tool's own options.
 */
export function ClearanceToolsField({
  enabled,
  onEnable,
  entries,
  onChange,
  units,
  sharpen = false,
  options: showOptions = true,
  showErrors = false,
}: ClearanceToolsFieldProps): JSX.Element {
  const [selected, setSelected] = useState(0);
  const tools = entries.map((entry) => entry.tool);
  const warning = enabled ? firstToolWarning(entries) : undefined;
  const update = (index: number, change: Partial<ClearanceEntry>) =>
    onChange(entries.map((entry, i) => (i === index ? { ...entry, ...change } : entry)));

  const options = (tool: Tool, index: number) => {
    const entry = entries[index];
    if (entry === undefined) {
      return null;
    }
    return (
      <div className="clearance-tools__options" role="group" aria-label={text.toolOptions}>
        <span className="clearance-tools__heading">{text.toolOptions}</span>
        <ClearanceOptionsFields
          disabled={!enabled}
          errors={showErrors ? validateClearance(entry.clearance, units) : {}}
          onChange={(clearance) => update(index, { clearance })}
          state={entry.clearance}
          strategyDisabled={index > 0}
          units={units}
        />
        {sharpen ? (
          <CheckField
            checked={entry.cornerSharpen}
            disabled={!enabled || index === 0 || !canSharpen(tool)}
            label={text.cornerSharpen}
            onChange={(cornerSharpen) => update(index, { cornerSharpen })}
          />
        ) : null}
      </div>
    );
  };

  return (
    <fieldset className="clearance-tools">
      <legend className="clearance-tools__legend">
        <CheckField checked={enabled} label={text.clearance} onChange={onEnable} />
      </legend>
      <ToolListField
        label={text.clearanceTools}
        offWhy={enabled ? undefined : strings.why.tickFirst(text.clearance)}
        onChange={(next) => onChange(syncEntries(entries, next, units))}
        onSelect={setSelected}
        renderSelected={showOptions ? options : undefined}
        selected={selected}
        tools={tools}
        units={units}
      />
      {warning === undefined ? null : (
        <p className="clearance-tools__notice" role="status">
          {warning}
        </p>
      )}
    </fieldset>
  );
}
