import { Suspense, useMemo, type JSX } from 'react';

import { FORMS } from '../../forms/registry';
import { lazyNamed } from '../../lib/lazyNamed';
import { strings } from '../../lib/strings';
import { useDocumentStore } from '../../state/documentStore';
import { formDock } from '../../state/formDock';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore, type WorkspaceTab } from '../../state/uiStore';
import { DOCK_PANEL_ID, dockTabId } from '../DockTabs/DockTabs';
import { DockButtons } from '../DockButtons/DockButtons';
import { DockResizer } from '../DockResizer/DockResizer';
import { ErrorBoundary } from '../ErrorBoundary/ErrorBoundary';
import { FormHost } from '../FormHost/FormHost';
import { PinButton } from '../PinButton/PinButton';
import { ToolGroups } from '../ToolGroups/ToolGroups';
import { TOOL_GROUPS, visibleGroups, type DesignOption } from './toolGroups';

import './LeftPanel.scss';

// Every dock tab but Design loads on first use, so the start bundle stays small.
const ClipartPanel = lazyNamed(
  () => import('../../panels/ClipartPanel/ClipartPanel'),
  'ClipartPanel',
);
const ComponentsPanel = lazyNamed(
  () => import('../../panels/ComponentsPanel/ComponentsPanel'),
  'ComponentsPanel',
);
const LayersPanel = lazyNamed(() => import('../../panels/LayersPanel/LayersPanel'), 'LayersPanel');
const SheetsPanel = lazyNamed(() => import('../../panels/SheetsPanel/SheetsPanel'), 'SheetsPanel');

const TAB_TITLES: Record<WorkspaceTab, string> = {
  design: strings.tabs.design,
  sheets: strings.tabs.sheets,
  layers: strings.tabs.layers,
  components: strings.tabs.components,
  clipart: strings.tabs.clipart,
};

function Loading(): JSX.Element {
  return (
    <p className="left-panel__loading" role="status">
      {strings.form.loading}
    </p>
  );
}

/** The Design tab's groups, with the optional rows and groups that are on. */
function DesignTools(): JSX.Element {
  const fileOperations = usePrefsStore((state) => state.prefs.general.showFileOperations);
  const alignTools = usePrefsStore((state) => state.prefs.general.showAlignTools);
  const rotary = useDocumentStore((state) => state.job.kind === 'rotary');
  const groups = useMemo(() => {
    const on = new Set<DesignOption>();
    if (fileOperations) {
      on.add('fileOperations');
    }
    if (alignTools) {
      on.add('alignTools');
    }
    if (rotary) {
      on.add('rotaryJob');
    }
    return visibleGroups(TOOL_GROUPS.design, on);
  }, [fileOperations, alignTools, rotary]);
  return <ToolGroups groups={groups} />;
}

function TabContent({ tab }: { tab: WorkspaceTab }): JSX.Element {
  switch (tab) {
    case 'design':
      return (
        <div className="left-panel__scroll">
          <DesignTools />
        </div>
      );
    case 'components':
      return (
        <div className="left-panel__tree">
          <ErrorBoundary resetKey="components">
            <Suspense fallback={<Loading />}>
              <ComponentsPanel />
            </Suspense>
          </ErrorBoundary>
        </div>
      );
    case 'sheets':
      return (
        <ErrorBoundary resetKey="sheets">
          <Suspense fallback={<Loading />}>
            <SheetsPanel />
          </Suspense>
        </ErrorBoundary>
      );
    case 'layers':
      return (
        <div className="left-panel__scroll">
          <ErrorBoundary resetKey="layers">
            <Suspense fallback={<Loading />}>
              <LayersPanel />
            </Suspense>
          </ErrorBoundary>
        </div>
      );
    case 'clipart':
      return (
        <ErrorBoundary resetKey="clipart">
          <Suspense fallback={<Loading />}>
            <ClipartPanel />
          </Suspense>
        </ErrorBoundary>
      );
  }
}

/**
 * The left dock: the active tab's content, or the form opened from it.
 * Pinned, it stays open beside the view; unpinned, it hides behind the dock
 * tabs and slides out over the view while the pointer is over a tab or it.
 */
export function LeftPanel(): JSX.Element {
  const activeTab = useUiStore((state) => state.activeTab);
  const activeForm = useUiStore((state) => state.activeForm);
  const formSerial = useUiStore((state) => state.formSerial);
  const pinned = useUiStore((state) => state.leftPinned);
  const open = useUiStore((state) => state.leftOpen);
  const width = useUiStore((state) => state.leftWidth);
  const togglePinned = useUiStore((state) => state.toggleLeftPinned);
  const setOpen = useUiStore((state) => state.setLeftOpen);
  const form = activeForm !== null && formDock(activeForm) === 'left' ? FORMS[activeForm] : null;
  const visible = pinned || open || form !== null;

  const onLeave = () => {
    if (!pinned && form === null) {
      setOpen(false);
    }
  };

  return (
    <div className={pinned ? 'left-dock' : 'left-dock left-dock--auto-hide'}>
      <section
        aria-labelledby={dockTabId(activeTab)}
        className={pinned ? 'left-panel' : 'left-panel left-panel--floating'}
        hidden={!visible}
        id={DOCK_PANEL_ID}
        onPointerLeave={onLeave}
        role="tabpanel"
        style={{ width }}
      >
        <DockResizer controls={DOCK_PANEL_ID} side="left" />
        <header className="left-panel__header">
          <h2 className="left-panel__title">{TAB_TITLES[activeTab]}</h2>
          <div className="left-panel__buttons">
            <DockButtons side="left" />
            <PinButton
              onToggle={togglePinned}
              pinLabel={strings.docks.pinLeft}
              pinned={pinned}
              unpinLabel={strings.docks.unpinLeft}
            />
          </div>
        </header>
        {form !== null ? (
          <FormHost form={activeForm} title={form.title}>
            <ErrorBoundary resetKey={activeForm ?? ''}>
              <form.Component key={formSerial} />
            </ErrorBoundary>
          </FormHost>
        ) : (
          <TabContent tab={activeTab} />
        )}
      </section>
    </div>
  );
}
