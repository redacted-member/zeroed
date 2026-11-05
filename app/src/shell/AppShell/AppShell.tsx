import { useCallback, useEffect, type JSX } from 'react';

import { getKernel, isTauriHost } from '../../bridge';
import { api } from '../../bridge/api';
import type { Unsubscribe } from '../../bridge/types';
import {
  exportVectors,
  importVectors,
  newFromTemplate,
  newJob,
  openJob,
  openRecent,
  saveJob,
  saveAsTemplate,
  saveJobAs,
} from '../../lib/fileCommands';
import { importBitmap } from '../../lib/bitmapCommands';
import { modalDialogOpen } from '../../lib/dialogKeys';
import { commandEnabled } from '../../lib/enableState';
import { runFieldEdit } from '../../lib/fieldEditing';
import {
  TRANSFORM_FORM_KEYS,
  commandForKey,
  nudgeForKey,
  type AppCommand,
} from '../../lib/shortcuts';
import { nodeToggle } from '../../lib/quickNodeEdit';
import { saveViewImage } from '../../lib/viewImage';
import { watchWindowTitle } from '../../lib/windowTitle';
import { offerRecovery } from '../../lib/crashRecovery';
import { isDoubleSided, sideRequest } from '../../lib/sides';
import { strings } from '../../lib/strings';
import { useDocumentStore } from '../../state/documentStore';
import { currentEnableState } from '../../state/enableSnapshot';
import { useGadgetStore } from '../../state/gadgetStore';
import { pickedComponents, useModelStore } from '../../state/modelStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useView3dStore } from '../../state/view3dStore';
import type { ViewPreset } from '../../views/View3D/camera3d';
import { startAutosave } from '../../state/autosave';
import { useVariablesStore, watchVariables } from '../../state/variablesStore';
import { watchGuides } from '../../state/guidesSync';
import { usePrefsStore, watchLayout } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { jobBounds } from '../../views/View2D/camera';
import { centreOnMaterial, zoomAllBounds } from '../../lib/zoomAll';
import { ViewArea } from '../../views/ViewArea/ViewArea';
import { LeftPanel } from '../LeftPanel/LeftPanel';
import { RightPanel } from '../RightPanel/RightPanel';
import { PrintPreview } from '../../panels/PrintPreview/PrintPreview';
import {
  duplicateToolpath,
  openToolpathEditor,
  selectedToolpath,
} from '../../panels/ToolpathsPanel/actions';
import { PrintView } from '../../views/PrintView/PrintView';
import { useDialogStore } from '../../state/dialogStore';
import { useGuideStore } from '../../state/guideStore';
import { contextTopic } from '../../lib/helpTopics';
import { GadgetsMenu } from '../GadgetsMenu/GadgetsMenu';
import { LazyPanels } from '../LazyPanels/LazyPanels';
import { DockTabs } from '../DockTabs/DockTabs';
import { MenuBar } from '../MenuBar/MenuBar';
import { watchHeldModifiers } from '../../lib/heldModifiers';
import { runMenuCommand } from '../MenuBar/menuCommands';
import { watchNativeMenu } from '../MenuBar/nativeMenu';
import { runMenuAction } from '../MenuBar/useMenuContext';
import { StatusBar } from '../StatusBar/StatusBar';
import { BusyShield } from '../BusyShield/BusyShield';
import { acceptsInput } from '../../state/busyStore';
import { Tooltip } from '../Tooltip/Tooltip';

import { afterPicks, picking } from '../../lib/pickOrder';
import './AppShell.scss';

function isTyping(event: KeyboardEvent): boolean {
  const target = event.target;
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    (target instanceof HTMLElement && target.isContentEditable) ||
    // A drop-down list such as the font list takes letters and arrows.
    (target instanceof HTMLElement && target.getAttribute('role') === 'combobox')
  );
}

const PRESETS: Partial<Record<AppCommand, ViewPreset>> = {
  'view3d.top': 'top',
  'view3d.bottom': 'bottom',
  'view3d.front': 'front',
  'view3d.back': 'back',
  'view3d.left': 'left',
  'view3d.right': 'right',
  'view3d.iso': 'iso',
};

const SHOW_3D: Partial<
  Record<AppCommand, 'showModel' | 'showBlock' | 'showToolpaths' | 'showVectors'>
> = {
  'view3d.showModel': 'showModel',
  'view3d.showBlock': 'showBlock',
  'view3d.showToolpaths': 'showToolpaths',
  'view3d.showVectors': 'showVectors',
};

