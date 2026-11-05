// The commands only the menus offer (brief 015): File > Close, Increment
// and Save, Print, Import and Export; the Edit selection and alignment
// items; the Model, Machine and Toolpaths items that open a form; View
// toggles; and the Help dialogs. runAppCommand hands them here.

import { api } from '../../bridge/api';
import { getKernel } from '../../bridge/client';
import type { VectorFormat } from '../../bridge/types';
import { BITMAP_EXTENSIONS, importBitmap } from '../../lib/bitmapCommands';
import { reloadPosts } from '../../lib/camData';
import {
  TEMPLATE_EXTENSION,
  IMPORT_VECTOR_EXTENSIONS,
  importVectorFile,
  loadToolpathTemplate,
  newJob,
  saveJobAs,
  withExtension,
} from '../../lib/fileCommands';
import { importToolpaths } from '../../lib/importToolpaths';
import { createJobSheet } from '../../lib/jobSheet';
import { printPreview, printView } from '../../lib/printView';
import { saveShadedImage } from '../../lib/shadedImage';
import type { AppCommand } from '../../lib/shortcuts';
import type { AlignMode } from '../../lib/transformMath';
import { strings } from '../../lib/strings';
import { useDialogStore, type DialogId } from '../../state/dialogStore';
import { useDocumentStore } from '../../state/documentStore';
import { usePrefsStore } from '../../state/prefsStore';
import { usePreviewStore } from '../../state/previewStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore, type FormId, type WorkspaceTab } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { togglePatch } from './displayState';
import { runModelMenuCommand } from './modelMenuCommands';

/** Menu items that open a form, and the left dock tab the form belongs to. */
export const FORMS: Partial<Record<AppCommand, { form: FormId; tab?: WorkspaceTab }>> = {
  'edit.joinVectors': { form: 'join', tab: 'design' },
  'edit.curveFit': { form: 'fitCurves', tab: 'design' },
  'edit.nest': { form: 'nest', tab: 'design' },
  'edit.documentVariables': { form: 'documentVariables', tab: 'design' },
  'model.twoRailSweep': { form: 'twoRailSweep', tab: 'design' },
  'model.extrudeWeave': { form: 'extrudeWeave', tab: 'design' },
  'model.turnSpin': { form: 'turnSpin', tab: 'design' },
  'model.texturedArea': { form: 'textureArea', tab: 'design' },
  'model.slice': { form: 'sliceModel', tab: 'design' },
  'model.exportStl': { form: 'exportStl', tab: 'design' },
  'model.sculpt': { form: 'sculpt', tab: 'design' },
  'model.replaceBelow': { form: 'replaceBelow', tab: 'design' },
  'model.distort': { form: 'distort', tab: 'design' },
  'model.smooth': { form: 'smoothComponents', tab: 'design' },
  'model.emboss': { form: 'embossComponent', tab: 'design' },
  'model.scaleZ': { form: 'scaleModelHeight', tab: 'design' },
  'model.offsetModel': { form: 'offsetModel', tab: 'design' },
  'model.addDraft': { form: 'addDraft', tab: 'design' },
  'toolpaths.merge': { form: 'mergeToolpaths' },
};

/** Menu items that show a dialog. */
const DIALOGS: Partial<Record<AppCommand, DialogId>> = {
  'edit.notes': 'notes',
  'help.whatsNew': 'whatsNew',
  'help.releaseNotes': 'releaseNotes',
  'help.licences': 'licences',
  'help.about': 'about',
  'toolpaths.materials': 'materials',
  'machine.postManagement': 'postManagement',
};

const EXPORTS: Partial<Record<AppCommand, VectorFormat>> = {
  'file.exportEps': 'eps',
  'file.exportDxf': 'dxf',
  'file.exportAi': 'ai',
  'file.exportSvg': 'svg',
  'file.exportPdf': 'pdf',
};

/**
 * Ops align modes and targets of the Design tab's Align Objects group and of
 * Edit > Align Selected Objects, which lists the same tools: to the material or
 * to the last selected object, as the Alignment Tools form does.
 */
