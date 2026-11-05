// JSON shapes of the templates.* kernel commands and job.setScaled.

import type { JobSetup, Units } from './types';

/** One job template. Sizes are millimetres. */
export interface TemplateInfo {
  name: string;
  /** Subfolder, `Standard` for the standard sheets, null at the top. */
  category: string | null;
  path: string;
  standard: boolean;
  units: Units;
  width: number;
  height: number;
  thickness: number;
  layers: number;
  vectors: number;
  toolpaths: number;
}

export interface TemplateListWarning {
  path: string;
  message: string;
}

export interface TemplateList {
  /** The user's templates folder. */
  userDir: string;
  /** The standard sheets first, then the user's by category and name. */
  templates: TemplateInfo[];
  warnings: TemplateListWarning[];
}

export interface TemplateSaveArgs {
  name: string;
  category?: string;
  /** Include the vectors and relief components; default true. */
  design?: boolean;
  /** Include the toolpaths (only with the design); default true. */
  toolpaths?: boolean;
  /** Replace a template of the same name. */
  overwrite?: boolean;
}

export interface JobScaledReply {
  job: JobSetup;
  scaleX: number;
  scaleY: number;
  revision: number;
}
