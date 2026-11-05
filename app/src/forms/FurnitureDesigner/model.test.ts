import { describe, expect, it } from 'vitest';

import type {
  AttachmentRow,
  ConnectionRow,
  CutListRow,
  FurnitureParam,
  StockThickness,
} from '../../bridge/furniture';
import {
  areaText,
  carvingCount,
  classRows,
  cutListCsv,
  estimateCsv,
  fieldsOf,
  followValues,
  hardwareCsv,
  initialJoints,
  jointsFromConnections,
  jointSizeFields,
  paramGroups,
  parseParam,
  sameValues,
  sizesOf,
  sizeTexts,
  sliderSpec,
  sliderText,
  sliderValue,
  stockChoices,
  templateId,
  timeText,
  valuesOf,
  withClassJoint,
  withClassParams,
  withConnectionJoint,
  withStyle,
} from './model';
import { formatLength } from '../../lib/units';

const WIDTH: FurnitureParam = {
  key: 'width',
  label: 'Width',
  kind: 'length',
  default: 800,
  min: 250,
  max: 2400,
  group: 'Size',
};
const T: FurnitureParam = {
  key: 't',
  label: 'Panel thickness',
  kind: 'thickness',
  default: 18,
  group: 'Material',
};
const SHELVES: FurnitureParam = {
  key: 'shelves',
  label: 'Shelves',
  kind: 'count',
  default: 4,
  min: 0,
  max: 12,
  group: 'Layout',
};
const BACK: FurnitureParam = {
  key: 'back',
  label: 'Back',
  kind: 'choice',
  default: 'inset',
  options: ['none', 'inset', 'overlay'],
  group: 'Layout',
};
const KICK: FurnitureParam = {
  key: 'kick',
  label: 'Toe kick',
  kind: 'toggle',
  default: 1,
  group: 'Layout',
};
const ANGLE: FurnitureParam = { key: 'splay', label: 'Splay', kind: 'angle', default: 8 };
const HEIGHT: FurnitureParam = { ...WIDTH, key: 'height', label: 'Height', default: 1800 };
const PARAMS = [WIDTH, HEIGHT, T, SHELVES, BACK, KICK, ANGLE];

/** The millimetres a parse gave, NaN when it failed. */
function mm(result: ReturnType<typeof parseParam>): number {
  return result.ok && typeof result.value === 'number' ? result.value : Number.NaN;
}

describe('parseParam', () => {
  it('reads a length in millimetres in a mm job, calculations too', () => {
    expect(parseParam(WIDTH, '900', 'mm')).toEqual({ ok: true, value: 900 });
    expect(parseParam(WIDTH, '600+2*150', 'mm')).toEqual({ ok: true, value: 900 });
  });

  it('reads a length typed in inches as millimetres', () => {
    expect(mm(parseParam(WIDTH, '36', 'inch'))).toBeCloseTo(914.4, 9);
    expect(mm(parseParam(T, '0.75', 'inch'))).toBeCloseTo(19.05, 9);
  });

  it('takes the bounds themselves and refuses a hundredth past them', () => {
    expect(parseParam(WIDTH, '250', 'mm')).toEqual({ ok: true, value: 250 });
    expect(parseParam(WIDTH, '2400', 'mm')).toEqual({ ok: true, value: 2400 });
    expect(parseParam(WIDTH, '249.99', 'mm')).toEqual({ ok: false, error: 'Between 250 and 2400' });
    expect(parseParam(WIDTH, '2400.01', 'mm')).toEqual({
      ok: false,
      error: 'Between 250 and 2400',
    });
  });

  it('says the bounds in inches in an inch job and checks them in millimetres', () => {
    // 250 mm is 9.8425 in; 9.84 in is 249.936 mm, under the minimum.
    expect(parseParam(WIDTH, '9.84', 'inch')).toEqual({
      ok: false,
      error: 'Between 9.8425 and 94.4882',
    });
    expect(mm(parseParam(WIDTH, '9.85', 'inch'))).toBeCloseTo(250.19, 9);
  });

  it('wants a whole count', () => {
    expect(parseParam(SHELVES, '5', 'mm')).toEqual({ ok: true, value: 5 });
    expect(parseParam(SHELVES, '4.5', 'mm')).toEqual({ ok: false, error: 'A whole number' });
    expect(parseParam(SHELVES, '13', 'mm')).toEqual({ ok: false, error: 'Between 0 and 12' });
  });

  it('keeps counts and angles unscaled in an inch job', () => {
    expect(parseParam(SHELVES, '6', 'inch')).toEqual({ ok: true, value: 6 });
    expect(parseParam(ANGLE, '10', 'inch')).toEqual({ ok: true, value: 10 });
  });

  it('takes only an offered option for a choice and 0 or 1 for a toggle', () => {
    expect(parseParam(BACK, 'overlay', 'mm')).toEqual({ ok: true, value: 'overlay' });
    expect(parseParam(BACK, 'glass', 'mm').ok).toBe(false);
    expect(parseParam(KICK, '0', 'mm')).toEqual({ ok: true, value: 0 });
    expect(parseParam(KICK, '2', 'mm').ok).toBe(false);
  });

  it('refuses text that is not a number', () => {
    expect(parseParam(WIDTH, 'wide', 'mm')).toEqual({
      ok: false,
      error: 'Type a number or a calculation',
    });
    expect(parseParam(WIDTH, '', 'mm').ok).toBe(false);
  });
});

