// The kernel's public shapes, mirroring carve-core's serde output, and the
// client interface every transport implements.

import type { MenuCommand } from '../lib/menu';
import type { Material3d } from './prefs';

/**
 * A warning the kernel reports: a stable code the interface words from
 * lib/strings/kernelWarnings.ts, the values it needs (lengths in mm), and
 * the English to fall back on.
 */
export interface KernelWarning {
  code: string;
  params: Readonly<Record<string, string | number>>;
  message: string;
}

export type Units = 'mm' | 'inch';
export type ZZero = 'top' | 'bottom';
export type XyOrigin = 'centre' | 'bottomLeft' | 'bottomRight' | 'topLeft' | 'topRight';
export type JobKind = 'singleSided' | 'doubleSided' | 'rotary';
export type RotaryAxis = 'x' | 'y';
/** Cross section of a rotary blank. */
export type RotaryBlank = 'cylinder' | 'square';
/** Z zero of a rotary job: on the axis or on the blank surface. */
export type RotaryZZero = 'centre' | 'surface';
/** A face of a double sided job. */
export type Side = 'top' | 'bottom';
/** How the material is turned over: horizontal swaps left and right. */
export type FlipAxis = 'horizontal' | 'vertical';

export interface DoubleSidedSetup {
  flip: FlipAxis;
  /** Z zero while the bottom side is up; the top side uses `JobSetup.zZero`. */
  bottomZZero: ZZero;
}

export interface RotarySetup {
  /** Diameter, or the side of a square blank. */
  diameter: number;
  axis: RotaryAxis;
  /** Length along the axis; absent from older kernels. */
  length?: number;
  blank?: RotaryBlank;
  zZero?: RotaryZZero;
}

/** Job setup. Every length is millimetres, whatever `units` says. */
export interface JobSetup {
  units: Units;
  width: number;
  height: number;
  thickness: number;
  zZero: ZZero;
  xyOrigin: XyOrigin;
  offsetX: number;
  offsetY: number;
  kind: JobKind;
  rotary: RotarySetup;
  /** Rapid heights and home position used by toolpaths. */
  machine: MachineSetup;
  /** Flip direction and bottom Z zero; absent from older kernels. */
  doubleSided?: DoubleSidedSetup;
  /** How the material looks in the 3D view; absent from older kernels. */
  appearance?: JobAppearance;
  /** How finely the 3D model is calculated; absent from older kernels. */
  resolution?: ModelResolution;
}

/** Modelling resolution: standard, or three or seven times the model cells. */
export type ModelResolution = 'standard' | 'high' | 'highest';

/** The material look kept with a job. */
export interface JobAppearance {
  material: Material3d;
  /** `#rrggbb`, used when the material is solid. */
  colour: string;
  /** A picture from the material textures laid over the material, by id; 0 or absent for none. */
  texture?: number;
}

/** Heights are millimetres above the material surface. */
export interface MachineSetup {
  /** Clearance (Z1). */
  safeZ: number;
  /** Plunge (Z2). */
  plungeGap: number;
  homeX: number;
  homeY: number;
  homeZ: number;
  /** Model Position in Material: the model top sits this far (mm) below the
   * material top, or with `gapBelow` the model underside this far above the
   * material bottom (a rotary job's axis). */
  modelGap: number;
  /** Gap Below Model is the chosen radio; absent (false) in older files. */
  gapBelow?: boolean;
}

export interface PingReply {
  pong: boolean;
  version: string;
}

export type Unsubscribe = () => void;

export type TypedArray = Float32Array | Uint32Array | Uint16Array | Uint8Array;
export type SectionType = 'f32' | 'u32' | 'u16' | 'u8';

export interface BlobSection {
  name: string;
  dtype: SectionType;
  offset: number;
  count: number;
}

/** Header of a binary reply. Commands add their own fields beside `sections`. */
export interface BlobHeader {
  sections: BlobSection[];
}

/** Header fields of a reply whose height sections may be `u16` steps:
 * code `q` stands for `heightBase + q * heightStep` mm. */
export interface HeightCodeHeader {
  heightBase?: number;
  heightStep?: number;
}

export interface BlobReply<H extends BlobHeader = BlobHeader> {
  header: H;
  sections: Record<string, TypedArray>;
}

export interface ProgressEvent {
  id: number;
  progress: number;
}

/** Whether the kernel is running a command, and how far it has got. */
export interface KernelActivity {
  running: boolean;
  /** 0 to 1; 0 while the command does not report its progress. */
  progress: number;
}

/** A file type offered in an open or save dialog. */
export interface FileFilter {
  name: string;
  /** Extensions without the dot. */
  extensions: string[];
}

/** Vector file formats the kernel reads and writes, named by extension. */
export type VectorFormat = 'svg' | 'dxf' | 'eps' | 'ai' | 'pdf';

/** What a session that crashed left, offered once at the next start. */
export interface SessionRecovery {
  /** The session before this one ended without quitting. */
  crashed: boolean;
  /** The crash report it wrote, if any. */
  report: string | null;
  /** Autosave backups of jobs it had unsaved changes in. */
  backups: string[];
  /** For each backup, the file it becomes when kept for later. */
  kept: string[];
}

