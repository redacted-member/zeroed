// High DPI: the views draw at the display's pixel ratio, and redraw at the
// new ratio when the window moves to a display with another one (or the
// page zoom changes), which a resize observer alone does not report.

/** The parts of `window` the watcher uses, so tests can pass a fake. */
export interface PixelRatioHost {
  readonly devicePixelRatio: number;
  matchMedia(query: string): Pick<MediaQueryList, 'addEventListener' | 'removeEventListener'>;
}

/** Calls `onChange` with the new ratio each time the pixel ratio changes. Answers a stop function. */
export function watchPixelRatio(
  onChange: (ratio: number) => void,
  host: PixelRatioHost = window,
): () => void {
  let stopped = false;
  let current: Pick<MediaQueryList, 'addEventListener' | 'removeEventListener'> | null = null;
  const listener = () => {
    arm();
    onChange(host.devicePixelRatio);
  };
  const arm = () => {
    current?.removeEventListener('change', listener);
    if (stopped) {
      return;
    }
    // A query that stops matching as soon as the ratio moves off this value.
    current = host.matchMedia(`(resolution: ${host.devicePixelRatio}dppx)`);
    current.addEventListener('change', listener);
  };
  arm();
  return () => {
    stopped = true;
    current?.removeEventListener('change', listener);
    current = null;
  };
}