describe('fieldsOf and valuesOf', () => {
  it('shows lengths in inches and keeps the rest as typed', () => {
    expect(
      fieldsOf([WIDTH, T, SHELVES, BACK], { width: 914.4, t: 19.05, back: 'none' }, 'inch'),
    ).toEqual({
      width: '36',
      t: '0.75',
      shelves: '4',
      back: 'none',
    });
  });

  it('gives the values in mm and keeps the last good value of a bad field', () => {
    const result = valuesOf(
      [WIDTH, SHELVES, BACK],
      { width: '30', shelves: 'x', back: 'none' },
      { width: 800, shelves: 6, back: 'inset' },
      'inch',
    );
    expect(result.values).toEqual({ width: 762, shelves: 6, back: 'none' });
    expect(result.errors).toEqual({ shelves: 'Type a number or a calculation' });
  });
});

describe('paramGroups', () => {
  it('groups in first appearance order and keeps ungrouped ones together', () => {
    expect(paramGroups(PARAMS).map((g) => [g.group, g.params.map((p) => p.key)])).toEqual([
      ['Size', ['width', 'height']],
      ['Material', ['t']],
      ['Layout', ['shelves', 'back', 'kick']],
      ['', ['splay']],
    ]);
  });
});

describe('sliders', () => {
  it('spans a length in the job unit', () => {
    expect(sliderSpec(WIDTH, 'mm')).toEqual({ min: 250, max: 2400, step: 1 });
    const inch = sliderSpec(WIDTH, 'inch');
    expect(inch?.min).toBeCloseTo(250 / 25.4, 9);
    expect(inch?.max).toBeCloseTo(2400 / 25.4, 9);
    expect(inch?.step).toBe(0.0625);
  });

  it('steps a count by one and has none without both bounds or for other kinds', () => {
    expect(sliderSpec(SHELVES, 'inch')).toEqual({ min: 0, max: 12, step: 1 });
    expect(sliderSpec(T, 'mm')).toBeNull();
    expect(sliderSpec({ ...WIDTH, max: undefined }, 'mm')).toBeNull();
    expect(sliderSpec(ANGLE, 'mm')).toBeNull();
  });

  it('stands where the text says, or at the default when it does not read', () => {
    expect(sliderValue(WIDTH, '1000', 'mm')).toBe(1000);
    expect(sliderValue(WIDTH, '36', 'inch')).toBeCloseTo(36, 9);
    expect(sliderValue(WIDTH, 'oops', 'inch')).toBeCloseTo(800 / 25.4, 9);
  });

  it('writes a slider position as field text', () => {
    expect(sliderText(WIDTH, 36.5, 'inch')).toBe('36.5');
    expect(sliderText(WIDTH, 1234, 'mm')).toBe('1234');
    expect(sliderText(SHELVES, 7, 'inch')).toBe('7');
  });
});

function connection(
  key: string,
  cls: string,
  joint: ConnectionRow['joint'],
  choices: ConnectionRow['choices'],
): ConnectionRow {
  const [a, b] = key.split('~');
  return {
    key,
    a: a ?? '',
    b: b ?? '',
    type: 'tee',
    class: cls,
    joint,
    params: {},
    source: 'style',
    choices,
    seam: { from: [0, 0, 0], to: [0, 1, 0] },
  };
}

