import { useState, type JSX } from 'react';

import type { VectorFilter } from '../../bridge/sketch';
import { strings } from '../../lib/formStrings';
import { VectorSelector } from '../../panels/VectorSelector/VectorSelector';
import { isAutomatic } from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './VectorSelectionField.scss';

const text = strings.toolpaths.selector;

interface VectorSelectionFieldProps {
  filter: VectorFilter | null;
  onChange: (filter: VectorFilter | null) => void;
  /** The row's name; Vector Selection by default. */
  label?: string;
  /** Why the row is off, in plain words; it is on while this is not given. */
  offWhy?: string;
}

/** The Vector Selection row of a toolpath form: Manual or Automatic, and the Selector button. */
export function VectorSelectionField({
  filter,
  onChange,
  label,
  offWhy,
}: VectorSelectionFieldProps): JSX.Element {
  const [open, setOpen] = useState(false);
  return (
    <div className="vector-selection">
      <span className="vector-selection__label">{label ?? text.section}</span>
      <span className="vector-selection__status">
        {isAutomatic(filter) ? text.automatic : text.manual}
      </span>
      <button
        className="vector-selection__button"
        disabled={offWhy !== undefined}
        onClick={() => setOpen(true)}
        type="button"
        data-tip={whyTip(text.open, offWhy)}
      >
        <ButtonFace icon="vectorSelection" label={text.open} />
      </button>
      <VectorSelector
        filter={filter}
        forToolpath
        onClose={(next) => {
          setOpen(false);
          onChange(next);
        }}
        open={open}
      />
    </div>
  );
}
