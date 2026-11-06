// File commands: new, open, save, save as, import and export vectors, and
// toolpath templates.
// Paths come from the host's dialogs; the kernel reads and writes the files.

import { api } from '../bridge/api';
import { getKernel } from '../bridge/client';
import { BITMAP_EXTENSIONS, importBitmap } from './bitmapCommands';
import type { Bbox } from '../bridge/document';
import type { FileFilter, JobSetup, VectorFormat } from '../bridge/types';
import { useDocumentStore } from '../state/documentStore';
import { showNotesIfFlagged } from '../panels/NotesDialog/notes';
import { mayHavePages } from '../panels/PdfPageDialog/model';
import { useDialogStore } from '../state/dialogStore';
import { usePendingFileStore } from '../state/pendingFileStore';
import { usePrefsStore } from '../state/prefsStore';
import { useTemplateStore } from '../state/templateStore';
import { useToolpathStore } from '../state/toolpathStore';
import { useUiStore } from '../state/uiStore';
import { useViewStore } from '../state/viewStore';
import { jobBounds } from '../views/View2D/camera';
import { strings } from './strings';

export const JOB_EXTENSION = 'carve';

const JOB_FILTERS: FileFilter[] = [{ name: strings.file.jobFiles, extensions: [JOB_EXTENSION] }];
/** Vector formats in the order the export dialog offers them. */
export const VECTOR_EXTENSIONS: readonly VectorFormat[] = ['svg', 'dxf', 'eps', 'ai', 'pdf'];
/** Vector files the kernel reads: every export format, and DWG. */
export const IMPORT_VECTOR_EXTENSIONS: readonly string[] = [...VECTOR_EXTENSIONS, 'dwg'];
const IMPORT_FILTERS: FileFilter[] = [
  { name: strings.file.vectorFiles, extensions: [...IMPORT_VECTOR_EXTENSIONS] },
];
const EXPORT_FILTERS: FileFilter[] = [
  { name: strings.file.svgFiles, extensions: ['svg'] },
  { name: strings.file.dxfFiles, extensions: ['dxf'] },
  { name: strings.file.epsFiles, extensions: ['eps'] },
  { name: strings.file.aiFiles, extensions: ['ai'] },
  { name: strings.file.pdfFiles, extensions: ['pdf'] },
];

/** The path with `extension` added unless it already ends in one of `accepted`. */
export function withExtension(
  path: string,
  extension: string,
  accepted: readonly string[] = [extension],
): string {
  const lower = path.toLowerCase();
  return accepted.some((ext) => lower.endsWith(`.${ext}`)) ? path : `${path}.${extension}`;
}

/** The export format a path asks for by its extension, SVG when it has none we write. */
export function exportFormat(path: string): VectorFormat {
  const lower = path.toLowerCase();
  return VECTOR_EXTENSIONS.find((ext) => lower.endsWith(`.${ext}`)) ?? 'svg';
}