const CONNECTIONS: ConnectionRow[] = [
  connection('left~shelf#0', 'shelf', 'dado', ['none', 'dado', 'tabSlot', 'dowel']),
  connection('left~shelf#1', 'shelf', 'dado', ['none', 'dado', 'tabSlot', 'dowel', 'camLock']),
  connection('right~shelf#0', 'shelf', 'dado', ['none', 'dado', 'tabSlot', 'dowel']),
  connection('left~top', 'carcass', 'rabbet', ['none', 'rabbet', 'finger']),
  connection('back~left', 'back', 'rabbet', ['none', 'rabbet', 'groove']),
  connection('kick~left', 'other', 'none', ['none', 'dowel']),
];
const CLASSES = {
  shelf: { label: 'Shelves to sides', default: 'dado' as const },
  carcass: { label: 'Top and bottom', default: 'rabbet' as const },
  back: { label: 'Back', default: 'rabbet' as const },
  unused: { label: 'Doors', default: 'none' as const },
};

describe('classRows', () => {
  const other = (cls: string) => `Other (${cls})`;

  it('lists the template classes in order, skips empty ones, and adds unclassed last', () => {
    const rows = classRows(CLASSES, CONNECTIONS, initialJoints(), other);
    expect(rows.map((r) => [r.cls, r.label, r.joint, r.source])).toEqual([
      ['shelf', 'Shelves to sides', 'dado', 'style'],
      ['carcass', 'Top and bottom', 'rabbet', 'style'],
      ['back', 'Back', 'rabbet', 'style'],
      ['other', 'Other (other)', 'none', 'style'],
    ]);
  });

  it('offers only the joints valid for every connection of the class', () => {
    const shelf = classRows(CLASSES, CONNECTIONS, initialJoints(), other)[0];
    expect(shelf?.choices).toEqual(['none', 'dado', 'tabSlot', 'dowel']);
    expect(shelf?.keys).toEqual(['left~shelf#0', 'left~shelf#1', 'right~shelf#0']);
  });

  it('shows a class override and its sizes over what the connections show', () => {
    const joints = withClassParams(initialJoints(), 'shelf', 'dowel', { count: 3 });
    const shelf = classRows(CLASSES, CONNECTIONS, joints, other)[0];
    expect(shelf).toMatchObject({ joint: 'dowel', source: 'class', params: { count: 3 } });
  });

  it('reads the style joint from the connections not set by hand', () => {
    const rows: ConnectionRow[] = CONNECTIONS.map((c) =>
      c.key === 'left~shelf#0' ? { ...c, joint: 'tabSlot', source: 'connection' } : c,
    );
    const joints = withConnectionJoint(initialJoints(), 'left~shelf#0', 'tabSlot');
    const shelf = classRows(CLASSES, rows, joints, other)[0];
    expect(shelf?.joint).toBe('dado');
    expect(shelf?.overridden).toEqual(['left~shelf#0']);
  });
});

describe('joint choices', () => {
  it('keeps the overrides when the style changes', () => {
    const before = withConnectionJoint(
      withClassJoint(initialJoints(), 'shelf', 'dowel'),
      'left~top',
      'finger',
    );
    const after = withStyle(before, 'flatPack');
    expect(after).toEqual({
      style: 'flatPack',
      classes: { shelf: { joint: 'dowel' } },
      connections: { 'left~top': { joint: 'finger' } },
    });
  });

  it('drops a class override with null and leaves the rest', () => {
    const joints = withClassJoint(
      withClassJoint(initialJoints(), 'shelf', 'dowel'),
      'back',
      'groove',
    );
    expect(withClassJoint(joints, 'shelf', null).classes).toEqual({ back: { joint: 'groove' } });
  });

  it('clears the class connections set by hand for Use for all like this', () => {
    const joints = withConnectionJoint(
      withConnectionJoint(initialJoints(), 'left~shelf#0', 'tabSlot'),
      'left~top',
      'finger',
    );
    const all = withClassJoint(joints, 'shelf', 'dowel', ['left~shelf#0', 'left~shelf#1']);
    expect(all.classes).toEqual({ shelf: { joint: 'dowel' } });
    expect(all.connections).toEqual({ 'left~top': { joint: 'finger' } });
  });

  it('starts a new class joint from its own sizes', () => {
    const sized = withClassParams(initialJoints(), 'shelf', 'dowel', { count: 4 });
    expect(withClassJoint(sized, 'shelf', 'tabSlot').classes).toEqual({
      shelf: { joint: 'tabSlot' },
    });
  });

  it('keeps no empty params', () => {
    expect(withClassParams(initialJoints(), 'shelf', 'dado', {}).classes).toEqual({
      shelf: { joint: 'dado' },
    });
  });
});

