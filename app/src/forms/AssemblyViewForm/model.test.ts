import { describe, expect, it } from 'vitest';

import type { PieceCutRow, PieceIssue, PieceRef, PieceRow } from '../../bridge/assembly';
import {
  componentOptions,
  csvField,
  cutListCsv,
  groupCutList,
  cutListName,
  issueText,
  noComponentNote,
  parsePlacement,
  pieceLabel,
  placementText,
  PIECE_SECTIONS,
  shownSections,
} from './model';

const naming = (part: number, parts: number, vectorName: string | null = null) => ({
  sheet: 3,
  part,
  parts,
  sheetName: 'Sides',
  vectorName,
  model: false,
});

describe('pieceLabel', () => {
  it('names a piece by its vector, else by its sheet and part number', () => {
    expect(pieceLabel(naming(2, 3, 'Left side'))).toBe('Left side');
    expect(pieceLabel(naming(2, 3))).toBe('Sides part 2');
    expect(pieceLabel(naming(1, 1))).toBe('Sides');
    expect(pieceLabel(naming(1, 2, '  '))).toBe('Sides part 1');
  });

  it('names a raw model piece after its sheet as its model', () => {
    expect(pieceLabel({ ...naming(0, 1), model: true })).toBe('Sides model');
  });

  it('names a component piece after its component', () => {
    expect(pieceLabel({ ...naming(1, 1), component: 5, componentName: 'Screen' })).toBe('Screen');
    expect(pieceLabel({ ...naming(1, 1), component: 5, componentName: ' ' })).toBe('Component 5');
    expect(pieceLabel({ ...naming(1, 1), component: null })).toBe('Sides');
  });
});

describe('adding components', () => {
  const screen = { id: 5, name: 'Screen', sheet: 1, sheetName: 'Front' };
  const knob = { id: 8, name: '', sheet: 1, sheetName: 'Front' };

  it('lists components by name, with the sheet only when they come from several', () => {
    expect(componentOptions([screen, knob])).toEqual([
      { id: 5, label: 'Screen' },
      { id: 8, label: 'Component 8' },
    ]);
    expect(componentOptions([screen, { ...knob, sheet: 2, sheetName: 'Back' }])).toEqual([
      { id: 5, label: 'Screen (Front)' },
      { id: 8, label: 'Component 8 (Back)' },
    ]);
  });
});

describe('placement fields', () => {
  const piece: Pick<PieceRow, 'position' | 'rotation'> = {
    position: [25.4, 50.8, 0],
    rotation: [90, -0.0001, 0],
  };

  it('show the position in the job unit and the turns in degrees', () => {
    expect(placementText(piece, 'mm')).toEqual({
      position: ['25.4', '50.8', '0'],
      rotation: ['90', '0', '0'],
    });
    expect(placementText(piece, 'inch').position).toEqual(['1', '2', '0']);
  });

  it('read back into millimetres', () => {
    const parsed = parsePlacement(
      { position: ['1', '2', '0.5'], rotation: ['90', '0', '45'] },
      'inch',
    );
    expect(parsed).toEqual({ ok: true, position: [25.4, 50.8, 12.7], rotation: [90, 0, 45] });
  });

  it('name the fields that are not numbers', () => {
    const parsed = parsePlacement({ position: ['1', 'x', '0'], rotation: ['', '0', '0'] }, 'mm');
    expect(parsed).toEqual({ ok: false, bad: [1, 3] });
  });
});

describe('issueText', () => {
  const names = (piece: PieceRef) => (piece.part === 1 ? 'Side' : 'Top');
  const issue = (overlap: number): PieceIssue => ({
    a: { sheet: 1, part: 1 },
    b: { sheet: 1, part: 2 },
    overlap,
    tooClose: true,
    bounds: [
      [0, 0, 0],
      [1, 1, 1],
    ],
  });

  it('gives the overlap in the job unit cubed', () => {
    expect(issueText(issue(1500), names, 0.5, 'mm')).toBe('Side and Top overlap by 1500 mm³');
  });

  it('says when pieces only come too close', () => {
    expect(issueText(issue(0), names, 0.5, 'mm')).toBe('Side and Top are closer than 0.5 mm');
  });
});

describe('cut list CSV', () => {
  it('lists each piece in the job unit, quoting where needed', () => {
    const csv = cutListCsv(
      [
        { ...naming(1, 2, 'Side, left'), length: 600, width: 300, thickness: 18 },
        { ...naming(2, 2), length: null, width: null, thickness: 18 },
      ],
      'mm',
    );
    expect(csv).toBe(
      'Piece,Count,Length,Width,Thickness\r\n"Side, left",1,600,300,18\r\nSides part 2,1,,,18\r\n',
    );
    expect(csvField('say "hi"')).toBe('"say ""hi"""');
  });

  it('counts identical pieces on one line', () => {
    const row = (part: number, name: string, length: number): PieceCutRow => ({
      ...naming(part, 3, name),
      length,
      width: 300,
      thickness: 18,
      area: length * 300,
      exact: true,
    });
    const groups = groupCutList([
      row(1, 'Shelf', 600),
      row(2, 'Shelf', 600.001),
      row(3, 'Top', 800),
    ]);
    expect(groups.map((g) => [g.label, g.count])).toEqual([
      ['Shelf', 2],
      ['Top', 1],
    ]);
    expect(cutListCsv([row(1, 'Shelf', 600), row(2, 'Shelf', 600)], 'mm')).toBe(
      'Piece,Count,Length,Width,Thickness\r\nShelf,2,600,300,18\r\n',
    );
  });

  it('is named after the job', () => {
    expect(cutListName('/jobs/arcade.carve')).toBe('arcade Cut List.csv');
    expect(cutListName(null)).toBe('Cut List.csv');
  });
});

describe('add component note', () => {
  it('says the job has no components rather than that all are pieces', () => {
    expect(noComponentNote(0)).toBe('The job has no components yet.');
    expect(noComponentNote(2)).toBe('Every component is already a piece.');
  });
});

describe('shownSections', () => {
  it('keeps the piece sections out of an empty assembly and shows them all with a piece', () => {
    expect(shownSections(0)).toEqual([]);
    expect(shownSections(1)).toEqual(PIECE_SECTIONS);
    expect(PIECE_SECTIONS).toContain('cutList');
  });
});