const ALIGN: Partial<Record<AppCommand, [AlignMode, 'material' | 'last']>> = {
  'align.centreInMaterial': ['centre', 'material'],
  'align.inMaterialX': ['hCentre', 'material'],
  'align.inMaterialY': ['vCentre', 'material'],
  'align.lastLeft': ['left', 'last'],
  'align.lastRight': ['right', 'last'],
  'align.lastTop': ['top', 'last'],
  'align.lastBottom': ['bottom', 'last'],
  'align.lastCenter': ['centre', 'last'],
  'align.lastHCenter': ['hCentre', 'last'],
  'align.lastVCenter': ['vCentre', 'last'],
  'align.outsideLeft': ['outsideLeft', 'last'],
  'align.outsideRight': ['outsideRight', 'last'],
  'align.outsideTop': ['outsideTop', 'last'],
  'align.outsideBottom': ['outsideBottom', 'last'],
};

const QUALITY = {
  'preview.standard': 'standard',
  'preview.high': 'high',
  'preview.veryHigh': 'veryHigh',
  'preview.extremelyHigh': 'extremelyHigh',
  'preview.maximum': 'maximum',
} as const;

/**
 * The next name File > Increment and Save uses: a trailing `_N` counts up,
 * otherwise `_1` is added before the extension.
 */
export function incrementedPath(path: string): string {
  const slash = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  const dot = path.lastIndexOf('.');
  const hasExt = dot > slash + 1;
  const stem = hasExt ? path.slice(0, dot) : path;
  const ext = hasExt ? path.slice(dot) : '';
  const match = /^(.*)_(\d+)$/.exec(stem.slice(slash + 1));
  const dir = stem.slice(0, slash + 1);
  const name =
    match === null ? `${stem.slice(slash + 1)}_1` : `${match[1]}_${Number(match[2]) + 1}`;
  return `${dir}${name}${ext}`;
}