/** The labels of a three button question. */
export interface ChoiceButtons {
  yes: string;
  no: string;
  cancel: string;
}

/** What the front end may ask the kernel for. */
export interface KernelClient {
  readonly kind: 'tauri' | 'http' | 'test';
  call<T>(name: string, args?: unknown): Promise<T>;
  callBlob<H extends BlobHeader>(name: string, args?: unknown): Promise<BlobReply<H>>;
  /** Asks the running command to stop; answers whether one was running. */
  cancel(id: number): Promise<boolean>;
  /** Whether a command is running and how far it has got, answered while
   * it runs, where the host can. */
  activity?(): Promise<KernelActivity>;
  onMenuCommand(handler: (command: MenuCommand) => void): Promise<Unsubscribe>;
  onProgress(handler: (event: ProgressEvent) => void): Promise<Unsubscribe>;
  /** Asks the user for a file to read, starting in `defaultPath` when
   * given. Null when they cancel. */
  chooseOpenPath(
    title: string,
    filters: FileFilter[],
    defaultPath?: string,
  ): Promise<string | null>;
  /** Asks the user where to write a file. Null when they cancel. */
  chooseSavePath(
    title: string,
    filters: FileFilter[],
    defaultPath?: string,
  ): Promise<string | null>;
  /** Asks a yes or no question. */
  confirm(message: string): Promise<boolean>;
  /** Sets the native window title, where the host has one. */
  setWindowTitle?(title: string): Promise<void>;
  /** Lists `paths` in the native File > Open Recent menu, where there is one. */
  setRecentFiles?(paths: string[]): Promise<void>;
  /** Rebuilds the native menu bar from the menus the page shows, where there is one. */
  setMenus?(menus: unknown[]): Promise<void>;
  /** Hands the native menu bar the icons its menus name, as PNG files by icon id, where there is one. */
  setMenuIcons?(images: Record<string, number[]>): Promise<void>;
  /** Shows the application data folder, or its posts or furniture library folder, in the Finder, where the host can. */
  openDataFolder?(sub?: 'posts' | 'furniture'): Promise<void>;
  /** Opens a post the user installed or customised in the system text editor, where the host can. */
  editPost?(id: string): Promise<void>;
  /** A path in the temporary folder to write the file `name` to before
   * Output direct to machine hands it to the driver, where the host can. */
  directOutputPath?(name: string): Promise<string>;
  /** Opens toolpath files in a post's direct output driver program. */
  sendToDriver?(paths: string[], driver: string): Promise<void>;
  /** Calls `handler` with the name of a gadget picked in the native menu. */
  onMenuGadget?(handler: (name: string) => void): Promise<Unsubscribe>;
  /** Calls `handler` with the list index of an Open Recent item picked. */
  onOpenRecent?(handler: (index: number) => void): Promise<Unsubscribe>;
  /**
   * Calls `handler` with files the system asks the app to open (a double
   * click in the Finder) or that are dropped on the window, where the host
   * can tell their paths.
   */
  onOpenFiles?(handler: (paths: string[]) => void): Promise<Unsubscribe>;
  /**
   * The window's close button and Quit, where the host lets the page answer
   * them: while the handler is listening the app only goes once the page
   * calls allowClose.
   */
  onCloseRequested?(handler: () => void): Promise<Unsubscribe>;
  /** Closes the window and quits the app, once the page agreed. */
  allowClose?(): Promise<void>;
  /** Asks a question with three buttons; answers which was pressed. */
  choose?(message: string, buttons: ChoiceButtons): Promise<'yes' | 'no' | 'cancel'>;
  /** Marks the window as holding unsaved changes and names its file, where the host can. */
  setDocumentState?(edited: boolean, path: string | null): Promise<void>;
  /** What a crashed session left, once, where the host keeps session records. */
  sessionRecovery?(): Promise<SessionRecovery>;
  /** Records the backup of the job's unsaved changes (null: none) for the next start after a crash. */
  recordBackup?(path: string | null): Promise<void>;
  /** Deletes an autosave backup the user discarded. */
  discardBackup?(path: string): Promise<void>;
  /** Moves a backup kept for later where autosave never writes; answers its new path. */
  keepBackup?(path: string): Promise<string>;
  /** Opens a crash report in the text editor. */
  showCrashReport?(path: string): Promise<void>;
}

/** Raised when the kernel refuses a command. */
export class KernelError extends Error {
  constructor(
    public readonly command: string,
    message: string,
    public readonly code = 'bridge',
  ) {
    super(message);
    this.name = 'KernelError';
  }
}

export const DEFAULT_JOB: JobSetup = {
  units: 'mm',
  width: 300,
  height: 200,
  thickness: 18,
  zZero: 'top',
  xyOrigin: 'bottomLeft',
  offsetX: 0,
  offsetY: 0,
  kind: 'singleSided',
  rotary: { diameter: 100, axis: 'x' },
  machine: { safeZ: 5, plungeGap: 2, homeX: 0, homeY: 0, homeZ: 20, modelGap: 0 },
  doubleSided: { flip: 'horizontal', bottomZZero: 'top' },
};
