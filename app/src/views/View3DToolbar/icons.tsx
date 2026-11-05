// Line icons for the 3D view toolbar, on a 24 by 24 grid with currentColor
// strokes. The view icons draw a box in oblique projection (front square,
// top and right faces, hidden edges dashed) and fill the face the view
// looks at.

import type { JSX, ReactNode } from 'react';

import type { ViewPreset } from '../View3D/camera3d';

function svg(children: ReactNode): JSX.Element {
  return (
    <svg aria-hidden="true" className="view-3d-icon" focusable="false" viewBox="0 0 24 24">
      {children}
    </svg>
  );
}

const FACES: Readonly<Record<Exclude<ViewPreset, 'iso'>, string>> = {
  front: '4,8 16,8 16,20 4,20',
  top: '4,8 8,4 20,4 16,8',
  right: '16,8 20,4 20,16 16,20',
  back: '8,4 20,4 20,16 8,16',
  left: '4,8 8,4 8,16 4,20',
  bottom: '4,20 8,16 20,16 16,20',
};

const BOX = (
  <>
    <path className="view-3d-icon__hidden" d="M8 4v12h12M8 16l-4 4" />
    <path d="M4 8h12v12H4zM4 8l4-4h12l-4 4M20 4v12l-4 4" />
  </>
);

function boxIcon(faces: readonly string[]): JSX.Element {
  return svg(
    <>
      {faces.map((points) => (
        <polygon className="view-3d-icon__face" key={points} points={points} />
      ))}
      {BOX}
    </>,
  );
}

export const VIEW_ICONS: Readonly<Record<ViewPreset, JSX.Element>> = {
  iso: boxIcon([FACES.front, FACES.top, FACES.right]),
  top: boxIcon([FACES.top]),
  front: boxIcon([FACES.front]),
  back: boxIcon([FACES.back]),
  left: boxIcon([FACES.left]),
  right: boxIcon([FACES.right]),
  bottom: boxIcon([FACES.bottom]),
};

export const FIT_ICON = svg(
  <>
    <path d="M3 8V3h5M16 3h5v5M21 16v5h-5M8 21H3v-5" />
    <path className="view-3d-icon__thin" d="M8 10l4-3 4 3v6H8z" />
  </>,
);

/** A box drawn with parallel sight lines. */
export const ORTHO_ICON = svg(
  <>
    <path d="M12 6h8v12h-8z" />
    <path className="view-3d-icon__thin" d="M2 8h8M2 12h8M2 16h8" />
  </>,
);

export const VECTORS_ICON = svg(
  <>
    <path d="M4 18C6 6 18 18 20 6" />
    <path className="view-3d-icon__thin" d="M3 17h2v2H3zM19 5h2v2h-2z" />
  </>,
);

export const TOOLPATHS_ICON = svg(<path d="M3 6h18v4H3v4h18v4H3" />);

export const MODEL_ICON = svg(
  <>
    <path d="M2 19h20" />
    <path d="M5 19c1-7 4-11 7-11s6 4 7 11" />
  </>,
);

export const BLOCK_ICON = svg(<path d="M3 11l5-4h13l-5 4zM3 11h13v6H3zM16 17l5-4V7" />);
