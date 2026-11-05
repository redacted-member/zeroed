import { describe, expect, it } from 'vitest';

import {
  dropPointAt,
  dropTargetOf,
  isDrag,
  registerDropTarget,
  type DropResolver,
  dropOnFace,
  type FaceDropHandler,
} from './drop';

/** A stand in for a DOM element: only the parent link is read. */
function node(parent: Element | null = null): Element {
  return { parentElement: parent } as unknown as Element;
}

describe('dragging a clip onto a view', () => {
  it('becomes a drag only past the threshold', () => {
    expect(isDrag({ x: 0, y: 0 }, { x: 2, y: 2 })).toBe(false);
    expect(isDrag({ x: 0, y: 0 }, { x: 3, y: 3 })).toBe(true);
    expect(isDrag({ x: 10, y: 10 }, { x: 10, y: 5 }, 5)).toBe(true);
  });

  it('finds the view holding the element under the pointer', () => {
    const view = node();
    const canvas = node(view);
    const elsewhere = node();
    const resolve: DropResolver = () => ({ x: 1, y: 2 });
    const registry = new Map([[view, resolve]]);
    expect(dropTargetOf(canvas, registry)).toBe(resolve);
    expect(dropTargetOf(view, registry)).toBe(resolve);
    expect(dropTargetOf(elsewhere, registry)).toBeNull();
    expect(dropTargetOf(null, registry)).toBeNull();
  });

  it('lands where the registered view says, and nowhere once it is gone', () => {
    const view = node();
    const canvas = node(view);
    const seen: number[][] = [];
    const stop = registerDropTarget(view, (x, y) => {
      seen.push([x, y]);
      return { x: x / 2, y: y / 2 };
    });
    expect(dropPointAt(40, 60, () => canvas)).toEqual({ x: 20, y: 30 });
    expect(seen).toEqual([[40, 60]]);
    expect(dropPointAt(40, 60, () => node())).toBeNull();
    stop();
    expect(dropPointAt(40, 60, () => canvas)).toBeNull();
  });

  it('keeps a newer registration when an older one is undone', () => {
    const view = node();
    const stopOld = registerDropTarget(view, () => ({ x: 1, y: 1 }));
    const stopNew = registerDropTarget(view, () => ({ x: 2, y: 2 }));
    stopOld();
    expect(dropPointAt(0, 0, () => view)).toEqual({ x: 2, y: 2 });
    stopNew();
  });
});

describe('dropping a clip on a furniture face', () => {
  it('lets the face under the point take the clip, from a child element too', () => {
    const view = { parentElement: null } as unknown as Element;
    const child = { parentElement: view } as unknown as Element;
    const took: string[] = [];
    const registry = new Map<Element, FaceDropHandler>([
      [
        view,
        (_x, _y, clip) => {
          took.push(clip);
          return clip.endsWith('.carveclip');
        },
      ],
    ]);
    expect(dropOnFace(5, 5, 'rose.carveclip', () => child, registry)).toBe(true);
    // The face handler says no (not on a face): the clip lands as before.
    expect(dropOnFace(5, 5, 'leaf.svg', () => child, registry)).toBe(false);
    expect(took).toEqual(['rose.carveclip', 'leaf.svg']);
  });

  it('leaves the clip alone off any face target', () => {
    const other = { parentElement: null } as unknown as Element;
    expect(dropOnFace(5, 5, 'rose.carveclip', () => other, new Map())).toBe(false);
    expect(dropOnFace(5, 5, 'rose.carveclip', () => null, new Map())).toBe(false);
  });
});
