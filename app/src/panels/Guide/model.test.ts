import { describe, expect, it } from 'vitest';

import { contentProblems } from '../../lib/usSpelling';
import { GUIDE_IDS, type GuideId } from '../../state/guideStore';
import {
  CARD_GAP,
  GUIDES,
  RING_GAP,
  placeCard,
  ringBox,
  targetDock,
  targetSelector,
} from './model';

const steps = GUIDE_IDS.flatMap((id) => GUIDES[id].steps.map((step) => ({ id, step })));

describe('guide steps', () => {
  it('has every walkthrough, each with a title and a few steps', () => {
    expect(Object.keys(GUIDES).sort()).toEqual([...GUIDE_IDS].sort());
    for (const id of GUIDE_IDS) {
      expect(GUIDES[id].title, id).not.toBe('');
      expect(GUIDES[id].blurb, id).not.toBe('');
      expect(GUIDES[id].steps.length, id).toBeGreaterThanOrEqual(5);
    }
  });

  it('points every step at a control the window has', () => {
    for (const { id, step } of steps) {
      if (step.target !== undefined) {
        expect(targetSelector(step.target), `${id}: ${step.target}`).not.toBeNull();
      }
      expect(step.title, id).not.toBe('');
      expect(step.text, id).not.toBe('');
    }
  });

  it('waits for a click only on a step that points at something', () => {
    for (const { id, step } of steps) {
      if (step.click === true) {
        expect(step.target, `${id}: ${step.title}`).toBeDefined();
      }
    }
  });

  it('tours the Design tab, the views and the Toolpaths panel', () => {
    const targets = GUIDES.tour.steps.map((s) => s.target);
    expect(targets).toEqual(expect.arrayContaining(['designTab', 'views', 'toolpaths']));
  });

  it('walks each project through the controls to click, in order', () => {
    const clicks = (id: GuideId) =>
      GUIDES[id].steps.filter((s) => s.click === true).map((s) => s.target);
    expect(clicks('sign')).toEqual([
      'tool:jobSetup',
      'tool:drawRectangle',
      'tool:createText',
      'tool:vgroove',
      'tool:preview',
      'tool:saveToolpaths',
    ]);
    expect(clicks('relief')).toEqual(
      expect.arrayContaining(['tool:createShape', 'tool:rough3d', 'tool:finish3d']),
    );
    expect(clicks('rotary')).toEqual([
      'tool:jobSetup',
      'tool:createText',
      'tool:pocket',
      'tool:preview',
      'tool:saveToolpaths',
    ]);
    expect(clicks('cutout')).toEqual([
      'tool:jobSetup',
      'tool:importVectors',
      'tool:profile',
      'tool:preview',
      'tool:saveToolpaths',
    ]);
    expect(clicks('laser')).toEqual(expect.arrayContaining(['tool:laserCutFill']));
    expect(clicks('nesting')).toEqual(expect.arrayContaining(['tool:nest']));
    expect(clicks('machine')).toEqual(['tool:toolDatabase']);
    expect(clicks('trace')).toEqual(expect.arrayContaining(['tool:traceBitmap']));
    expect(clicks('box')).toEqual([
      'tool:drawRectangle',
      'tool:jointCreator',
      'tool:preview',
      'assembly',
    ]);
  });

  it('is written in US spelling, without em dashes or other product names', () => {
    const texts = GUIDE_IDS.flatMap((id) => [
      GUIDES[id].title,
      GUIDES[id].blurb,
      ...GUIDES[id].steps.flatMap((s) => [s.title, s.text]),
    ]);
    expect(texts.flatMap(contentProblems)).toEqual([]);
  });
});

describe('the welcome card', () => {
  it('offers every walkthrough once, with the first steps first', () => {
    expect(GUIDE_IDS.slice(0, 2)).toEqual(['tour', 'machine']);
    expect(new Set(GUIDE_IDS).size).toBe(GUIDE_IDS.length);
    expect(Object.keys(GUIDES).sort()).toEqual([...GUIDE_IDS].sort());
  });
});

describe('targets', () => {
  it('finds tools by id, and nothing for an unknown name', () => {
    expect(targetSelector('tool:vgroove')).toBe('[data-tool="vgroove"]');
    expect(targetSelector('tool:noSuchTool')).toBeNull();
    expect(targetSelector('designTab')).toBe('#dock-tab-design');
    expect(targetSelector('nowhere')).toBeNull();
  });

  it('knows which dock to show for a target', () => {
    expect(targetDock('tool:drawCircle')).toBe('design');
    expect(targetDock('tool:profile')).toBe('toolpaths');
    expect(targetDock('toolpaths')).toBe('toolpaths');
    expect(targetDock('view3d')).toBeNull();
  });
});

describe('placeCard', () => {
  const card = { width: 300, height: 150 };
  const view = { width: 1400, height: 900 };

  it('centers the card with no target', () => {
    expect(placeCard(null, card, view)).toEqual({ left: 550, top: 374 });
  });

  it('puts the card to the right of a target on the left', () => {
    const at = placeCard({ left: 40, top: 100, width: 30, height: 30 }, card, view);
    expect(at).toEqual({ left: 70 + CARD_GAP, top: 100 });
  });

  it('puts the card to the left of a target on the right', () => {
    const at = placeCard({ left: 1300, top: 100, width: 60, height: 30 }, card, view);
    expect(at).toEqual({ left: 1300 - CARD_GAP - 300, top: 100 });
  });

  it('keeps the card inside the window', () => {
    const at = placeCard({ left: 40, top: 850, width: 30, height: 30 }, card, view);
    expect(at.top).toBe(900 - 150 - CARD_GAP);
  });

  it('lands on whole, even pixels when the room would split one', () => {
    const at = placeCard(null, { width: 301, height: 181 }, { width: 1101, height: 701 });
    expect(at).toEqual({ left: 400, top: 260 });
    const beside = placeCard({ left: 11, top: 21.5, width: 33, height: 11 }, card, view);
    expect(beside.left % 2).toBe(0);
    expect(beside.top % 2).toBe(0);
  });

  it('centers the card over a target that fills the window', () => {
    const at = placeCard({ left: 0, top: 0, width: 1400, height: 900 }, card, view);
    expect(at).toEqual({ left: 550, top: 374 });
  });
});

describe('ringBox', () => {
  it('rings a control a little larger, inside the window', () => {
    const view = { width: 1024, height: 768 };
    expect(ringBox({ left: 100, top: 100, width: 30, height: 30 }, view)).toEqual({
      left: 100 - RING_GAP,
      top: 100 - RING_GAP,
      width: 30 + 2 * RING_GAP,
      height: 30 + 2 * RING_GAP,
    });
    expect(ringBox({ left: 720, top: 0, width: 304, height: 768 }, view)).toEqual({
      left: 716,
      top: 0,
      width: 308,
      height: 768,
    });
  });
});
