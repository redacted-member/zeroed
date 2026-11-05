import { describe, expect, it } from 'vitest';

import type { TemplateInfo } from '../../bridge/templates';
import {
  draftProblems,
  groupTemplates,
  initialDraft,
  nameProblem,
  saveArgs,
  suggestedName,
  templateDetails,
  templateExists,
  templateSize,
  userCategories,
} from './model';

function template(overrides: Partial<TemplateInfo>): TemplateInfo {
  return {
    name: 'Job',
    category: null,
    path: '/t/Job.carve',
    standard: false,
    units: 'mm',
    width: 600,
    height: 400,
    thickness: 19,
    layers: 1,
    vectors: 0,
    toolpaths: 0,
    ...overrides,
  };
}

describe('template dialog model', () => {
  it('groups standard sheets first, then the top folder, then categories', () => {
    const groups = groupTemplates([
      template({ name: 'Door', category: 'Cabinets' }),
      template({ name: 'Mine' }),
      template({ name: 'Ply', category: 'Standard', standard: true }),
    ]);
    expect(groups.map((g) => g.title)).toEqual(['Standard', 'My Templates', 'Cabinets']);
    expect(groups[2]?.items.map((t) => t.name)).toEqual(['Door']);
  });

  it('keeps a user category called Standard apart from the standard sheets', () => {
    const groups = groupTemplates([
      template({ name: 'Ply', category: 'Standard', standard: true }),
      template({ name: 'Own', category: 'Standard' }),
    ]);
    expect(groups).toHaveLength(2);
  });

  it('shows the size in the template units', () => {
    expect(templateSize(template({}))).toBe('600 x 400 x 19 mm');
    const inch = template({ units: 'inch', width: 1219.2, height: 2438.4, thickness: 19.05 });
    expect(templateSize(inch)).toBe('48 x 96 x 0.75 inch');
    expect(templateDetails(template({ layers: 2, vectors: 1 }))).toBe(
      '2 layers, 1 vector, 0 toolpaths',
    );
  });

  it('refuses names that are not file names', () => {
    expect(nameProblem('Sign blank')).toBeNull();
    expect(nameProblem('  ')).not.toBeNull();
    expect(nameProblem('a/b')).not.toBeNull();
    expect(nameProblem('.hidden')).not.toBeNull();
    expect(nameProblem('x'.repeat(121))).not.toBeNull();
  });

  it('suggests the job file name', () => {
    expect(suggestedName('/jobs/Sign.carve')).toBe('Sign');
    expect(suggestedName(null)).toBe('');
    expect(initialDraft('C:\\jobs\\Box.v2.carve').name).toBe('Box.v2');
  });

  it('builds the save arguments', () => {
    const draft = { name: ' Sign ', category: '', design: true, toolpaths: true };
    expect(saveArgs(draft)).toEqual({
      name: 'Sign',
      design: true,
      toolpaths: true,
      overwrite: false,
    });
    expect(saveArgs({ ...draft, category: 'Signs', design: false }, true)).toEqual({
      name: 'Sign',
      category: 'Signs',
      design: false,
      toolpaths: false,
      overwrite: true,
    });
    expect(saveArgs({ ...draft, name: '' })).toBeNull();
    expect(draftProblems({ ...draft, category: 'a:b' }).category).toBeDefined();
  });

  it('finds an existing template of the same name', () => {
    const list = [
      template({ name: 'Sign', category: 'Signs' }),
      template({ name: 'Ply', standard: true, category: 'Standard' }),
    ];
    expect(templateExists(list, 'sign', 'signs')).toBe(true);
    expect(templateExists(list, 'Sign', '')).toBe(false);
    expect(templateExists(list, 'Ply', 'Standard')).toBe(false);
    expect(userCategories(list)).toEqual(['Signs']);
  });
});
