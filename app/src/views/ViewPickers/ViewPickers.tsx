import { useEffect, type JSX, type ReactNode } from 'react';

import { strings } from '../../lib/strings';
import { useDocumentStore } from '../../state/documentStore';
import { useModelStore } from '../../state/modelStore';
import { activePicker, levelPicker, type Picker } from '../SnapBar/model';

import './ViewPickers.scss';

const text = strings.snapBar;

function icon(children: ReactNode): JSX.Element {
  return (
    <svg aria-hidden="true" className="view-pickers__icon" focusable="false" viewBox="0 0 24 24">
      {children}
    </svg>
  );
}

const SHEET_ICON = icon(<path d="M3 3h7v5H3zM12 3h9v5h-9zM3 10h13v11H3zM18 10h3v11h-3z" />);
const LAYER_ICON = icon(
  <>
    <path d="M4 9l8-4 8 4-8 4z" />
    <path d="M4 14l8 4 8-4" />
  </>,
);
const LEVEL_ICON = icon(
  <>
    <path className="view-pickers__tint" d="M4 10l8-5 8 5-8 5z" />
    <path d="M4 10l8-5 8 5-8 5zM4 10v5l8 5 8-5v-5" />
  </>,
);

interface PickerFieldProps {
  id: string;
  label: string;
  glyph: JSX.Element;
  picker: Picker;
  onPick: (id: number) => void;
}

function PickerField({ id, label, glyph, picker, onPick }: PickerFieldProps): JSX.Element {
  return (
    <div className="view-pickers__field">
      {glyph}
      <label className="view-pickers__sr-only" htmlFor={id}>
        {label}
      </label>
      <select
        className="view-pickers__select"
        disabled={picker.disabled}
        id={id}
        onChange={(event) => onPick(Number(event.target.value))}
        data-tip={label}
        value={picker.value ?? ''}
      >
        {picker.options.map((option) => (
          <option key={option.id} value={option.id}>
            {option.name}
          </option>
        ))}
      </select>
    </div>
  );
}

/** The Sheet, Layer and Level drop-downs in the middle of the document bar. */
export function ViewPickers(): JSX.Element {
  const sheets = useDocumentStore((state) => state.sheets);
  const layers = useDocumentStore((state) => state.layers);
  const setActiveSheet = useDocumentStore((state) => state.setActiveSheet);
  const setActiveLayer = useDocumentStore((state) => state.setActiveLayer);
  const levels = useModelStore((state) => state.levels);
  const activeLevel = useModelStore((state) => state.activeLevel);
  const setActiveLevel = useModelStore((state) => state.setActiveLevel);
  const levelsRead = levels.length > 0;
  useEffect(() => {
    // The levels arrive with the first model read, which otherwise waits
    // for the first document change.
    if (!levelsRead) {
      void useModelStore.getState().refresh();
    }
  }, [levelsRead]);
  return (
    <div aria-label={text.pickersGroup} className="view-pickers" role="group">
      <PickerField
        glyph={SHEET_ICON}
        id="view-toolbar-sheet"
        label={text.sheet}
        onPick={(id) => void setActiveSheet(id)}
        picker={activePicker(sheets)}
      />
      <PickerField
        glyph={LAYER_ICON}
        id="view-toolbar-layer"
        label={text.layer}
        onPick={(id) => void setActiveLayer(id)}
        picker={activePicker(layers)}
      />
      <PickerField
        glyph={LEVEL_ICON}
        id="view-toolbar-level"
        label={text.level}
        onPick={(id) => void setActiveLevel(id)}
        picker={levelPicker(levels, activeLevel)}
      />
    </div>
  );
}