/** Shows the 3D view if it is hidden, then runs `then` once it is mounted. */
function withView3d(then: () => void): void {
  const ui = useUiStore.getState();
  if (ui.viewMode === '3d' || ui.viewMode === 'split') {
    then();
    return;
  }
  ui.setViewMode('3d');
  requestAnimationFrame(() => requestAnimationFrame(then));
}

/** Asks, then removes every component from the model as one undo step. */
async function clearModel(): Promise<void> {
  const model = useModelStore.getState();
  if (
    model.components.length > 0 &&
    !(await getKernel().confirm(strings.modelShell.clearConfirm))
  ) {
    return;
  }
  await model.clear();
}

/**
 * Runs a command picked from a menu or a keyboard shortcut, unless the
 * menus grey it (lib/enableState). Tool icons never grey, so they call
 * runAppCommand directly and a tool that needs a selection says so.
 */
export async function runEnabledCommand(command: AppCommand): Promise<void> {
  if (!commandEnabled(command, currentEnableState()) && !(await undoNowEnabled(command))) {
    return;
  }
  await runAppCommand(command);
}

/** Undo and Redo grey by the undo state the last refresh read; a change made
 * without one would leave them greyed, so ask the kernel again first. */
async function undoNowEnabled(command: AppCommand): Promise<boolean> {
  if (command !== 'edit.undo' && command !== 'edit.redo') {
    return false;
  }
  try {
    useDocumentStore.setState({ undo: await api.undo.info() });
  } catch {
    return false;
  }
  return commandEnabled(command, currentEnableState());
}

