/** The logo stays at least this long after the page starts, so a quick start does not flash it. */
export const SPLASH_MIN_MS = 1200;

/** How long the fade takes: `.splash`'s transition in Splash.scss. */
export const SPLASH_FADE_MS = 240;

/** The parts of the splash element this file uses. */
export interface SplashElement {
  dataset: { hiding?: string };
  classList: { add(name: string): void };
  remove(): void;
}

/** How long to keep the logo up once the app is ready, `elapsed` ms after the page started. */
export function splashDelay(elapsed: number): number {
  return Math.max(0, SPLASH_MIN_MS - elapsed);
}

/** Fades the splash out and takes it off the page; asking again does nothing. */
export function hideSplash(splash: SplashElement | null, elapsed: number): void {
  if (splash === null || splash.dataset.hiding !== undefined) {
    return;
  }
  splash.dataset.hiding = 'true';
  setTimeout(() => {
    splash.classList.add('splash--done');
    setTimeout(() => splash.remove(), SPLASH_FADE_MS);
  }, splashDelay(elapsed));
}
