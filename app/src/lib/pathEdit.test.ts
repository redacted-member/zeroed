import { describe, expect, it } from 'vitest';

import type { Path } from '../bridge/document';
import { rectPath } from './geometry';
import {
  alignNodes,
  deleteSpan,
  dragSpan,
  insertMidpoint,
  mirrorNode,
  moveNodes,
  convertSegment,
  cutAtNode,
  deleteNode,
  insertNode,
  listHandles,
  listNodes,
  moveNode,
  nearestSegment,
  setStartNode,
  toggleSmooth,
  closeSubpaths,
  nodePosition,
  reversePath,
} from './pathEdit';

const square = (): Path => rectPath(0, 0, 10, 10);

describe('listNodes', () => {
  it('lists one node per corner of a closed square', () => {
    const nodes = listNodes(square());
    expect(nodes).toHaveLength(4);
    expect(nodes[0]?.start).toBe(true);
    expect(nodes.every((n) => !n.smooth)).toBe(true);
  });

  it('lists both ends of an open polyline', () => {
    const path: Path = {
      subpaths: [
        { start: { x: 0, y: 0 }, closed: false, segments: [{ kind: 'line', to: { x: 5, y: 0 } }] },
      ],
    };
    expect(listNodes(path)).toHaveLength(2);
  });
});

describe('moveNode', () => {
  it('moves a corner and keeps the square closed', () => {
    const moved = moveNode(square(), { subpath: 0, node: 1 }, { x: 12, y: -1 });
    expect(moved.subpaths[0]?.segments[0]?.to).toEqual({ x: 12, y: -1 });
    expect(listNodes(moved)).toHaveLength(4);
  });

  it('moves the start node and the closing segment together', () => {
    const moved = moveNode(square(), { subpath: 0, node: 0 }, { x: -3, y: -3 });
    const sub = moved.subpaths[0];
    expect(sub?.start).toEqual({ x: -3, y: -3 });
    expect(sub?.segments[3]?.to).toEqual({ x: -3, y: -3 });
  });

  it('drags cubic handles along with their node', () => {
    const path: Path = {
      subpaths: [
        {
          start: { x: 0, y: 0 },
          closed: false,
          segments: [
            { kind: 'cubic', c1: { x: 1, y: 1 }, c2: { x: 9, y: 1 }, to: { x: 10, y: 0 } },
          ],
        },
      ],
    };
    const moved = moveNode(path, { subpath: 0, node: 1 }, { x: 10, y: 5 });
    const segment = moved.subpaths[0]?.segments[0];
    expect(segment?.kind === 'cubic' ? segment.c2 : null).toEqual({ x: 9, y: 6 });
  });
});

describe('insertNode and deleteNode', () => {
  it('inserts a node halfway along a line', () => {
    const path = insertNode(square(), { subpath: 0, segment: 0 }, 0.5);
    expect(listNodes(path)).toHaveLength(5);
    expect(path.subpaths[0]?.segments[0]?.to).toEqual({ x: 5, y: 0 });
  });

  it('splits an arc into two arcs covering the same sweep', () => {
    const path: Path = {
      subpaths: [
        {
          start: { x: -5, y: 0 },
          closed: false,
          segments: [{ kind: 'arc', to: { x: 5, y: 0 }, bulge: 1 }],
        },
      ],
    };
    const split = insertNode(path, { subpath: 0, segment: 0 }, 0.5);
    const [a, b] = split.subpaths[0]?.segments ?? [];
    expect(a?.kind).toBe('arc');
    expect(b?.kind).toBe('arc');
    if (a?.kind === 'arc' && b?.kind === 'arc') {
      expect(Math.abs(a.bulge - Math.tan(Math.PI / 8))).toBeLessThan(1e-9);
      expect(Math.abs(b.bulge - Math.tan(Math.PI / 8))).toBeLessThan(1e-9);
      // A positive bulge sweeps counter clockwise, so left to right passes below.
      expect(Math.abs(a.to.x)).toBeLessThan(1e-9);
      expect(Math.abs(a.to.y + 5)).toBeLessThan(1e-9);
    }
  });

  it('deletes a corner, joining its neighbours with a line', () => {
    const path = deleteNode(square(), { subpath: 0, node: 1 });
    expect(listNodes(path)).toHaveLength(3);
    expect(path.subpaths[0]?.segments[0]?.to).toEqual({ x: 10, y: 10 });
  });

  it('refuses to delete below two nodes', () => {
    const path: Path = {
      subpaths: [
        { start: { x: 0, y: 0 }, closed: false, segments: [{ kind: 'line', to: { x: 5, y: 0 } }] },
      ],
    };
    expect(deleteNode(path, { subpath: 0, node: 0 })).toEqual(path);
  });
});

