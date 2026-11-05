import { lazy, Suspense, useEffect, useRef, type JSX } from 'react';

import { FORMS } from '../../forms/registry';
import { strings } from '../../lib/strings';
import { ToolpathsPanel } from '../../panels/ToolpathsPanel/ToolpathsPanel';
import { formDock } from '../../state/formDock';
import { useDocumentStore } from '../../state/documentStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { DockButtons } from '../DockButtons/DockButtons';
import { DockResizer } from '../DockResizer/DockResizer';
import { ErrorBoundary } from '../ErrorBoundary/ErrorBoundary';
import { FormHost } from '../FormHost/FormHost';
import { ListResizer } from '../ListResizer/ListResizer';
import { MaterialSummary } from '../MaterialSummary/MaterialSummary';
import { PinButton } from '../PinButton/PinButton';
import { TOOL_GROUPS } from '../LeftPanel/toolGroups';
import { ToolGroups } from '../ToolGroups/ToolGroups';

import './RightPanel.scss';

// Only rotary jobs show it, so it stays out of the start bundle.
const RotaryGuide = lazy(() =>
  import('../RotaryGuide/RotaryGuide').then((module) => ({ default: module.RotaryGuide })),
);

const text = strings.rightPanel;

/** The id of the Toolpaths panel, which its resize edge controls. */
const PANEL_ID = 'toolpaths-panel';

/**
 * The Toolpaths panel docked on the right: the toolpath operation groups
 * over the toolpath list, or the open toolpath form in their place. Unpinned,
 * it hides to a tab on the window edge and slides out over the view.
 */
export function RightPanel(): JSX.Element {
  const rotary = useDocumentStore((state) => state.job.kind === 'rotary');
  const activeForm = useUiStore((state) => state.activeForm);
  const formSerial = useUiStore((state) => state.formSerial);
  const pinned = useUiStore((state) => state.toolpathsPinned);
  const open = useUiStore((state) => state.toolpathsOpen);
  const focusRequest = useUiStore((state) => state.toolpathsFocus);
  const togglePinned = useUiStore((state) => state.toggleToolpathsPinned);
  const setOpen = useUiStore((state) => state.setToolpathsOpen);
  const showToolpaths = useUiStore((state) => state.showToolpaths);
  const width = useUiStore((state) => state.rightWidth);
  const listHeight = useUiStore((state) => state.toolpathListHeight);
  const listRef = useRef<HTMLElement | null>(null);
  const panelRef = useRef<HTMLElement | null>(null);
  const form = activeForm !== null && formDock(activeForm) === 'right' ? FORMS[activeForm] : null;
  // Preferences > Toolpaths > Show Toolpath Operations with Preview.
  const withOperations = usePrefsStore((state) => state.prefs.toolpaths.showOperationsWithPreview);
  const operationsAbove = form !== null && activeForm === 'previewToolpaths' && withOperations;
  const visible = pinned || open;

  useEffect(() => {
    if (focusRequest > 0) {
      panelRef.current?.querySelector<HTMLElement>('button, input, select')?.focus();
    }
  }, [focusRequest]);

  const onLeave = () => {
    if (!pinned && form === null) {
      setOpen(false);
    }
  };

  return (
    <div
      className={pinned ? 'right-dock' : 'right-dock right-dock--auto-hide'}
      data-guide="toolpaths"
    >
      {pinned ? null : (
        <button
          aria-expanded={open}
          className="right-dock__tab"
          onClick={() => (open ? setOpen(false) : showToolpaths())}
          onPointerEnter={() => setOpen(true)}
          data-tip={text.show}
          type="button"
        >
          <span className="right-dock__tab-label">{strings.tabs.toolpaths}</span>
        </button>
      )}
      {visible ? (
        <aside
          aria-labelledby="right-panel-title"
          className={pinned ? 'right-panel' : 'right-panel right-panel--floating'}
          id={PANEL_ID}
          onPointerLeave={onLeave}
          ref={panelRef}
          style={{ width }}
        >
          <DockResizer controls={PANEL_ID} side="right" />
          <header className="right-panel__header">
            <h2 className="right-panel__title" id="right-panel-title">
              {text.label}
            </h2>
            <div className="right-panel__buttons">
              <DockButtons side="right" />
              <PinButton
                onToggle={togglePinned}
                pinLabel={text.pin}
                pinned={pinned}
                unpinLabel={text.unpin}
              />
            </div>
          </header>
          {operationsAbove ? (
            <div className="right-panel__operations right-panel__operations--above-form">
              <ToolGroups groups={TOOL_GROUPS.toolpaths} />
            </div>
          ) : null}
          {form !== null ? (
            <FormHost form={activeForm} title={form.title}>
              <ErrorBoundary resetKey={activeForm ?? ''}>
                <form.Component key={formSerial} />
              </ErrorBoundary>
            </FormHost>
          ) : (
            <>
              <MaterialSummary />
              <div className="right-panel__operations">
                {rotary ? (
                  <Suspense fallback={null}>
                    <RotaryGuide />
                  </Suspense>
                ) : null}
                <ToolGroups groups={TOOL_GROUPS.toolpaths} />
              </div>
              <ListResizer
                controls="toolpath-list"
                list="toolpaths"
                measure={() => listRef.current?.getBoundingClientRect().height ?? 0}
              />
              <section
                aria-labelledby="toolpath-list-title"
                className={
                  listHeight > 0
                    ? 'right-panel__list right-panel__list--sized'
                    : 'right-panel__list'
                }
                id="toolpath-list"
                ref={listRef}
                style={listHeight > 0 ? { flexBasis: listHeight } : undefined}
              >
                <div className="right-panel__list-body">
                  <ErrorBoundary resetKey="toolpaths">
                    <ToolpathsPanel />
                  </ErrorBoundary>
                </div>
              </section>
            </>
          )}
        </aside>
      ) : null}
    </div>
  );
}