describe('joint sizes', () => {
  it('offers sizes for the new kinds and the Joint Creator ones', () => {
    expect(jointSizeFields('camLock').map((f) => f.key)).toEqual(['diameter', 'depth', 'inset']);
    expect(jointSizeFields('groove').map((f) => f.key)).toEqual(['depth']);
    expect(jointSizeFields('screws').map((f) => f.key)).toEqual(['count', 'diameter', 'inset']);
    expect(jointSizeFields('dado').map((f) => f.key)).toEqual(['depth', 'stop']);
  });

  it('reads lengths typed in inches as millimetres and leaves empty ones to the joint', () => {
    expect(sizesOf('camLock', { diameter: '0.5', depth: '', inset: '1.25' }, 'inch')).toEqual({
      ok: true,
      params: { diameter: 12.7, inset: 31.75 },
    });
  });

  it('keeps counts unscaled and wants them whole and positive', () => {
    expect(sizesOf('none', { count: '4' }, 'inch')).toEqual({ ok: true, params: { count: 4 } });
    expect(sizesOf('none', { count: '2.5' }, 'mm')).toEqual({ ok: false, bad: ['count'] });
    expect(sizesOf('dado', { depth: '0', stop: 'x' }, 'mm')).toEqual({
      ok: false,
      bad: ['depth', 'stop'],
    });
  });

  it('shows typed sizes in the job unit', () => {
    expect(sizeTexts('camLock', { diameter: 12.7 }, 'inch')).toEqual({
      diameter: '0.5',
      depth: '',
      inset: '',
    });
  });
});

describe('templateId', () => {
  it('makes a kebab case id from a name', () => {
    expect(templateId('My Bookcase 2')).toBe('my-bookcase-2');
    expect(templateId("  Kid's  Step / Chair! ")).toBe('kid-s-step-chair');
    expect(templateId('!!!')).toBe('template');
  });
});

describe('sameValues', () => {
  it('compares every key both ways', () => {
    expect(sameValues({ a: 1, b: 'x' }, { b: 'x', a: 1 })).toBe(true);
    expect(sameValues({ a: 1 }, { a: 1, b: 2 })).toBe(false);
    expect(sameValues({ a: 1 }, { a: 1.0001 })).toBe(false);
  });
});

describe('CSV', () => {
  it('writes the cut list in inches with banding and quotes', () => {
    const csv = cutListCsv(
      [
        {
          names: ['Shelf, fixed', 'Shelf 2'],
          panels: ['shelf#1', 'shelf#2'],
          material: 'main',
          materialLabel: 'Carcass',
          length: 762,
          width: 279.4,
          thickness: 19.05,
          grain: 'long',
          banding: ['front'],
          bandingLength: 762,
          quantity: 2,
        },
      ],
      'inch',
    );
    expect(csv).toBe(
      'Panel,Material,Quantity,Length,Width,Thickness,Grain,Edge banding\n' +
        '"Shelf, fixed, Shelf 2",Carcass,2,30,11,0.75,Along length,front\n',
    );
  });

  it('writes the hardware rows', () => {
    expect(hardwareCsv([{ name: 'Dowel 8 x 30', count: 16, note: '' }])).toBe(
      'Item,Count,Note\nDowel 8 x 30,16,\n',
    );
  });

  it('writes the estimate with area and waste share', () => {
    const csv = estimateCsv(
      {
        sheets: [
          {
            material: 'main',
            label: 'Carcass',
            thickness: 18,
            count: 2,
            area: 5_950_000,
            used: 3_250_000,
            waste: 0.274,
          },
        ],
        seconds: 0,
        toolpaths: 0,
      },
      'mm',
    );
    expect(csv).toBe('Material,Thickness,Sheets,Area used,Waste\nCarcass,18,2,3.25 m²,27%\n');
  });

  it('gives area in square feet for inch jobs', () => {
    expect(areaText(304.8 * 304.8 * 12.5, 'inch')).toBe('12.50 ft²');
  });

  it('writes time as h:mm:ss', () => {
    expect(timeText(3725.4)).toBe('1:02:05');
    expect(timeText(59.6)).toBe('0:01:00');
  });
});

describe('carvingCount', () => {
  const row: CutListRow = {
    names: ['Door 1', 'Door 2'],
    panels: ['door#0', 'door#1'],
    material: 'main',
    materialLabel: 'Carcass',
    length: 700,
    width: 400,
    thickness: 18,
    grain: 'long',
    banding: [],
    bandingLength: 0,
    quantity: 2,
  };
  const attachment = (panel: string, kind: 'component' | 'vector', id: number): AttachmentRow => ({
    panel,
    kind,
    id,
    face: 'top',
    faceWord: 'front',
    at: [0.5, 0.5],
    stretch: false,
    otherSide: false,
  });

  it("counts the models on the row's panels, not vectors or other panels", () => {
    expect(
      carvingCount(row, [
        attachment('door#0', 'component', 1),
        attachment('door#1', 'component', 2),
        attachment('door#1', 'vector', 5),
        attachment('side', 'component', 3),
      ]),
    ).toBe(2);
  });

  it('counts none for a row without those panels', () => {
    expect(carvingCount({ ...row, panels: [] }, [attachment('door#0', 'component', 1)])).toBe(0);
  });
});

