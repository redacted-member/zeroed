// The help: getting started pages, the window's panels, every tool of the
// Design tab and the Toolpaths panel (with its form's settings), and the
// forms no tool opens, found by topic id and filtered by a search. Pure:
// built from the content files (content/*.json), the tool group tables and
// the strings. It loads with the Help dialog, never with the start bundle.

import { FORMS } from '../../forms/registry';
import { START_TOPIC } from '../../lib/helpTopics';
import { dialogsStrings } from '../../lib/strings/dialogs';
import { toolDescriptions } from '../../lib/strings/toolDescriptions';
import { TOOL_GROUPS, type ToolSet } from '../../shell/LeftPanel/toolGroups';
import type { FormId } from '../../state/uiStore';
import forms from './content/forms.json';
import pages from './content/pages.json';
import panels from './content/panels.json';

const text = dialogsStrings.helpDialog;

export const HELP_SETS: readonly ToolSet[] = ['design', 'toolpaths'];

export interface HelpField {
  name: string;
  text: string;
}

export interface HelpBlock {
  heading: string;
  items: string[];
}

/** One page of the help. */
export interface HelpTopic {
  /** `page:`, `panel:`, `tool:` or `form:` and the id. */
  id: string;
  title: string;
  summary: string;
  /** Each setting of the form or control of the panel, in one line. */
  fields: HelpField[];
  /** A short how-to. */
  steps: string[];
  /** Further headed lists (the checklist, the joints). */
  blocks: HelpBlock[];
  /** Topic ids shown under See also. */
  related: string[];
  /** Whether the page offers the getting started walkthroughs. */
  guides: boolean;
}

export interface HelpGroup {
  id: string;
  /** Empty for a section's only, untitled group. */
  title: string;
  description: string;
  topics: HelpTopic[];
}

export interface HelpSection {
  id: string;
  title: string;
  intro: string;
  groups: HelpGroup[];
}

interface FormContent {
  summary: string;
  fields?: HelpField[];
  steps?: string[];
  related?: string[];
}

interface PanelContent extends FormContent {
  title: string;
}

interface PageContent {
  title: string;
  summary: string;
  sections?: HelpBlock[];
  guides?: boolean;
  related?: string[];
}

export const FORM_HELP: Readonly<Record<string, FormContent>> = forms;
export const PANEL_HELP: Readonly<Record<string, PanelContent>> = panels;
export const PAGE_HELP: Readonly<Record<string, PageContent>> = pages;

function topic(id: string, title: string, content: Partial<PageContent & FormContent>): HelpTopic {
  return {
    id,
    title,
    summary: content.summary ?? '',
    fields: content.fields ?? [],
    steps: content.steps ?? [],
    blocks: content.sections ?? [],
    related: content.related ?? [],
    guides: content.guides ?? false,
  };
}

/** The topic of a tool: its one line, or its form's page when it opens one. */
function toolTopic(tool: { id: string; label: string; form?: FormId }): HelpTopic {
  const own = { summary: text.tools[tool.id] ?? toolDescriptions[tool.id] ?? '' };
  const form = tool.form === undefined ? undefined : FORM_HELP[tool.form];
  const page = topic(`tool:${tool.id}`, tool.label, form === undefined ? own : form);
  if (tool.id === 'jointCreator') {
    page.blocks = [
      ...page.blocks,
      {
        heading: text.joints,
        items: text.jointNotes.map((note) => `${note.label}: ${note.description}`),
      },
    ];
  }
  return page;
}

function build(): HelpSection[] {
  const start: HelpSection = {
    id: 'start',
    title: text.sections.start,
    intro: text.sectionIntro.start,
    groups: [
      {
        id: 'pages',
        title: '',
        description: '',
        topics: Object.entries(PAGE_HELP).map(([id, page]) =>
          topic(`page:${id}`, page.title, page),
        ),
      },
    ],
  };
  const window: HelpSection = {
    id: 'window',
    title: text.sections.window,
    intro: text.sectionIntro.window,
    groups: [
      {
        id: 'panels',
        title: '',
        description: '',
        topics: Object.entries(PANEL_HELP).map(([id, panel]) =>
          topic(`panel:${id}`, panel.title, panel),
        ),
      },
    ],
  };
  const toolSections = HELP_SETS.map((set): HelpSection => ({
    id: set,
    title: text.sets[set],
    intro: text.setIntro[set],
    groups: TOOL_GROUPS[set].map((group) => ({
      id: group.id,
      title: group.title,
      description: text.groups[group.id] ?? '',
      topics: group.tools.map(toolTopic),
    })),
  }));
  const toolForms = new Set(
    HELP_SETS.flatMap((set) => TOOL_GROUPS[set].flatMap((g) => g.tools.map((t) => t.form))),
  );
  const more: HelpSection = {
    id: 'more',
    title: text.moreForms,
    intro: text.moreFormsIntro,
    groups: [
      {
        id: 'forms',
        title: '',
        description: '',
        topics: (Object.keys(FORMS) as FormId[])
          .filter((form) => !toolForms.has(form))
          .map((form) =>
            topic(`form:${form}`, FORMS[form].title, FORM_HELP[form] ?? { summary: '' }),
          ),
      },
    ],
  };
  return [start, window, ...toolSections, more];
}

let built: { sections: HelpSection[]; byId: Map<string, HelpTopic> } | null = null;

function index(): { sections: HelpSection[]; byId: Map<string, HelpTopic> } {
  if (built === null) {
    const sections = build();
    const byId = new Map<string, HelpTopic>();
    for (const t of sections.flatMap((s) => s.groups.flatMap((g) => g.topics))) {
      byId.set(t.id, t);
    }
    // A form a tool opens is that tool's page.
    for (const set of HELP_SETS) {
      for (const tool of TOOL_GROUPS[set].flatMap((g) => g.tools)) {
        const page = byId.get(`tool:${tool.id}`);
        if (tool.form !== undefined && page !== undefined && !byId.has(`form:${tool.form}`)) {
          byId.set(`form:${tool.form}`, page);
        }
      }
    }
    built = { sections, byId };
  }
  return built;
}

/** The topic with `id` (a form's id finds the page of the tool that opens it), else the start page. */
export function findTopic(id: string | null): HelpTopic {
  const { byId } = index();
  return byId.get(id ?? START_TOPIC) ?? (byId.get(START_TOPIC) as HelpTopic);
}

/** Whether `id` names a topic of the help. */
export function hasTopic(id: string): boolean {
  return index().byId.has(id);
}

function topicTexts(t: HelpTopic): string[] {
  return [
    t.title,
    t.summary,
    ...t.fields.flatMap((f) => [f.name, f.text]),
    ...t.steps,
    ...t.blocks.flatMap((b) => [b.heading, ...b.items]),
  ];
}

function matches(query: string, texts: readonly string[]): boolean {
  return texts.some((t) => t.toLowerCase().includes(query));
}

/**
 * The help sections, keeping only the topics whose text holds `query` (a
 * group whose title matches keeps all its topics). Empty groups and
 * sections are dropped.
 */
export function helpSections(query = ''): HelpSection[] {
  const q = query.trim().toLowerCase();
  const { sections } = index();
  if (q === '') {
    return sections;
  }
  return sections
    .map((section) => ({
      ...section,
      groups: section.groups
        .map((group) => ({
          ...group,
          topics:
            group.title !== '' && matches(q, [group.title, group.description])
              ? group.topics
              : group.topics.filter((t) => matches(q, topicTexts(t))),
        }))
        .filter((group) => group.topics.length > 0),
    }))
    .filter((section) => section.groups.length > 0);
}