/** The file name part of a path, for dialogs and the window title. */
export function fileName(path: string | null): string {
  if (path === null) {
    return strings.file.untitled;
  }
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

/** True when there is nothing unsaved, or the user agrees to lose it. */
export async function mayDiscard(): Promise<boolean> {
  if (!useDocumentStore.getState().dirty) {
    return true;
  }
  return getKernel().confirm(strings.file.discardChanges);
}

export async function newJob(): Promise<boolean> {
  if (!(await mayDiscard())) {
    return false;
  }
  await useDocumentStore.getState().newDocument();
  // A form left open would still show (and act on) the old job.
  const ui = useUiStore.getState();
  if (ui.activeForm !== null) {
    ui.closeForm();
  }
  return true;
}

/** File > Open: any file Zeroed reads; see openFile for what each kind does. */
export async function openJob(): Promise<void> {
  const path = await getKernel().chooseOpenPath(strings.file.openTitle, openFilters());
  if (path !== null) {
    await openFile(path);
  }
}

async function openPath(path: string): Promise<void> {
  const doc = useDocumentStore.getState();
  if (await doc.openDocument(path)) {
    // An opened job has its setup already, and a form left open would still
    // show (and act on) the old job: close whatever form is showing.
    const ui = useUiStore.getState();
    if (ui.activeForm !== null) {
      ui.closeForm();
    }
    useViewStore.getState().zoomToJob(useDocumentStore.getState().job);
    await usePrefsStore.getState().addRecent(path);
    // Notes that start with a full stop show whenever the job opens.
    void showNotesIfFlagged();
  }
}

/** Opens entry `index` of the recent files list. */
export async function openRecent(index: number): Promise<void> {
  const path = usePrefsStore.getState().prefs.general.recentFiles[index];
  if (path === undefined || !(await mayDiscard())) {
    return;
  }
  await openPath(path);
}

export async function saveJobAs(): Promise<void> {
  const current = useDocumentStore.getState().path;
  const chosen = await getKernel().chooseSavePath(
    strings.file.saveTitle,
    JOB_FILTERS,
    current ?? `${strings.file.untitled}.${JOB_EXTENSION}`,
  );
  if (chosen !== null) {
    const path = withExtension(chosen, JOB_EXTENSION);
    if (await useDocumentStore.getState().saveDocument(path)) {
      await usePrefsStore.getState().addRecent(path);
    }
  }
}

export async function saveJob(): Promise<void> {
  const current = useDocumentStore.getState().path;
  if (current === null) {
    await saveJobAs();
  } else {
    await useDocumentStore.getState().saveDocument(current);
  }
}

export const TEMPLATE_EXTENSION = 'cstemplate';
const TEMPLATE_FILTERS: FileFilter[] = [
  { name: strings.file.templateFiles, extensions: [TEMPLATE_EXTENSION] },
];

export async function loadToolpathTemplate(): Promise<void> {
  const path = await getKernel().chooseOpenPath(strings.file.loadTemplateTitle, TEMPLATE_FILTERS);
  if (path !== null) {
    await useToolpathStore.getState().loadTemplate(path);
  }
}

export async function importVectors(): Promise<void> {
  const path = await getKernel().chooseOpenPath(strings.file.importTitle, IMPORT_FILTERS);
  if (path !== null) {
    await importVectorFile(path);
  }
}

/**
 * The job a vector file opened with File > Open starts as: the vectors keep
 * the size and place they were drawn at, so the material is the smallest
 * rectangle holding them and the file's origin, with the datum at that
 * origin (bottom left, offset when the drawing reaches below or left of it).
 * Everything else comes from `job`, the new job's defaults.
 */
export function jobForVectors(job: JobSetup, bbox: Bbox): JobSetup {
  const [minX, minY, maxX, maxY] = bbox;
  const left = Math.min(minX, 0);
  const bottom = Math.min(minY, 0);
  // A drawing with no width or height (one straight line) keeps the default.
  const size = (low: number, high: number, fallback: number) =>
    high - low > 1e-6 ? high - low : fallback;
  return {
    ...job,
    width: size(left, Math.max(maxX, 0), job.width),
    height: size(bottom, Math.max(maxY, 0), job.height),
    xyOrigin: 'bottomLeft',
    offsetX: left === 0 ? 0 : -left,
    offsetY: bottom === 0 ? 0 : -bottom,
    kind: 'singleSided',
  };
}

/**
 * File > Open of a vector file (DXF, SVG, EPS, AI, PDF, DWG), as the
 * reference: the file starts a new job at the size and place it was drawn,
 * and Job Setup opens to check the material. A PDF with more than one page
 * first asks which page.
 */
export async function openVectorsAsJob(path: string): Promise<boolean> {
  if (!(await mayDiscard())) {
    return false;
  }
  await useDocumentStore.getState().newDocument();
  await importVectorFile(path, true);
  return useDocumentStore.getState().lastError === null;
}

/**
 * Imports a vector file and zooms to what came in. A PDF with more than one
 * page first asks which page to read (the import then runs from the dialog).
 * With `asJob` the file sizes the job (openVectorsAsJob).
 */
export async function importVectorFile(path: string, asJob = false): Promise<void> {
  if (mayHavePages(path)) {
    let pages = 1;
    try {
      pages = (await api.io.vectorPages(path)).pages;
    } catch (error) {
      useDocumentStore.setState({
        lastError: error instanceof Error ? error.message : String(error),
      });
      return;
    }
    if (pages > 1) {
      useDialogStore.getState().askPdfPage({ path, pages, asJob });
      return;
    }
  }
  await importVectorPage(path, undefined, asJob);
}

/**
 * Imports `page` (from 1) of a vector file, or the whole file, and zooms to
 * it. With `asJob` the job takes the vectors' size and Job Setup opens.
 */
export async function importVectorPage(path: string, page?: number, asJob = false): Promise<void> {
  const doc = useDocumentStore.getState();
  await doc.importVectors(path, page);
  const { lastError, selection } = useDocumentStore.getState();
  if (lastError !== null) {
    return;
  }
  if (asJob && selection.bbox !== null) {
    await doc.applyJob(jobForVectors(useDocumentStore.getState().job, selection.bbox));
    useViewStore.getState().zoomToJob(useDocumentStore.getState().job);
    const ui = useUiStore.getState();
    ui.closeForm();
    ui.setActiveTab('design');
    ui.openForm('jobSetup');
    return;
  }
  useViewStore.getState().zoomToBounds(selection.bbox);
}

export async function exportVectors(): Promise<void> {
  const chosen = await getKernel().chooseSavePath(strings.file.exportTitle, EXPORT_FILTERS);
  if (chosen === null) {
    return;
  }
  const path = withExtension(chosen, 'svg', VECTOR_EXTENSIONS);
  await useDocumentStore.getState().exportVectors(path, exportFormat(path));
}

/** What opening a file does with it. */
export type OpenKind = 'job' | 'vectors' | 'mesh' | 'bitmap' | 'clip' | 'toolpathTemplate';

const OPEN_EXTENSIONS: Record<OpenKind, readonly string[]> = {
  job: [JOB_EXTENSION],
  vectors: IMPORT_VECTOR_EXTENSIONS,
  mesh: ['stl', 'obj', '3ds', '3dm'],
  bitmap: BITMAP_EXTENSIONS,
  clip: ['carveclip'],
  toolpathTemplate: [TEMPLATE_EXTENSION],
};

const OPEN_ORDER: readonly OpenKind[] = [
  'job',
  'vectors',
  'bitmap',
  'mesh',
  'clip',
  'toolpathTemplate',
];

/** The kind of file a path names by its extension, or null for one we do not read. */
export function openKind(path: string): OpenKind | null {
  const dot = path.lastIndexOf('.');
  if (dot < 0 || dot < Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'))) {
    return null;
  }
  const ext = path.slice(dot + 1).toLowerCase();
  return OPEN_ORDER.find((kind) => OPEN_EXTENSIONS[kind].includes(ext)) ?? null;
}

/** File > Open filters: every supported file first, then one filter per kind. */
export function openFilters(): FileFilter[] {
  const names: Record<OpenKind, string> = {
    job: strings.file.jobFiles,
    vectors: strings.file.vectorFiles,
    mesh: strings.file.meshFiles,
    bitmap: strings.file.bitmapFiles,
    clip: strings.file.clipFiles,
    toolpathTemplate: strings.file.templateFiles,
  };
  return [
    {
      name: strings.file.allSupported,
      extensions: OPEN_ORDER.flatMap((kind) => [...OPEN_EXTENSIONS[kind]]),
    },
    ...OPEN_ORDER.map((kind) => ({ name: names[kind], extensions: [...OPEN_EXTENSIONS[kind]] })),
  ];
}

/**
 * Opens any supported file the way its kind asks: a job replaces the open
 * document, a vector file starts a new job sized to it, a bitmap is placed and opens Trace Bitmap,
 * a 3D model opens its import form with the file chosen, a clip is inserted
 * at the job centre and a toolpath template is loaded. False when nothing
 * was opened.
 */
export async function openFile(path: string): Promise<boolean> {
  const kind = openKind(path);
  const ui = useUiStore.getState();
  switch (kind) {
    case 'job':
      if (!(await mayDiscard())) {
        return false;
      }
      await openPath(path);
      return useDocumentStore.getState().lastError === null;
    case 'vectors':
      return openVectorsAsJob(path);
    case 'bitmap': {
      // The image is placed on the job, selected, ready to trace.
      const id = await importBitmap(path);
      if (id === null) {
        return false;
      }
      await useDocumentStore.getState().select([id], 'set');
      ui.closeForm();
      ui.setActiveTab('design');
      ui.openForm('traceBitmap');
      return true;
    }
    case 'mesh':
      usePendingFileStore.getState().offer('mesh', path);
      ui.closeForm();
      ui.setActiveTab('design');
      ui.openForm('importModel');
      return true;
    case 'clip': {
      const bounds = jobBounds(useDocumentStore.getState().job);
      await api.clipart.insert({
        path,
        x: (bounds.minX + bounds.maxX) / 2,
        y: (bounds.minY + bounds.maxY) / 2,
        scale: 1,
      });
      await useDocumentStore.getState().refresh();
      return true;
    }
    case 'toolpathTemplate':
      ui.showToolpaths();
      await useToolpathStore.getState().loadTemplate(path);
      return true;
    case null:
      await getKernel().confirm(strings.file.unsupportedFile(fileName(path)));
      return false;
  }
}

/**
 * Opens files handed over by the system (double click in the Finder) or
 * dropped on the window: the first job if there is one, otherwise each file
 * in turn, the vector files after the first one imported into its job.
 */
export async function openFiles(paths: readonly string[]): Promise<void> {
  const job = paths.find((path) => openKind(path) === 'job');
  if (job !== undefined) {
    await openFile(job);
    return;
  }
  // The first vector file starts the job; the others are imported into it.
  let vectorJob = false;
  for (const path of paths) {
    if (vectorJob && openKind(path) === 'vectors') {
      await importVectorFile(path);
    } else if ((await openFile(path)) && openKind(path) === 'vectors') {
      vectorJob = true;
    }
  }
}

/** Listens for files the host asks the app to open. Returns the unsubscribe. */
export function startFileOpening(): () => void {
  let stop: (() => void) | null = null;
  let cancelled = false;
  void getKernel()
    .onOpenFiles?.((paths) => void openFiles(paths))
    .then((off) => {
      if (cancelled) {
        off();
      } else {
        stop = off;
      }
    });
  return () => {
    cancelled = true;
    stop?.();
  };
}

let closing = false;

/**
 * The window's close button and Quit: the app goes once unsaved changes
 * may be lost. A second request while the question shows is ignored.
 */
export async function closeRequested(): Promise<void> {
  if (closing) {
    return;
  }
  closing = true;
  try {
    // A question that cannot be asked must not keep the app open.
    const may = await mayDiscard().catch(() => true);
    if (may) {
      await getKernel().allowClose?.();
    }
  } finally {
    closing = false;
  }
}

/** Answers the host's close requests. Returns the unsubscribe. */
export function startCloseGuard(): () => void {
  let stop: (() => void) | null = null;
  let cancelled = false;
  void getKernel()
    .onCloseRequested?.(() => void closeRequested())
    .then((off) => {
      if (cancelled) {
        off();
      } else {
        stop = off;
      }
    });
  return () => {
    cancelled = true;
    stop?.();
  };
}

/** File > New from Template: asks to discard unsaved changes, then lists the templates. */
export async function newFromTemplate(): Promise<void> {
  if (await mayDiscard()) {
    useTemplateStore.getState().openNew();
  }
}

/** File > Save as Template. */
export function saveAsTemplate(): void {
  useTemplateStore.getState().openSave();
}
