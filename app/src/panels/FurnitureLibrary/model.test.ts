import { describe, expect, it } from 'vitest';

import type { TemplateSummary } from '../../bridge/furniture';
import {
  ALL,
  categoriesOf,
  defaultValues,
  filterTemplates,
  licenseBadge,
  needsCredit,
} from './model';

function template(id: string, over: Partial<TemplateSummary> = {}): TemplateSummary {
  return {
    id,
    name: id,
    category: 'Storage',
    description: '',
    license: 'CC0-1.0',
    author: 'CarveStudio contributors',
    source: null,
    credit: null,
    tags: [],
    builtIn: true,
    params: [],
    ...over,
  };
}

const LIBRARY = [
  template('Wine Rack', { category: 'Kitchen', tags: ['bottles'] }),
  template('Bookcase', { description: 'Open bookcase with fixed shelves', tags: ['flat pack'] }),
  template('Coffee Table', { category: 'Tables' }),
  template('Bench', { category: 'Seating', author: 'Open Desk' }),
  template('Base Cabinet', { description: 'Cabinet with doors' }),
  template('Lamp Stand', { category: 'Lighting' }),
];

describe('categoriesOf', () => {
  it('lists All with the total, then the used categories in library order, unknown last', () => {
    expect(categoriesOf(LIBRARY)).toEqual([
      { id: ALL, count: 6 },
      { id: 'Storage', count: 2 },
      { id: 'Tables', count: 1 },
      { id: 'Seating', count: 1 },
      { id: 'Kitchen', count: 1 },
      { id: 'Lighting', count: 1 },
    ]);
  });

  it('lists only All for an empty library', () => {
    expect(categoriesOf([])).toEqual([{ id: ALL, count: 0 }]);
  });
});

describe('filterTemplates', () => {
  const names = (list: TemplateSummary[]) => list.map((t) => t.name);

  it('keeps one category, sorted by name', () => {
    expect(names(filterTemplates(LIBRARY, 'Storage', ''))).toEqual(['Base Cabinet', 'Bookcase']);
  });

  it('lists every template, sorted, for All and no query', () => {
    expect(names(filterTemplates(LIBRARY, ALL, ''))).toEqual([
      'Base Cabinet',
      'Bench',
      'Bookcase',
      'Coffee Table',
      'Lamp Stand',
      'Wine Rack',
    ]);
  });

  it('matches tags, descriptions and authors, ignoring case', () => {
    expect(names(filterTemplates(LIBRARY, ALL, 'BOTTLES'))).toEqual(['Wine Rack']);
    expect(names(filterTemplates(LIBRARY, ALL, 'doors'))).toEqual(['Base Cabinet']);
    expect(names(filterTemplates(LIBRARY, ALL, 'open desk'))).toEqual(['Bench']);
  });

  it('needs every word to match', () => {
    expect(names(filterTemplates(LIBRARY, ALL, 'flat pack'))).toEqual(['Bookcase']);
    expect(names(filterTemplates(LIBRARY, ALL, 'flat bottles'))).toEqual([]);
  });

  it('applies the category and the query together', () => {
    expect(names(filterTemplates(LIBRARY, 'Tables', 'bookcase'))).toEqual([]);
    expect(names(filterTemplates(LIBRARY, 'Storage', 'shelves'))).toEqual(['Bookcase']);
  });
});

describe('licenseBadge and needsCredit', () => {
  it('reads each license the library takes', () => {
    expect(licenseBadge('CC0-1.0')).toBe('cc0');
    expect(licenseBadge('CC-BY-4.0')).toBe('ccBy');
    expect(licenseBadge('CC-BY-SA-4.0')).toBe('ccBySa');
    expect(licenseBadge('PD')).toBe('pd');
    expect(licenseBadge('CC-BY-NC-4.0')).toBeNull();
  });

  it('asks for the credit line only for BY and BY-SA', () => {
    expect(needsCredit('CC-BY-4.0')).toBe(true);
    expect(needsCredit('CC-BY-SA-4.0')).toBe(true);
    expect(needsCredit('CC0-1.0')).toBe(false);
    expect(needsCredit('PD')).toBe(false);
  });
});

describe('defaultValues', () => {
  it('takes each parameter default, choices as their option', () => {
    expect(
      defaultValues([
        { key: 'width', label: 'Width', kind: 'length', default: 800 },
        {
          key: 'back',
          label: 'Back',
          kind: 'choice',
          default: 'inset',
          options: ['none', 'inset'],
        },
        { key: 'kick', label: 'Toe kick', kind: 'toggle', default: 1 },
      ]),
    ).toEqual({ width: 800, back: 'inset', kick: 1 });
  });
});
