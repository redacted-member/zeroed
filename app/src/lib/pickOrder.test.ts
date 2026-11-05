import { describe, expect, it } from 'vitest';
import { afterPicks, picking, trackPick } from './pickOrder';

describe('pickOrder', () => {
  it('lets a key wait for the click before it', async () => {
    const order: string[] = [];
    let finish = () => {};
    const pick = new Promise<void>((resolve) => {
      finish = () => {
        order.push('picked');
        resolve();
      };
    });
    void trackPick(pick);
    expect(picking()).toBe(true);
    const key = afterPicks().then(() => order.push('nudged'));
    await Promise.resolve();
    expect(order).toEqual([]);
    finish();
    await key;
    expect(order).toEqual(['picked', 'nudged']);
    expect(picking()).toBe(false);
  });

  it('does not hold keys after a pick fails', async () => {
    await trackPick(Promise.reject(new Error('gone'))).catch(() => undefined);
    await afterPicks();
    expect(picking()).toBe(false);
  });
});