/** Whether Import Vectors / Bitmaps reads `path` as a picture or as vectors. */
export function importKindOf(path: string): 'bitmap' | 'vectors' | null {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  if (BITMAP_EXTENSIONS.includes(ext)) {
    return 'bitmap';
  }
  return IMPORT_VECTOR_EXTENSIONS.includes(ext) ? 'vectors' : null;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function incrementAndSave(): Promise<void> {
  const current = useDocumentStore.getState().path;
  if (current === null) {
    await saveJobAs();
    return;
  }
  const next = incrementedPath(current);
  if (await useDocumentStore.getState().saveDocument(next)) {
    await usePrefsStore.getState().addRecent(next);
  }
}

async function importVectorsOrBitmaps(): Promise<void> {
  const text = strings.menus;
  const path = await getKernel().chooseOpenPath(text.importVectorsBitmaps, [
    {
      name: text.importVectorsBitmaps,
      extensions: [...IMPORT_VECTOR_EXTENSIONS, ...BITMAP_EXTENSIONS],
    },
  ]);
  if (path === null) {
    return;
  }
  if (importKindOf(path) === 'bitmap') {
    await importBitmap(path);
    return;
  }
  await importVectorFile(path);
}

async function exportAs(format: VectorFormat): Promise<void> {
  const chosen = await getKernel().chooseSavePath(strings.file.exportTitle, [
    { name: format.toUpperCase(), extensions: [format] },
  ]);
  if (chosen !== null) {
    await useDocumentStore
      .getState()
      .exportVectors(withExtension(chosen, format, [format]), format);
  }
}

async function selectWhere(
  keep: (entity: { kind: string; closed: boolean; layer: number }) => boolean,
): Promise<void> {
  const doc = useDocumentStore.getState();
  const ids = doc.entities.filter((e) => e.kind !== 'bitmap' && keep(e)).map((e) => e.id);
  await doc.select(ids, 'set');
}

async function selectDuplicates(): Promise<void> {
  try {
    await api.drawTools.selectDuplicates(0.001);
    await useDocumentStore.getState().refresh();
  } catch (error) {
    useDocumentStore.setState({ lastError: message(error) });
  }
}

async function saveTemplate(ids: number[] | undefined): Promise<void> {
  const chosen = await getKernel().chooseSavePath(strings.file.saveTemplateTitle, [
    { name: strings.file.templateFiles, extensions: [TEMPLATE_EXTENSION] },
  ]);
  if (chosen === null) {
    return;
  }
  try {
    await api.cam.saveTemplate(
      withExtension(chosen, TEMPLATE_EXTENSION, [TEMPLATE_EXTENSION]),
      ids,
    );
    useDocumentStore.setState({ lastError: null });
  } catch (error) {
    useDocumentStore.setState({ lastError: message(error) });
  }
}

async function installPost(): Promise<void> {
  const path = await getKernel().chooseOpenPath(strings.menus.installPost, [
    { name: strings.menus.installPost, extensions: ['pp'] },
  ]);
  if (path === null) {
    return;
  }
  try {
    await api.posts.import(path);
    await reloadPosts();
    useDocumentStore.setState({ lastError: null });
  } catch (error) {
    useDocumentStore.setState({ lastError: message(error) });
  }
}

/** Runs a menu only command. Answers false for a command it does not know. */
export async function runMenuCommand(command: AppCommand): Promise<boolean> {
  if (await runModelMenuCommand(command)) {
    return true;
  }
  const ui = useUiStore.getState();
  const form = FORMS[command];
  if (form !== undefined) {
    if (form.tab !== undefined) {
      ui.setActiveTab(form.tab);
    }
    ui.openForm(form.form);
    return true;
  }
  const dialog = DIALOGS[command];
  if (dialog !== undefined) {
    useDialogStore.getState().show(dialog);
    return true;
  }
  const format = EXPORTS[command];
  if (format !== undefined) {
    await exportAs(format);
    return true;
  }
  const align = ALIGN[command];
  if (align !== undefined) {
    await useDocumentStore.getState().alignSelection(align[0], align[1]);
    return true;
  }
  if (command in QUALITY) {
    usePreviewStore.getState().setQuality(QUALITY[command as keyof typeof QUALITY]);
    return true;
  }
  const prefs = usePrefsStore.getState();
  const toggle = togglePatch(command, prefs.prefs);
  if (toggle !== null) {
    await prefs.update(toggle);
    return true;
  }
  const view = useViewStore.getState();
  switch (command) {
    case 'file.close':
      await newJob();
      return true;
    case 'file.incrementSave':
      await incrementAndSave();
      return true;
    case 'file.print':
    case 'file.printSetup':
      printView();
      return true;
    case 'file.printPreview':
      printPreview();
      return true;
    case 'file.importVectorsBitmaps':
      await importVectorsOrBitmaps();
      return true;
    case 'file.importToolpaths':
      await importToolpaths();
      return true;
    case 'file.openDataFolder':
      await getKernel().openDataFolder?.();
      return true;
    case 'select.openVectors':
      await selectWhere((e) => !e.closed);
      return true;
    case 'select.duplicates':
      await selectDuplicates();
      return true;
    case 'select.currentLayer': {
      const layer = useDocumentStore.getState().layers.find((l) => l.active)?.id;
      await selectWhere((e) => e.layer === layer);
      return true;
    }
    case 'machine.add':
      useDialogStore.getState().setAddMachine(true);
      useDialogStore.getState().show('machineConfiguration');
      return true;
    case 'machine.installPost':
      await installPost();
      return true;
    case 'toolpaths.showTab':
      ui.toggleToolpathsPinned();
      return true;
    case 'toolpaths.saveTemplateSelected': {
      const selected = useToolpathStore.getState().selectedId;
      if (selected !== null) {
        await saveTemplate([selected]);
      }
      return true;
    }
    case 'toolpaths.saveTemplateVisible':
      await saveTemplate(
        useToolpathStore
          .getState()
          .toolpaths.filter((t) => t.visible)
          .map((t) => t.id),
      );
      return true;
    case 'toolpaths.loadTemplate':
      await loadToolpathTemplate();
      return true;
    case 'toolpaths.jobSheet':
      await createJobSheet();
      return true;
    case 'view.saveShadedImage':
      await saveShadedImage();
      return true;
    case 'view.guidesVisible':
      view.toggleGuides();
      return true;
    case 'view.lockGuides':
    case 'view.unlockGuides':
      for (const guide of view.guides) {
        view.setGuideLocked(guide.id, command === 'view.lockGuides');
      }
      return true;
    case 'view.shadedBackground': {
      const shaded = usePrefsStore.getState().prefs.view3d.background === 'gradient';
      await usePrefsStore
        .getState()
        .update({ view3d: { background: shaded ? 'theme' : 'gradient' } });
      return true;
    }
    case 'view.cascade':
      ui.setViewMode('2d');
      return true;
    case 'view.newFurniture':
      useDialogStore.getState().show('furnitureLibrary');
      return true;
    default:
      return false;
  }
}
