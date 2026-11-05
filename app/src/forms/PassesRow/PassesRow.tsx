import { useState, type JSX } from 'react';

import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import { passValues, plannedDepths, type PassState } from '../../lib/passFields';
import { EditPassesDialog } from '../EditPassesDialog/EditPassesDialog';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './PassesRow.scss';

const text = strings.toolpaths.passes;

interface PassesRowProps {
  state: PassState;
  onChange: (state: PassState) => void;
  /** Millimetres below the surface; null while the depth fields are invalid. */
  startDepth: number | null;
  cutDepth: number | null;
  /** The tool's pass depth in millimetres; null before a tool is chosen. */
  toolPassDepth: number | null;
  units: Units;
}

/** Number of passes the toolpath will cut, with Edit Passes... to set them. */
export function PassesRow(props: PassesRowProps): JSX.Element {
  const { state, onChange, startDepth, cutDepth, toolPassDepth, units } = props;
  const [editing, setEditing] = useState(false);
  const ready = startDepth !== null && cutDepth !== null && toolPassDepth !== null;
  const depths = ready
    ? plannedDepths(startDepth, cutDepth, toolPassDepth, passValues(state, units, startDepth))
    : null;
  const editWhy = depths === null ? strings.why.passDepths : undefined;
  return (
    <div className="passes-row">
      <span className="passes-row__count">{text.summary(depths?.length ?? null)}</span>
      <button
        className="passes-row__button"
        disabled={editWhy !== undefined}
        onClick={() => setEditing(true)}
        type="button"
        data-tip={whyTip(text.edit, editWhy)}
      >
        <ButtonFace icon="itemEdit" label={text.edit} />
      </button>
      {editing && ready ? (
        <EditPassesDialog
          onClose={(next) => {
            setEditing(false);
            if (next !== null) {
              onChange(next);
            }
          }}
          passes={state}
          span={{ span: cutDepth - startDepth, toolPassDepth }}
          units={units}
        />
      ) : null}
    </div>
  );
}