/** Runs one application command against the stores. */
export async function runAppCommand(command: AppCommand): Promise<void> {
  // Native menu accelerators (Cmd+A, Cmd+Z, Delete) arrive here even while
  // a text box has focus; there they edit the text.
  if (runFieldEdit(command)) {
    return;
  }
  const doc = useDocumentStore.getState();
  const ui = useUiStore.getState();
  const view = useViewStore.getState();
  const preset = PRESETS[command];
  if (preset !== undefined) {
    withView3d(() => useView3dStore.getState().requestPreset(preset));
    return;
  }
  const show = SHOW_3D[command];
  if (show !== undefined) {
    const current = usePrefsStore.getState().prefs.view3d[show];
    await usePrefsStore.getState().update({ view3d: { [show]: !current } });
    return;
  }
  switch (command) {
    case 'file.new':
      if (await newJob()) {
        ui.openNewJobSetup();
      }
      return;
    case 'file.newFromTemplate':
      await newFromTemplate();
      return;
    case 'file.saveAsTemplate':
      saveAsTemplate();
      return;
    case 'file.open':
      await openJob();
      return;
    case 'file.save':
      await saveJob();
      return;
    case 'file.saveAs':
      await saveJobAs();
      return;
    case 'file.clearRecent':
      await usePrefsStore.getState().clearRecent();
      return;
    case 'app.preferences':
      usePrefsStore.getState().openDialog();
      return;
    case 'file.importVectors':
      await importVectors();
      return;
    case 'file.export':
      await exportVectors();
      return;
    case 'file.saveViewImage':
      await saveViewImage();
      return;
    case 'file.importBitmap':
      await importBitmap();
      return;
    case 'file.importModel':
    case 'model.importModel': {
      const { importModelCommand } = await import('../../lib/importModel');
      await importModelCommand();
      return;
    }
    case 'edit.jobSetup':
      ui.openForm('jobSetup');
      return;
    case 'edit.undo':
      await doc.undoLast();
      return;
    case 'edit.redo':
      await doc.redoLast();
      return;
    case 'edit.delete':
      await doc.deleteSelection();
      return;
    case 'edit.selectAll':
      await doc.selectAll();
      return;
    case 'edit.selectNone':
      await doc.clearSelection();
      return;
    case 'edit.copy':
      await doc.copySelection();
      return;
    case 'edit.cut':
      await doc.cutSelection();
      return;
    case 'edit.paste':
      await doc.paste();
      return;
    case 'edit.duplicate':
      await doc.duplicateSelection();
      return;
    case 'edit.group':
      await doc.groupSelection();
      return;
    case 'edit.ungroup':
      await doc.ungroupSelection();
      return;
    case 'edit.ungroupToGroupLayer':
      await doc.ungroupSelection({ toGroupLayer: true });
      return;
    case 'edit.ungroupDeep':
      await doc.ungroupSelection({ deep: true });
      return;
    case 'edit.ungroupDeepToGroupLayer':
      await doc.ungroupSelection({ deep: true, toGroupLayer: true });
      return;
    case 'edit.textToCurves':
      await doc.textToCurvesSelection();
      return;
    case 'edit.weld':
      await doc.booleanSelection('weld');
      return;
    case 'edit.subtract':
      await doc.booleanSelection('subtract');
      return;
    case 'edit.intersect':
      await doc.booleanSelection('intersect');
      return;
    case 'edit.xor':
      await doc.booleanSelection('xor');
      return;
    case 'edit.closeVectors':
      await doc.closeSelection('straight');
      return;
    case 'edit.closeSmooth':
      // The smooth method chosen in the preferences.
      await doc.closeSelection(
        usePrefsStore.getState().prefs.general.smoothJoinMethod === 'shallow'
          ? 'smoothShallow'
          : 'smooth',
      );
      return;
    case 'edit.closeMoveEnds':
      await doc.closeSelection('moveEnd');
      return;
    case 'edit.rotateCcw45':
      await doc.rotateSelection(45);
      return;
    case 'edit.rotateCw45':
      await doc.rotateSelection(-45);
      return;
    case 'edit.mirrorHorizontal':
      await doc.mirrorSelection('h', false);
      return;
    case 'edit.mirrorVertical':
      await doc.mirrorSelection('v', false);
      return;
    case 'edit.mirrorHorizontalCopy':
      await doc.mirrorSelection('h', true);
      return;
    case 'edit.mirrorVerticalCopy':
      await doc.mirrorSelection('v', true);
      return;
    case 'edit.mirrorMaterialHorizontal':
    case 'edit.mirrorMaterialVertical':
    case 'edit.mirrorMaterialHorizontalCopy':
    case 'edit.mirrorMaterialVerticalCopy': {
      const b = jobBounds(doc.job);
      const cx = (b.minX + b.maxX) / 2;
      const cy = (b.minY + b.maxY) / 2;
      const horizontal =
        command === 'edit.mirrorMaterialHorizontal' ||
        command === 'edit.mirrorMaterialHorizontalCopy';
      const matrix: [number, number, number, number, number, number] = horizontal
        ? [-1, 0, 0, 1, 2 * cx, 0]
        : [1, 0, 0, -1, 0, 2 * cy];
      if (command.endsWith('Copy')) {
        await doc.transformSelectionCopy(matrix);
      } else {
        await doc.transformSelection(matrix);
      }
      return;
    }
    case 'view.zoomToJob':
      view.zoomToJob(doc.job);
      return;
    case 'view.zoomExtents':
      view.zoomToBounds(zoomAllBounds());
      return;
    case 'edit.centreInMaterial': {
      const box = doc.selection.bbox;
      if (box !== null) {
        const { dx, dy } = centreOnMaterial(box, jobBounds(doc.job));
        await doc.moveSelection(dx, dy);
      }
      return;
    }
    case 'view.zoomToSelection':
    case 'view.centreSelection':
      view.zoomToBounds(doc.selection.bbox);
      return;
    case 'view.mode2d':
      ui.setViewMode('2d');
      return;
    case 'view.mode3d':
      ui.setViewMode('3d');
      return;
    case 'view.modeSplit':
      ui.setViewMode('split');
      return;
    case 'view.tileVertical':
    case 'view.tileHorizontal':
      ui.setViewMode('split');
      void usePrefsStore.getState().update({
        window: { tile: command === 'view.tileVertical' ? 'vertical' : 'horizontal' },
      });
      return;
    case 'view.clearGuides':
      view.clearGuides();
      return;
    case 'view3d.fit':
      withView3d(() => useView3dStore.getState().requestFit());
      return;
    case 'view3d.twiddle':
      withView3d(() => useView3dStore.getState().toggleTwiddle());
      return;
    case 'model.components':
      ui.setActiveTab('components');
      return;
    case 'model.modelSetup':
      // The modelling settings (resolution, appearance) live in Job Setup.
      ui.openForm('jobSetup');
      return;
    case 'model.clear':
      void clearModel();
      return;
    case 'toolpaths.edit': {
      const toolpath = selectedToolpath();
      if (toolpath !== null) {
        openToolpathEditor(toolpath);
      }
      return;
    }
    case 'toolpaths.duplicate': {
      const toolpath = selectedToolpath();
      if (toolpath !== null) {
        await duplicateToolpath(toolpath);
      }
      return;
    }
    case 'toolpaths.delete': {
      const toolpath = selectedToolpath();
      if (toolpath !== null) {
        await useToolpathStore.getState().remove(toolpath.id);
      }
      return;
    }
    case 'toolpaths.summary':
      useDialogStore.getState().show('machiningTime');
      return;
    case 'toolpaths.recalculateAll': {
      const toolpaths = useToolpathStore.getState();
      await toolpaths.recalculateMany(toolpaths.toolpaths.map((t) => t.id));
      return;
    }
    case 'toolpaths.save':
      ui.openForm('saveToolpaths');
      return;
    case 'toolpaths.materialSetup':
      ui.openForm('materialSetup');
      return;
    case 'gadgets.browse':
      ui.openGadgetsMenu();
      return;
    case 'gadgets.refresh':
      await useGadgetStore.getState().load();
      return;
    case 'gadgets.runLast':
      if (useGadgetStore.getState().current !== null) {
        ui.openForm('gadget');
      }
      return;
    case 'view.toggleGrid':
      view.toggleGrid();
      return;
    case 'view.toggleRulers':
      view.toggleRulers();
      return;
    case 'tool.select':
    case 'tool.node':
    case 'tool.transform': {
      // The Edit Vectors modes end a drawing tool and close its form.
      if (view.activeTool === 'create' && ui.activeForm !== null) {
        ui.closeForm();
      }
      if (command === 'tool.transform') {
        view.enterTransformMode();
      } else {
        view.setActiveTool(command === 'tool.node' ? 'node' : 'select');
      }
      return;
    }
    case 'tool.zoomBox':
      view.setActiveTool('zoomBox');
      return;
    case 'tabs.layers':
      ui.setActiveTab('layers');
      if (!ui.leftPinned) {
        ui.setLeftOpen(true);
      }
      return;
    case 'tabs.drawingTab':
      ui.setActiveTab('design');
      if (!ui.leftPinned) {
        ui.setLeftOpen(true);
      }
      return;
    case 'select.insideBoundary':
      await doc.selectInside();
      return;
    case 'bitmap.editPicture':
      ui.setActiveTab('design');
      ui.openForm('editPicture');
      return;
    case 'text.removeFromCurve':
      await doc.removeTextFromCurve();
      return;
    case 'text.adjust':
      ui.openForm('textSelection');
      return;
    case 'bitmap.crop':
      await doc.cropBitmap();
      return;
    case 'tool.nodeToggle': {
      const toggle = nodeToggle(view.activeTool, ui.activeForm !== null);
      view.setActiveTool(toggle.tool);
      if (toggle.closeForm) {
        ui.closeForm();
      }
      return;
    }
    case 'tabs.drawing':
      ui.designLayout();
      return;
    case 'tabs.toolpaths':
      ui.machiningLayout();
      return;
    case 'view.toolpathsPanel':
      ui.showToolpaths();
      return;
    case 'tabs.toggleDrawingModeling':
      ui.setActiveTab(ui.activeTab === 'design' ? 'components' : 'design');
      return;
    case 'gadgets.jointCreator':
      ui.setActiveTab('design');
      ui.openForm('jointCreator');
      return;
    case 'form.join':
    case 'form.measure':
    case 'form.move':
    case 'form.scale':
    case 'form.rotate':
    case 'form.align':
      ui.setActiveTab('design');
      ui.openForm(TRANSFORM_FORM_KEYS[command]);
      return;
    case 'toolpaths.preview':
      ui.openForm('previewToolpaths');
      return;
    case 'toolpaths.toolDatabase':
      ui.openToolDatabase();
      return;
    case 'machine.configuration':
      useDialogStore.getState().show('machineConfiguration');
      return;
    case 'help.contents':
      useDialogStore.getState().showHelp(contextTopic(ui.activeForm));
      return;
    case 'help.videoTutorials':
      useGuideStore.getState().showWelcome();
      return;
    case 'select.vectorSelector':
      useDialogStore.getState().show('vectorSelector');
      return;
    case 'help.shortcuts':
      useDialogStore.getState().show('shortcuts');
      return;
    case 'view.snapSettings':
      ui.openForm('snapSettings');
      return;
    case 'view.modelVolume': {
      // Shown in the 3D view; the same keys hide it again.
      if (ui.modelVolume !== null) {
        ui.setModelVolume(null);
        return;
      }
      const model = useModelStore.getState();
      const stamp = model.stamp;
      // Components picked in the tree are measured on their own.
      const { volume, empty, selected } = await api.model.refine.volume(pickedComponents(model));
      if (ui.viewMode === '2d') {
        ui.setViewMode('3d');
      }
      useUiStore.getState().setModelVolume({ volume, empty, selected, stamp });
      return;
    }
    case 'view.sideTop':
    case 'view.sideBottom': {
      const side = sideRequest(
        doc.job,
        doc.activeSide,
        command === 'view.sideTop' ? 'top' : 'bottom',
      );
      if (side !== null) {
        await doc.setSide(side);
      }
      return;
    }
    case 'edit.swapSides':
      if (isDoubleSided(doc.job)) {
        await doc.swapSides();
      }
      return;
    default:
      // Menu only commands; others arrive with later phases and are ignored.
      await runMenuCommand(command);
      return;
  }
}

