// Which modal dialog of the menus is showing, if any, plus the one shot
// hand-offs those dialogs read when they open.

import { create } from 'zustand';

export type DialogId =
  | 'helpContents'
  | 'shortcuts'
  | 'machineConfiguration'
  | 'notes'
  | 'whatsNew'
  | 'releaseNotes'
  | 'licences'
  | 'about'
  | 'printPreview'
  | 'vectorSelector'
  | 'materials'
  | 'postManagement'
  | 'pdfPage'
  | 'missingLayers'
  | 'machiningTime'
  | 'toolpathWarnings'
  | 'furnitureLibrary';

/** A PDF waiting for its page to be chosen. */
export interface PdfPageRequest {
  path: string;
  pages: number;
  /** File > Open: the page becomes a new job sized to it (see openVectorsAsJob). */
  asJob?: boolean;
}

/** A toolpath template whose layers the job lacks, waiting for a choice. */
export interface MissingLayersRequest {
  path: string;
  layers: string[];
}

interface DialogState {
  open: DialogId | null;
  /** Machine > Add Machine: Machine Configuration opens with a new machine. */
  addMachine: boolean;
  /** The 2D view as a PNG data URL, for printing. */
  printImage: string | null;
  /** The file the PDF page dialog imports from. */
  pdfPage: PdfPageRequest | null;
  /** The template the missing layers dialog is about. */
  missingLayers: MissingLayersRequest | null;
  /** The help topic to show (lib/helpTopics ids), null for the start page. */
  helpTopic: string | null;
  /** Counts showHelp calls, so asking for the same topic again shows it again. */
  helpSerial: number;
  /** Opens the help at `topic` (the start page when none). */
  showHelp: (topic?: string | null) => void;
  askMissingLayers: (request: MissingLayersRequest) => void;
  show: (id: DialogId) => void;
  /** Opens the PDF page dialog for `path`. */
  askPdfPage: (request: PdfPageRequest) => void;
  close: () => void;
  setAddMachine: (addMachine: boolean) => void;
  setPrintImage: (printImage: string | null) => void;
}

export const useDialogStore = create<DialogState>()((set) => ({
  open: null,
  addMachine: false,
  printImage: null,
  pdfPage: null,
  missingLayers: null,
  helpTopic: null,
  helpSerial: 0,
  showHelp: (topic = null) =>
    set((state) => ({ open: 'helpContents', helpTopic: topic, helpSerial: state.helpSerial + 1 })),
  askMissingLayers: (missingLayers) => set({ open: 'missingLayers', missingLayers }),
  show: (open) => set({ open }),
  askPdfPage: (pdfPage) => set({ open: 'pdfPage', pdfPage }),
  close: () => set({ open: null }),
  setAddMachine: (addMachine) => set({ addMachine }),
  setPrintImage: (printImage) => set({ printImage }),
}));
