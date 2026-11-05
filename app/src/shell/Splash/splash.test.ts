import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  hideSplash,
  SPLASH_FADE_MS,
  SPLASH_MIN_MS,
  splashDelay,
  type SplashElement,
} from './splash';

function fakeSplash(): SplashElement & { classes: string[]; removed: number } {
  const fake = {
    dataset: {},
    classes: [] as string[],
    removed: 0,
    classList: { add: (name: string) => fake.classes.push(name) },
    remove: () => {
      fake.removed += 1;
    },
  };
  return fake;
}

describe('splashDelay', () => {
  it('holds the logo up to the minimum and no longer', () => {
    expect(SPLASH_MIN_MS).toBe(1200);
    expect(splashDelay(0)).toBe(1200);
    expect(splashDelay(500)).toBe(700);
    expect(splashDelay(1200)).toBe(0);
    expect(splashDelay(5000)).toBe(0);
  });
});

describe('hideSplash', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('waits out the minimum, fades, then leaves the page', () => {
    const splash = fakeSplash();
    hideSplash(splash, 200);
    vi.advanceTimersByTime(999);
    expect(splash.classes).toEqual([]);
    vi.advanceTimersByTime(1);
    expect(splash.classes).toEqual(['splash--done']);
    expect(splash.removed).toBe(0);
    vi.advanceTimersByTime(SPLASH_FADE_MS);
    expect(splash.removed).toBe(1);
  });

  it('runs once when asked twice', () => {
    const splash = fakeSplash();
    hideSplash(splash, 5000);
    hideSplash(splash, 5000);
    vi.advanceTimersByTime(1000);
    expect(splash.classes).toEqual(['splash--done']);
    expect(splash.removed).toBe(1);
  });

  it('does nothing with no splash on the page', () => {
    expect(() => hideSplash(null, 0)).not.toThrow();
  });
});