export function AppShell(): JSX.Element {
  const connect = useDocumentStore((state) => state.connect);
  const openNewJobSetup = useUiStore((state) => state.openNewJobSetup);

  const runCommand = useCallback((command: AppCommand) => {
    void runAppCommand(command);
  }, []);

  useEffect(() => {
    let cancelled = false;
    void Promise.all([connect(), usePrefsStore.getState().load()]).then(async () => {
      void useVariablesStore
        .getState()
        .load()
        .catch(() => undefined);
      // After a crash the backup of an unsaved job may be restored first.
      const restored = await offerRecovery().catch(() => false);
      // A job opened from the Finder at launch skips the start up Job Setup.
      const opened = restored || useDocumentStore.getState().path !== null;
      if (!cancelled && !opened && usePrefsStore.getState().prefs.window.jobSetupAtStart) {
        openNewJobSetup();
      }
    });
    return () => {
      cancelled = true;
    };
  }, [connect, openNewJobSetup]);

  useEffect(() => {
    const stopLayout = watchLayout();
    const stopAutosave = startAutosave();
    const stopVariables = watchVariables();
    const stopGuides = watchGuides();
    const stopNativeMenu = watchNativeMenu((name) => void runMenuAction({ type: 'gadget', name }));
    const stopTitle = watchWindowTitle();
    const stopModifiers = watchHeldModifiers(window);
    let cancelled = false;
    let stopRecent: Unsubscribe | null = null;
    void getKernel()
      .onOpenRecent?.((index) => void openRecent(index))
      .then((off) => {
        if (cancelled) {
          off();
        } else {
          stopRecent = off;
        }
      });
    return () => {
      cancelled = true;
      stopLayout();
      stopAutosave();
      stopVariables();
      stopGuides();
      stopNativeMenu();
      stopTitle();
      stopModifiers();
      stopRecent?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unsubscribe: Unsubscribe | null = null;
    void getKernel()
      .onMenuCommand((command) => {
        if (acceptsInput()) {
          runCommand(command);
        }
      })
      .then((off) => {
        if (cancelled) {
          off();
        } else {
          unsubscribe = off;
        }
      });
    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, [runCommand]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (isTyping(event) || event.defaultPrevented || modalDialogOpen()) {
        return;
      }
      const nudge = nudgeForKey(event, useViewStore.getState().nudgeDistance);
      if (nudge !== null) {
        const nudgeSelection = () => {
          const doc = useDocumentStore.getState();
          if (doc.selection.ids.length > 0) {
            void doc.moveSelection(nudge.dx, nudge.dy);
          }
        };
        if (picking()) {
          // A click is still picking: the nudge moves what it picks.
          event.preventDefault();
          void afterPicks().then(nudgeSelection);
        } else if (useDocumentStore.getState().selection.ids.length > 0) {
          event.preventDefault();
          nudgeSelection();
        }
        return;
      }
      const command = commandForKey(event);
      if (command !== null) {
        event.preventDefault();
        if (picking()) {
          void afterPicks().then(() => runEnabledCommand(command));
        } else {
          void runEnabledCommand(command);
        }
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [runCommand]);

  return (
    <div className="app-shell">
      {isTauriHost() ? null : <MenuBar />}
      <main className="app-shell__body">
        <DockTabs />
        <LeftPanel />
        <ViewArea />
        <RightPanel />
      </main>
      <StatusBar />
      <GadgetsMenu />
      <PrintPreview />
      <PrintView />
      <LazyPanels />
      <Tooltip />
      <BusyShield />
    </div>
  );
}
