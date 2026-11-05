// JSON shapes of the clipart.* kernel commands.

import type { Combine } from './model';

/** What a clip inserts as. */
export type ClipKind = 'relief' | 'vector';
/** The kind of file a clip is. */
export type ClipFormat = 'clip' | 'mesh' | 'vectors';

/** One file in the clipart library. */
export interface ClipItem {
  name: string;
  /** Full path, as thumbnail and insert take it. */
  path: string;
  /** Subfolder the file sits in, `/` between levels. */
  category: string;
  kind: ClipKind;
  format: ClipFormat;
  /** Words a search matches besides the name and category. */
  tags?: string[];
}

export interface ClipartScan {
  /** Categories, `/` between levels, sorted. */
  categories: string[];
  items: ClipItem[];
  /** The user's clipart folder. */
  userDir: string;
  roots: string[];
  warnings: string[];
}

export interface ClipartInsertArgs {
  path: string;
  /** Where the clip's centre goes, job mm. */
  x: number;
  y: number;
  /** Size as a factor of the clip's own size. */
  scale: number;
  combine?: Combine;
}

export type ClipartInsertReply =
  | { kind: 'relief'; id: number; revision: number }
  | { kind: 'vector'; ids: number[]; revision: number };

export interface ThumbnailHeader {
  width: number;
  height: number;
  sections: Array<{ name: string; dtype: 'u8'; offset: number; count: number }>;
}