describe('convert, smooth, start and cut', () => {
  it('converts a line to a bezier with collinear handles', () => {
    const path = convertSegment(square(), { subpath: 0, segment: 0 }, 'cubic');
    const segment = path.subpaths[0]?.segments[0];
    expect(segment?.kind).toBe('cubic');
    expect(listHandles(path)).toHaveLength(2);
  });

  it('smooths a node between two beziers', () => {
    let path = convertSegment(square(), { subpath: 0, segment: 0 }, 'cubic');
    path = convertSegment(path, { subpath: 0, segment: 1 }, 'cubic');
    const smoothed = toggleSmooth(path, { subpath: 0, node: 1 });
    expect(listNodes(smoothed)[1]?.smooth).toBe(true);
  });

  it('rotates the start point of a closed path', () => {
    const path = setStartNode(square(), { subpath: 0, node: 2 });
    expect(path.subpaths[0]?.start).toEqual({ x: 10, y: 10 });
    expect(listNodes(path)).toHaveLength(4);
  });

  it('opens a closed path at a node', () => {
    const path = cutAtNode(square(), { subpath: 0, node: 2 });
    expect(path.subpaths[0]?.closed).toBe(false);
    expect(listNodes(path)).toHaveLength(5);
  });

  it('splits an open path into two at an inner node', () => {
    const open = cutAtNode(square(), { subpath: 0, node: 0 });
    const split = cutAtNode(open, { subpath: 0, node: 2 });
    expect(split.subpaths).toHaveLength(2);
  });

  it('finds the nearest segment to a point', () => {
    const hit = nearestSegment(square(), { x: 5, y: -1 }, 0.01);
    expect(hit?.ref).toEqual({ subpath: 0, segment: 0 });
    expect(hit?.dist).toBe(1);
    expect(hit?.t).toBeCloseTo(0.5, 6);
  });

  it('moves and lines up several nodes, finds mirror nodes', () => {
    const square = rectPath(0, 0, 10, 10);
    const moved = moveNodes(
      square,
      [
        { subpath: 0, node: 0 },
        { subpath: 0, node: 1 },
      ],
      { x: 0, y: -2 },
    );
    expect(
      listNodes(moved)
        .map((n) => n.y)
        .slice(0, 2),
    ).toEqual([-2, -2]);
    const lined = alignNodes(
      moved,
      [
        { subpath: 0, node: 2 },
        { subpath: 0, node: 0 },
      ],
      'y',
    );
    expect(listNodes(lined)[0]?.y).toBe(10);
    expect(mirrorNode(square, { subpath: 0, node: 0 }, 'h')).toEqual({ subpath: 0, node: 1 });
  });

  it('inserts midpoints, deletes spans and drags spans', () => {
    const square = rectPath(0, 0, 10, 10);
    expect(listNodes(insertMidpoint(square, { subpath: 0, segment: 0 }))).toHaveLength(5);
    const opened = deleteSpan(square, { subpath: 0, segment: 0 });
    expect(opened.subpaths[0]?.closed).toBe(false);
    expect(opened.subpaths[0]?.segments).toHaveLength(3);
    const line: Path = {
      subpaths: [
        {
          start: { x: 0, y: 0 },
          closed: false,
          segments: [
            { kind: 'line', to: { x: 10, y: 0 } },
            { kind: 'line', to: { x: 20, y: 0 } },
            { kind: 'line', to: { x: 30, y: 0 } },
          ],
        },
      ],
    };
    expect(deleteSpan(line, { subpath: 0, segment: 1 }).subpaths).toHaveLength(2);
    const shifted = dragSpan(line, { subpath: 0, segment: 1 }, 0.5, { x: 0, y: 5 }, false);
    expect(listNodes(shifted).map((n) => n.y)).toEqual([0, 5, 5, 0]);
    const arc: Path = {
      subpaths: [
        {
          start: { x: 0, y: 0 },
          closed: false,
          segments: [{ kind: 'arc', to: { x: 10, y: 0 }, bulge: 0.2 }],
        },
      ],
    };
    // The middle sits 1 below the chord; dragged up 3 it rises 2 above, bending left.
    const bent = dragSpan(arc, { subpath: 0, segment: 0 }, 0.5, { x: 0, y: 3 }, false);
    const seg = bent.subpaths[0]?.segments[0];
    expect(seg?.kind === 'arc' ? Math.round(seg.bulge * 1e6) / 1e6 : null).toBe(-0.4);
  });

  it('keeps the end directions of a curve in keep tangency mode', () => {
    const curve: Path = {
      subpaths: [
        {
          start: { x: 0, y: 0 },
          closed: false,
          segments: [
            { kind: 'cubic', c1: { x: 0, y: 5 }, c2: { x: 10, y: 5 }, to: { x: 10, y: 0 } },
          ],
        },
      ],
    };
    const pulled = dragSpan(curve, { subpath: 0, segment: 0 }, 0.5, { x: 2, y: 3 }, false, true);
    const seg = pulled.subpaths[0]?.segments[0];
    expect(seg?.kind).toBe('cubic');
    if (seg?.kind !== 'cubic') {
      return;
    }
    // Both handles stay vertical above their nodes.
    expect(seg.c1.x).toBeCloseTo(0);
    expect(seg.c2.x).toBeCloseTo(10);
    expect(seg.c1.y).toBeGreaterThan(5);
    expect(seg.c2.y).toBeGreaterThan(5);
    // Without the mode the handles swing sideways.
    const free = dragSpan(curve, { subpath: 0, segment: 0 }, 0.5, { x: 2, y: 3 }, false);
    const loose = free.subpaths[0]?.segments[0];
    expect(loose?.kind === 'cubic' ? loose.c1.x : 0).not.toBeCloseTo(0);
  });
});

