import { useState, type JSX } from 'react';

import { strings } from '../../lib/formStrings';
import type { PlacedMatrix } from '../../lib/transformMath';
import { useDocumentStore } from '../../state/documentStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { FormButtons } from '../FormButtons/FormButtons';
import { IconButton } from '../IconButton/IconButton';
import {
  ALIGN_BUTTONS,
  MATERIAL_BUTTONS,
  SPACE_BUTTONS,
  spaceInOrder,
  spaceInside,
  initialAlignState,
} from './model';

import './AlignForm.scss';

export function AlignForm(): JSX.Element {
  const selectedIds = useDocumentStore((store) => store.selection.ids);
  const entities = useDocumentStore((store) => store.entities);
  const transformSelection = useDocumentStore((store) => store.transformSelection);
  const selectedCount = selectedIds.length;
  const alignSelection = useDocumentStore((store) => store.alignSelection);
  const distributeSelection = useDocumentStore((store) => store.distributeSelection);
  const closeForm = useUiStore((store) => store.closeForm);
  const showOnDesignTab = usePrefsStore((store) => store.prefs.general.showAlignTools);
  const updatePrefs = usePrefsStore((store) => store.update);
  const text = strings.transform.align;
  const [state, setState] = useState(initialAlignState);
  const canAlign = selectedCount >= 1;
  // The alignments that move objects against another always line them up
  // with the last one selected.
  const canAlignToLast = selectedCount >= 2;

  /** Space Selection: inside the last object, in selection order with Ctrl, or evenly. */
  const space = async (horizontal: boolean, inOrder: boolean) => {
    const placed = selectedIds
      .map((id) => entities.find((e) => e.id === id))
      .filter((e) => e !== undefined)
      .map((e) => ({ id: e.id, bbox: e.bbox }));
    let moves: PlacedMatrix[] | null = null;
    if (state.insideLast) {
      const last = placed[placed.length - 1];
      moves = last === undefined ? null : spaceInside(placed.slice(0, -1), last.bbox, horizontal);
    } else if (inOrder) {
      moves = spaceInOrder(placed, horizontal);
    }
    if (moves === null) {
      await distributeSelection(horizontal ? 'hGap' : 'vGap', null);
      return;
    }
    for (const move of moves) {
      await transformSelection(move.matrix, [move.id]);
    }
  };

  return (
    <div className="align-form">
      {canAlign ? null : <p className="align-form__notice">{strings.transform.noSelection}</p>}
      <fieldset className="align-form__group">
        <legend className="align-form__legend">{text.material}</legend>
        <div className="align-form__grid">
          {MATERIAL_BUTTONS.map((button) => (
            <IconButton
              why={canAlign ? undefined : strings.transform.noSelection}
              icon={button.icon}
              key={button.mode}
              label={button.short}
              onClick={() => void alignSelection(button.mode, 'material')}
              title={button.label}
            />
          ))}
        </div>
      </fieldset>
      <fieldset className="align-form__group">
        <legend className="align-form__legend">{text.selection}</legend>
        <div className="align-form__grid">
          {ALIGN_BUTTONS.map((button) => (
            <IconButton
              why={canAlignToLast ? undefined : strings.why.twoVectors}
              icon={button.icon}
              key={button.mode}
              label={button.short}
              onClick={() => void alignSelection(button.mode, 'last')}
              title={button.label}
            />
          ))}
        </div>
      </fieldset>
      <fieldset className="align-form__group">
        <legend className="align-form__legend">{text.distribute}</legend>
        <div className="align-form__grid">
          {SPACE_BUTTONS.map((button) => (
            <IconButton
              why={selectedCount < 2 ? strings.why.twoVectors : undefined}
              icon={button.icon}
              key={button.icon}
              label={button.short}
              onClick={(event) => void space(button.horizontal, event.ctrlKey || event.metaKey)}
              title={button.label}
            />
          ))}
        </div>
        <label className="align-form__choice">
          <input
            checked={state.insideLast}
            onChange={(event) =>
              setState((current) => ({ ...current, insideLast: event.target.checked }))
            }
            type="checkbox"
          />
          {text.insideLast}
        </label>
        <p className="align-form__notice">{text.spaceHint}</p>
      </fieldset>
      <label className="align-form__choice">
        <input
          checked={showOnDesignTab}
          onChange={(event) =>
            void updatePrefs({ general: { showAlignTools: event.target.checked } })
          }
          type="checkbox"
        />
        {text.showOnDesignTab}
      </label>
      <FormButtons onClose={closeForm} />
    </div>
  );
}