describe('followValues', () => {
  it('keeps a typed calculation that still gives its value and shows changed ones', () => {
    const fields = { width: '600+200', shelves: '4', back: 'inset' };
    const before = { width: 700, shelves: 4, back: 'inset' };
    const after = { width: 800, shelves: 5, back: 'none' };
    expect(followValues([WIDTH, SHELVES, BACK], fields, before, after, 'mm')).toEqual({
      width: '600+200',
      shelves: '5',
      back: 'none',
    });
  });

  it('shows a dragged length in inches', () => {
    const shown = followValues(
      [WIDTH],
      { width: '31.5' },
      { width: 800.1 },
      { width: 914.4 },
      'inch',
    );
    expect(shown).toEqual({ width: '36' });
  });

  it('leaves a field being typed when its value did not change', () => {
    const fields = { width: '9', shelves: '4' };
    const before = { width: 800, shelves: 4 };
    const after = { width: 800, shelves: 6 };
    expect(followValues([WIDTH, SHELVES], fields, before, after, 'mm')).toEqual({
      width: '9',
      shelves: '6',
    });
  });
});

describe('jointsFromConnections', () => {
  it('reads class joints and hand set seams back, leaving the style ones', () => {
    const rows: ConnectionRow[] = [
      {
        ...connection('left~shelf#0', 'shelf', 'dowel', ['dowel']),
        source: 'class',
        params: { count: 3 },
      },
      {
        ...connection('left~shelf#1', 'shelf', 'dowel', ['dowel']),
        source: 'class',
        params: { count: 3 },
      },
      { ...connection('left~top', 'carcass', 'finger', ['finger']), source: 'connection' },
      connection('back~left', 'back', 'rabbet', ['rabbet']),
    ];
    expect(jointsFromConnections('flatPack', rows)).toEqual({
      style: 'flatPack',
      classes: { shelf: { joint: 'dowel', params: { count: 3 } } },
      connections: { 'left~top': { joint: 'finger' } },
    });
  });
});

describe('stockChoices', () => {
  const T: FurnitureParam = {
    key: 't',
    label: 'Thickness',
    kind: 'thickness',
    default: 18,
    min: 12,
    max: 25,
  };
  const stocks: StockThickness[] = [
    {
      id: 'ply-3-4',
      label: 'Plywood 3/4 in nominal',
      nominal: 19.05,
      actual: 18.3,
      system: 'inch',
    },
    { id: 'ply-1-4', label: 'Plywood 1/4 in nominal', nominal: 6.35, actual: 5.2, system: 'inch' },
    { id: 'mdf-18', label: 'MDF 18 mm', nominal: 18, actual: 18, system: 'metric' },
    { id: 'ply-12', label: 'Birch ply 12 mm', nominal: 12, actual: 12, system: 'metric' },
    { id: 'ply-25', label: 'Birch ply 25 mm', nominal: 25, actual: 25.01, system: 'metric' },
  ];

  it('offers the stock within the bounds, the job system first, thinnest first', () => {
    expect(stockChoices(T, stocks, 'mm').map((c) => c.id)).toEqual(['ply-12', 'mdf-18', 'ply-3-4']);
    expect(stockChoices(T, stocks, 'mm')[0]?.text).toBe('12');
    // The size is said once: a metric stock names it already.
    expect(stockChoices(T, stocks, 'mm').map((c) => c.label)).toEqual([
      'Birch ply 12 mm',
      'MDF 18 mm',
      'Plywood 3/4 in nominal, 18.3 mm',
    ]);
  });

  it('gives inch jobs the inch stock first and its text in inches', () => {
    const choices = stockChoices(T, stocks, 'inch');
    expect(choices.map((c) => c.id)).toEqual(['ply-3-4', 'ply-12', 'mdf-18']);
    expect(choices[0]?.text).toBe(formatLength(18.3, 'inch'));
    expect(choices[0]?.text).toBe('0.7205');
  });

  it('offers nothing for other kinds of parameter', () => {
    expect(stockChoices({ ...T, kind: 'length' }, stocks, 'mm')).toEqual([]);
  });
});
