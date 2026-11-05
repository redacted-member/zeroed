import { describe, expect, it } from 'vitest';

import { FORMS } from '../../forms/registry';
import {
  BEFORE_YOU_CUT_TOPIC,
  START_TOPIC,
  TOOLPATHS_TOPIC,
  formTopic,
  tabTopic,
} from '../../lib/helpTopics';
import { contentProblems } from '../../lib/usSpelling';
import { TOOL_GROUPS } from '../../shell/LeftPanel/toolGroups';
import type { FormId, WorkspaceTab } from '../../state/uiStore';
import { FORM_HELP, HELP_SETS, findTopic, hasTopic, helpSections, type HelpTopic } from './model';

const FORM_IDS = Object.keys(FORMS) as FormId[];
const TABS: readonly WorkspaceTab[] = ['design', 'sheets', 'layers', 'components', 'clipart'];

function allTopics(): HelpTopic[] {
  return helpSections().flatMap((s) => s.groups.flatMap((g) => g.topics));
}

function texts(topic: HelpTopic): string[] {
  return [
    topic.title,
    topic.summary,
    ...topic.fields.flatMap((f) => [f.name, f.text]),
    ...topic.steps,
    ...topic.blocks.flatMap((b) => [b.heading, ...b.items]),
  ];
}

describe('help content index', () => {
  it('has a page for every tool of the Design tab and the Toolpaths panel', () => {
    for (const set of HELP_SETS) {
      for (const tool of TOOL_GROUPS[set].flatMap((g) => g.tools)) {
        const topic = findTopic(`tool:${tool.id}`);
        expect(topic.id, tool.id).toBe(`tool:${tool.id}`);
        expect(topic.summary, tool.id).not.toBe('');
      }
    }
  });

  it('has an entry for every form, with what it does and its settings', () => {
    const missing = FORM_IDS.filter((form) => FORM_HELP[form] === undefined);
    expect(missing).toEqual([]);
    for (const form of FORM_IDS) {
      const entry = FORM_HELP[form];
      expect(entry?.summary, form).not.toBe('');
      expect(entry?.fields?.length ?? 0, form).toBeGreaterThan(0);
      for (const field of entry?.fields ?? []) {
        expect(field.name, form).not.toBe('');
        expect(field.text, `${form} ${field.name}`).not.toBe('');
      }
    }
  });

  it('holds no entries for forms that do not exist', () => {
    expect(Object.keys(FORM_HELP).filter((id) => !FORM_IDS.includes(id as FormId))).toEqual([]);
  });

  it('finds every form, by the tool that opens it or on its own page', () => {
    for (const form of FORM_IDS) {
      const topic = findTopic(formTopic(form));
      expect(topic.id, form).not.toBe(START_TOPIC);
      expect(topic.fields.length, form).toBeGreaterThan(0);
    }
    expect(findTopic(formTopic('profileToolpath')).id).toBe('tool:profile');
    expect(findTopic(formTopic('mergeToolpaths')).id).toBe('form:mergeToolpaths');
  });

  it('has the pages the shell opens: every dock tab, the Toolpaths panel and the start pages', () => {
    for (const tab of TABS) {
      expect(hasTopic(tabTopic(tab)), tab).toBe(true);
    }
    for (const id of [TOOLPATHS_TOPIC, START_TOPIC, BEFORE_YOU_CUT_TOPIC]) {
      expect(hasTopic(id), id).toBe(true);
    }
    expect(hasTopic('panel:view2d') && hasTopic('panel:view3d')).toBe(true);
  });

  it('opens the start page for an unknown or missing topic', () => {
    expect(findTopic(null).id).toBe(START_TOPIC);
    expect(findTopic('tool:noSuchTool').id).toBe(START_TOPIC);
    expect(findTopic(START_TOPIC).guides).toBe(true);
  });

  it('links only to topics that exist', () => {
    const links = allTopics().flatMap((t) => t.related.map((id) => [t.id, id]));
    expect(links.filter(([, id]) => !hasTopic(id as string))).toEqual([]);
  });

  it('turns the machine checklist into the Before you cut page, linked from Save Toolpaths', () => {
    const page = findTopic(BEFORE_YOU_CUT_TOPIC);
    expect(page.blocks.length).toBeGreaterThanOrEqual(6);
    expect(page.blocks.flatMap((b) => b.items).join(' ')).toMatch(/Test Coupon/);
    expect(page.related).toContain('form:saveToolpaths');
  });

  it('lists the joints on the Joint Creator page', () => {
    const joints = findTopic(formTopic('jointCreator')).blocks.at(-1);
    expect(joints?.items.some((item) => item.startsWith('Finger joint:'))).toBe(true);
  });

  it('is written in US spelling, without em dashes or other product names', () => {
    const problems = allTopics().flatMap((t) => texts(t).flatMap(contentProblems));
    expect(problems).toEqual([]);
  });
});

describe('helpSections', () => {
  it('lists the start pages, the window, both tool sets and the other forms', () => {
    const sections = helpSections();
    expect(sections.map((s) => s.id)).toEqual(['start', 'window', 'design', 'toolpaths', 'more']);
    for (const set of HELP_SETS) {
      const section = sections.find((s) => s.id === set);
      expect(section?.groups.map((g) => g.id)).toEqual(TOOL_GROUPS[set].map((g) => g.id));
      expect(section?.groups.every((g) => g.description !== '')).toBe(true);
    }
  });

  it('shows each page once', () => {
    const ids = allTopics().map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('filters by any word of a page, ignoring case', () => {
    const topics = helpSections('POCKET').flatMap((s) =>
      s.groups.flatMap((g) => g.topics.map((t) => t.id)),
    );
    expect(topics).toContain('tool:pocket');
    expect(topics).not.toContain('tool:drawCircle');
    const calipers = helpSections('calipers').flatMap((s) =>
      s.groups.flatMap((g) => g.topics.map((t) => t.id)),
    );
    expect(calipers).toContain(BEFORE_YOU_CUT_TOPIC);
  });

  it('keeps a whole group when its title matches', () => {
    const group = helpSections('edit objects')
      .flatMap((s) => s.groups)
      .find((g) => g.id === 'editObjects');
    expect(group?.topics.length).toBe(
      TOOL_GROUPS.design.find((g) => g.id === 'editObjects')?.tools.length,
    );
  });

  it('answers nothing when nothing matches', () => {
    expect(helpSections('zzzz no such tool')).toEqual([]);
  });
});
