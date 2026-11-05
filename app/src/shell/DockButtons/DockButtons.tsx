import type { JSX } from 'react';

import { dockTopic } from '../../lib/helpTopics';
import { strings } from '../../lib/strings';
import { useDialogStore } from '../../state/dialogStore';
import { useUiStore } from '../../state/uiStore';

import './DockButtons.scss';

interface DockButtonsProps {
  /** The dock the buttons sit in; its layout button switches to the other dock's layout. */
  side: 'left' | 'right';
}

/**
 * The Switch Layout and help buttons in a dock's title area, before its pin:
 * the tool panel's switches to the machining layout (F12), the Toolpaths
 * panel's to the design layout (F11). Help opens on the form open in the
 * dock, else on the dock's page.
 */
export function DockButtons({ side }: DockButtonsProps): JSX.Element {
  const machiningLayout = useUiStore((state) => state.machiningLayout);
  const designLayout = useUiStore((state) => state.designLayout);
  const layoutLabel = side === 'left' ? strings.docks.toMachining : strings.docks.toDesign;
  return (
    <>
      <button
        aria-label={layoutLabel}
        className="dock-buttons__button"
        onClick={side === 'left' ? machiningLayout : designLayout}
        data-tip={layoutLabel}
        type="button"
      >
        <svg
          aria-hidden="true"
          className="dock-buttons__icon"
          focusable="false"
          viewBox="0 0 16 16"
        >
          {side === 'left' ? (
            <path d="M2 2h12v12H2zM11 2v12M4.5 5.5 7 8l-2.5 2.5" />
          ) : (
            <path d="M2 2h12v12H2zM5 2v12M11.5 5.5 9 8l2.5 2.5" />
          )}
        </svg>
      </button>
      <button
        aria-label={strings.docks.help}
        className="dock-buttons__button"
        onClick={() => {
          const ui = useUiStore.getState();
          useDialogStore.getState().showHelp(dockTopic(side, ui.activeForm, ui.activeTab));
        }}
        data-tip={strings.docks.help}
        type="button"
      >
        <span aria-hidden="true" className="dock-buttons__text">
          ?
        </span>
      </button>
    </>
  );
}
