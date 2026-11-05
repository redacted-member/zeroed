// Pure helpers of the template dialogs: grouping the list, the size text of a
// template, and checking and building what Save as Template sends.

import type { TemplateInfo, TemplateSaveArgs } from '../../bridge/templates';
import { strings } from '../../lib/formStrings';
import { formatLength } from '../../lib/units';

const text = strings.templates;

/** Longest name the kernel accepts. */
export const MAX_NAME = 120;

const BAD_CHARS = /[/\\:*?"<>|\p{Cc}]/u;

export interface TemplateGroup {
  /** Stable key: the category, or an empty string for the top folder. */
  key: string;
  title: string;
  items: TemplateInfo[];
}

/** The list grouped for display: Standard first, then My Templates, then each category. */
export function groupTemplates(templates: readonly TemplateInfo[]): TemplateGroup[] {
  const groups = new Map<string, TemplateGroup>();
  const ordered = [
    ...templates.filter((t) => t.standard),
    ...templates.filter((t) => !t.standard && t.category === null),
    ...templates.filter((t) => !t.standard && t.category !== null),
  ];
  for (const template of ordered) {
    const key = template.standard
      ? `standard:${template.category ?? ''}`
      : (template.category ?? '');
    const title = template.standard
      ? text.standard
      : template.category === null
        ? text.mine
        : template.category;
    const group = groups.get(key);
    if (group === undefined) {
      groups.set(key, { key, title, items: [template] });
    } else {
      group.items.push(template);
    }
  }
  return [...groups.values()];
}

/** Width x height x thickness in the template's own units. */
export function templateSize(template: TemplateInfo): string {
  const unit = template.units === 'inch' ? strings.units.inch : strings.units.mm;
  return text.size(
    formatLength(template.width, template.units),
    formatLength(template.height, template.units),
    formatLength(template.thickness, template.units),
    unit,
  );
}

/** Layers, vectors and toolpaths a template holds. */
export function templateDetails(template: TemplateInfo): string {
  return text.details(template.layers, template.vectors, template.toolpaths);
}

/** Why a template or category name is not usable, or null when it is. */
export function nameProblem(name: string): string | null {
  const trimmed = name.trim();
  if (trimmed === '') {
    return text.errorName;
  }
  if ([...trimmed].length > MAX_NAME) {
    return text.errorNameLength;
  }
  if (trimmed.startsWith('.') || trimmed.endsWith('.') || BAD_CHARS.test(trimmed)) {
    return text.errorNameChars;
  }
  return null;
}

export interface SaveDraft {
  name: string;
  category: string;
  design: boolean;
  toolpaths: boolean;
}

/** What the dialog starts with: the job's file name as the template name. */
export function initialDraft(path: string | null): SaveDraft {
  return { name: suggestedName(path), category: '', design: true, toolpaths: true };
}

/** The file name of `path` without its extension, or empty for an untitled job. */
export function suggestedName(path: string | null): string {
  if (path === null) {
    return '';
  }
  const file = path.split(/[\\/]/).pop() ?? '';
  const dot = file.lastIndexOf('.');
  return dot > 0 ? file.slice(0, dot) : file;
}

export interface DraftProblems {
  name?: string;
  category?: string;
}

export function draftProblems(draft: SaveDraft): DraftProblems {
  const problems: DraftProblems = {};
  const name = nameProblem(draft.name);
  if (name !== null) {
    problems.name = name;
  }
  if (draft.category.trim() !== '') {
    const category = nameProblem(draft.category);
    if (category !== null) {
      problems.category = category;
    }
  }
  return problems;
}

/** The kernel arguments, or null while the draft has problems. */
export function saveArgs(draft: SaveDraft, overwrite = false): TemplateSaveArgs | null {
  const problems = draftProblems(draft);
  if (problems.name !== undefined || problems.category !== undefined) {
    return null;
  }
  const category = draft.category.trim();
  return {
    name: draft.name.trim(),
    ...(category === '' ? {} : { category }),
    design: draft.design,
    toolpaths: draft.design && draft.toolpaths,
    overwrite,
  };
}

/** True when the user already has a template of this name in this category. */
export function templateExists(
  templates: readonly TemplateInfo[],
  name: string,
  category: string,
): boolean {
  const wantName = name.trim().toLowerCase();
  const wantCategory = category.trim().toLowerCase();
  return templates.some(
    (t) =>
      !t.standard &&
      t.name.toLowerCase() === wantName &&
      (t.category ?? '').toLowerCase() === wantCategory,
  );
}

/** The user's own categories, for the category suggestions. */
export function userCategories(templates: readonly TemplateInfo[]): string[] {
  const names = new Set<string>();
  for (const template of templates) {
    if (!template.standard && template.category !== null) {
      names.add(template.category);
    }
  }
  return [...names].sort((a, b) => a.localeCompare(b));
}
