import { describe, expect, it } from 'vitest';

import type { PlainTextArgs } from '../../bridge/document';
import { LiveText, sameText, type LiveTextIo } from './liveText';

function args(text: string, x = 0): PlainTextArgs {
  return {
    spec: {
      text,
      fontFamily: 'Sans',
      bold: false,
      italic: false,
      sizeMm: 10,
      alignment: 'left',
      lineSpacing: 1,
      charSpacing: 0,
      kerning: true,
    },
    x,
    y: 0,
  };
}

function recorder() {
  const calls: string[] = [];
  let revision = 10;
  const targets: Array<number | null> = [];
  const io: LiveTextIo = {
    create: (a) => {
      calls.push(`create ${a.spec.text}`);
      revision += 2;
      return Promise.resolve({ id: 7, revision });
    },
    update: (id, a, merge) => {
      calls.push(`update ${id} ${a.spec.text} ${a.x} merge ${merge}`);
      revision += 1;
      return Promise.resolve(revision);
    },
    onTarget: (id) => targets.push(id),
    onError: () => undefined,
  };
  return { calls, targets, io };
}

describe('LiveText', () => {
  it('makes the text on the first change, then edits it with the last revision', async () => {
    const { calls, targets, io } = recorder();
    const live = new LiveText(io);
    live.apply(args('A'));
    await live.settled();
    live.apply(args('AB'));
    await live.settled();
    live.apply(args('AB', 5));
    await live.settled();
    expect(calls).toEqual(['create A', 'update 7 AB 0 merge 12', 'update 7 AB 5 merge 13']);
    expect(targets).toEqual([7]);
    expect(live.target).toBe(7);
  });

  it('collapses changes made while a call is out into the newest', async () => {
    const { calls, io } = recorder();
    const live = new LiveText(io);
    live.apply(args('A'));
    live.apply(args('AB'));
    live.apply(args('ABC'));
    await live.settled();
    expect(calls).toEqual(['create A', 'update 7 ABC 0 merge 12']);
  });

  it('does not send the loaded layout back, and edits a retargeted text', async () => {
    const { calls, io } = recorder();
    const live = new LiveText(io);
    live.retarget(3, args('Old'));
    live.apply(args('Old'));
    await live.settled();
    expect(calls).toEqual([]);
    live.apply(args('New'));
    await live.settled();
    expect(calls).toEqual(['update 3 New 0 merge null']);
    live.retarget(null);
    live.apply(args('Next'));
    await live.settled();
    expect(calls[1]).toBe('create Next');
  });

  it('compares layouts by value', () => {
    expect(sameText(args('A'), args('A'))).toBe(true);
    expect(sameText(args('A'), args('A', 1))).toBe(false);
    expect(sameText(null, args('A'))).toBe(false);
  });
});
