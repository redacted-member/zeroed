import type { JSX, MouseEvent } from 'react';

import type { HeightBand } from '../SliceModelForm/model';

import './SliceHeightBar.scss';

export interface SliceHeightBarProps {
  /** The slices, bottom up, as fractions of the model height. */
  bands: readonly HeightBand[];
  /** The slice height being edited (the top of band `active + 1`), if any. */
  active: number | null;
  /** Accessible name; the bar only shows what the list says. */
  label: string;
  /** A double click adds a height at `fromBottom` (0 foot, 1 top). */
  onAddAt: (fromBottom: number) => void;
}

/** A tall bar split into the slices, the slice height being edited marked. */
export function SliceHeightBar({
  bands,
  active,
  label,
  onAddAt,
}: SliceHeightBarProps): JSX.Element {
  const onDoubleClick = (event: MouseEvent<HTMLDivElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    if (box.height > 0) {
      onAddAt((box.bottom - event.clientY) / box.height);
    }
  };
  return (
    <div aria-label={label} className="slice-height-bar" onDoubleClick={onDoubleClick} role="img">
      {bands.map((band) => (
        <div
          className={
            band.index % 2 === 0
              ? 'slice-height-bar__band slice-height-bar__band--even'
              : 'slice-height-bar__band'
          }
          key={band.index}
          // The band's place is computed from the heights.
          style={{ bottom: `${band.from * 100}%`, height: `${(band.to - band.from) * 100}%` }}
        />
      ))}
      {bands.slice(0, -1).map((band, k) => (
        <div
          className={
            k === active
              ? 'slice-height-bar__cut slice-height-bar__cut--active'
              : 'slice-height-bar__cut'
          }
          key={band.index}
          style={{ bottom: `${band.to * 100}%` }}
        />
      ))}
    </div>
  );
}