describe('reversePath and closeSubpaths', () => {
  const open: Path = {
    subpaths: [
      {
        start: { x: 0, y: 0 },
        closed: false,
        segments: [
          { kind: 'line', to: { x: 10, y: 0 } },
          { kind: 'arc', to: { x: 20, y: 0 }, bulge: 0.5 },
          { kind: 'cubic', c1: { x: 22, y: 5 }, c2: { x: 28, y: 5 }, to: { x: 30, y: 0 } },
        ],
      },
    ],
  };

  it('runs every span the other way', () => {
    const back = reversePath(open);
    const sub = back.subpaths[0];
    expect(sub?.start).toEqual({ x: 30, y: 0 });
    expect(sub?.segments).toEqual([
      { kind: 'cubic', c1: { x: 28, y: 5 }, c2: { x: 22, y: 5 }, to: { x: 20, y: 0 } },
      { kind: 'arc', to: { x: 10, y: 0 }, bulge: -0.5 },
      { kind: 'line', to: { x: 0, y: 0 } },
    ]);
    expect(reversePath(back)).toEqual(open);
  });

  it('closes an open vector with a straight span back to its start', () => {
    const closed = closeSubpaths(open);
    const sub = closed.subpaths[0];
    expect(sub?.closed).toBe(true);
    expect(sub?.segments.at(-1)).toEqual({ kind: 'line', to: { x: 0, y: 0 } });
    expect(closeSubpaths(closed)).toEqual(closed);
    expect(nodePosition(open, { subpath: 0, node: 2 })).toEqual({ x: 20, y: 0 });
  });
});
