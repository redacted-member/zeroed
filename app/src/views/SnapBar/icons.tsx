// Line icons for the 2D view toolbar, on a 24 by 24 grid with currentColor
// strokes so they follow the theme.

import type { JSX, ReactNode } from 'react';

function svg(children: ReactNode): JSX.Element {
  return (
    <svg aria-hidden="true" className="snap-bar__icon" focusable="false" viewBox="0 0 24 24">
      {children}
    </svg>
  );
}

export const ZOOM_BOX_ICON = svg(
  <>
    <circle cx="10" cy="10" r="6" />
    <path d="M14.5 14.5L20 20M7.5 10h5M10 7.5v5" />
  </>,
);

export const PAN_ICON = svg(
  <path d="M12 3v18M3 12h18M12 3l-3 3M12 3l3 3M12 21l-3-3M12 21l3-3M3 12l3-3M3 12l3 3M21 12l-3-3M21 12l-3 3" />,
);

export const ZOOM_JOB_ICON = svg(
  <>
    <path d="M3 8V3h5M16 3h5v5M21 16v5h-5M8 21H3v-5" />
    <path className="snap-bar__thin" d="M7 8h10v8H7z" />
  </>,
);

export const ZOOM_SELECTION_ICON = svg(
  <>
    <path d="M3 8V3h5M16 3h5v5M21 16v5h-5M8 21H3v-5" />
    <path className="snap-bar__dash" d="M8 9h8v6H8z" />
  </>,
);

export const SNAP_OBJECTS_ICON = svg(
  <>
    <path className="snap-bar__thin" d="M3 19L19 5" />
    <path d="M9 11h4v4H9z" />
    <path d="M17 3l2 2M5 21l-2-2" />
  </>,
);

export const SNAP_GRID_ICON = svg(
  <>
    <path className="snap-bar__thin" d="M4 4h16v16H4zM4 12h16M12 4v16" />
    <circle className="snap-bar__fill" cx="12" cy="12" r="2.2" />
  </>,
);

export const SNAP_GUIDES_ICON = svg(
  <>
    <path className="snap-bar__dash" d="M8 3v18M3 15h18" />
    <circle className="snap-bar__fill" cx="8" cy="15" r="2.2" />
  </>,
);

export const SMART_SNAP_ICON = svg(
  <>
    <path d="M5 5h5v5H5z" />
    <path className="snap-bar__dash" d="M10 7.5h10M7.5 10v10" />
    <circle className="snap-bar__fill" cx="18" cy="7.5" r="1.8" />
  </>,
);

export const SNAP_SETTINGS_ICON = svg(
  <>
    <path d="M4 7h9M17 7h3M4 17h3M11 17h9" />
    <circle cx="15" cy="7" r="2" />
    <circle cx="9" cy="17" r="2" />
  </>,
);

export const GRID_ICON = svg(
  <path className="snap-bar__thin" d="M4 4h16v16H4zM4 9.3h16M4 14.6h16M9.3 4v16M14.6 4v16" />,
);

export const RULERS_ICON = svg(
  <>
    <path d="M3 3h18v5H8v13H3z" />
    <path className="snap-bar__thin" d="M11 3v3M15 3v3M19 3v3M3 11h3M3 15h3M3 19h3" />
  </>,
);

export const CLEAR_GUIDES_ICON = svg(
  <>
    <path className="snap-bar__dash" d="M9 3v18M3 9h18" />
    <path d="M14 14l6 6M20 14l-6 6" />
  </>,
);

export const COMPONENT_PREVIEW_ICON = svg(
  <>
    <path d="M3 18c3-9 6-12 9-12s6 3 9 12z" />
    <path className="snap-bar__thin" d="M8 13c2-2 4-3 6-2" />
  </>,
);

export const BITMAP_ICON = svg(
  <>
    <rect height="14" width="18" x="3" y="5" />
    <path className="snap-bar__thin" d="M3 16l5-5 4 4 3-3 6 6" />
    <circle className="snap-bar__thin" cx="16" cy="9" r="1.5" />
  </>,
);

export const TOOLPATH_LINES_ICON = svg(<path d="M4 6h16v4H6v4h14v4H4" />);

export const TOOLPATH_SOLID_ICON = svg(
  <>
    <path className="snap-bar__fill" d="M4 5h16v5H9v1h11v8H4v-5h11v-1H4z" />
  </>,
);

export const ZOOM_ALL_ICON = svg(
  <>
    <circle cx="10" cy="10" r="6" />
    <path d="M14.5 14.5L20 20" />
    <path className="snap-bar__thin" d="M7 8V7h1M12 7h1v1M13 12v1h-1M8 13H7v-1" />
  </>,
);
