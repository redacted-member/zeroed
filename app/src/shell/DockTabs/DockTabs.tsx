import { useRef, type JSX, type KeyboardEvent, type PointerEvent } from 'react';

import { strings } from '../../lib/strings';
import { useUiStore, type WorkspaceTab } from '../../state/uiStore';

import './DockTabs.scss';

const TABS: ReadonlyArray<{ id: WorkspaceTab; label: string; key?: string }> = [
  { id: 'design', label: strings.tabs.design, key: 'F11' },
  { id: 'sheets', label: strings.tabs.sheets },
  { id: 'layers', label: strings.tabs.layers },
  { id: 'components', label: strings.tabs.components },
  { id: 'clipart', label: strings.tabs.clipart },
];

/** The id of the left panel the dock tabs control. */
export const DOCK_PANEL_ID = 'dock-panel';

/** The id of the tab button for `tab`. */
export function dockTabId(tab: WorkspaceTab): string {
  return `dock-tab-${tab}`;
}

/** The vertical tab strip down the left edge of the window. */
export function DockTabs(): JSX.Element {
  const activeTab = useUiStore((state) => state.activeTab);
  const setActiveTab = useUiStore((state) => state.setActiveTab);
  const pinned = useUiStore((state) => state.leftPinned);
  const setLeftOpen = useUiStore((state) => state.setLeftOpen);
  const listRef = useRef<HTMLDivElement | null>(null);

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    const index = TABS.findIndex((tab) => tab.id === activeTab);
    let next: number | null = null;
    if (event.key === 'ArrowDown' || event.key === 'ArrowRight') {
      next = (index + 1) % TABS.length;
    } else if (event.key === 'ArrowUp' || event.key === 'ArrowLeft') {
      next = (index - 1 + TABS.length) % TABS.length;
    } else if (event.key === 'Home') {
      next = 0;
    } else if (event.key === 'End') {
      next = TABS.length - 1;
    }
    const tab = next === null ? undefined : TABS[next];
    if (tab !== undefined) {
      event.preventDefault();
      setActiveTab(tab.id);
      if (!pinned) {
        setLeftOpen(true);
      }
      listRef.current?.querySelector<HTMLButtonElement>(`#${dockTabId(tab.id)}`)?.focus();
    }
  };

  const choose = (tab: WorkspaceTab) => {
    setActiveTab(tab);
    if (!pinned) {
      setLeftOpen(true);
    }
  };

  // Hovering an auto hidden tab shows its page, but never closes an open form.
  const peek = (tab: WorkspaceTab) => {
    if (useUiStore.getState().activeForm === null) {
      setActiveTab(tab);
    }
    setLeftOpen(true);
  };

  // Unpinned, leaving the tabs for anywhere but the panel hides it again.
  const onPointerLeave = (event: PointerEvent<HTMLDivElement>) => {
    const to = event.relatedTarget;
    const panel = document.getElementById(DOCK_PANEL_ID);
    if (!pinned && !(to instanceof Node && panel?.contains(to) === true)) {
      if (useUiStore.getState().activeForm === null) {
        setLeftOpen(false);
      }
    }
  };

  return (
    <div
      onPointerLeave={onPointerLeave}
      aria-label={strings.tabs.label}
      aria-orientation="vertical"
      className="dock-tabs"
      ref={listRef}
      role="tablist"
    >
      {TABS.map(({ id, label, key }) => (
        <button
          aria-controls={DOCK_PANEL_ID}
          aria-selected={activeTab === id}
          className="dock-tabs__tab"
          id={dockTabId(id)}
          key={id}
          onClick={() => choose(id)}
          onKeyDown={onKeyDown}
          onPointerEnter={pinned ? undefined : () => peek(id)}
          role="tab"
          tabIndex={activeTab === id ? 0 : -1}
          data-tip={key === undefined ? label : strings.tabs.hint(label, key)}
          type="button"
        >
          <span className="dock-tabs__label">{label}</span>
        </button>
      ))}
    </div>
  );
}
