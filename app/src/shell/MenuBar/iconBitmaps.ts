// Renders menu icons (shell/ToolIcons) to the color PNG images the native
// menu bar draws, in the colors of the current light or dark appearance.
// Only the desktop build loads it, lazily.

import { createElement, Fragment } from 'react';
import { flushSync } from 'react-dom';
import { createRoot } from 'react-dom/client';

import { loadExtraIcons, toolIcon } from '../ToolIcons/ToolIcons';
import {
  ICON_PX,
  PALETTE_TOKENS,
  presentation,
  rootPresentation,
  type IconPalette,
} from './iconStyle';

function setAll(element: Element, attributes: Readonly<Record<string, string>>): void {
  for (const [name, value] of Object.entries(attributes)) {
    element.setAttribute(name, value);
  }
}

/** The icon colors the page shows now, read from its custom properties. */
function currentPalette(): IconPalette {
  const style = getComputedStyle(document.documentElement);
  return Object.fromEntries(
    PALETTE_TOKENS.map((token) => [token, style.getPropertyValue(`--${token}`).trim()]),
  );
}

async function rasterize(
  svg: SVGSVGElement,
  palette: IconPalette,
  canvas: HTMLCanvasElement,
  context: CanvasRenderingContext2D,
): Promise<number[]> {
  setAll(svg, rootPresentation(palette));
  svg.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  svg.setAttribute('width', String(ICON_PX));
  svg.setAttribute('height', String(ICON_PX));
  for (const shape of svg.querySelectorAll('[class]')) {
    setAll(shape, presentation(shape.getAttribute('class') ?? '', palette));
  }
  const image = new Image();
  image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(
    new XMLSerializer().serializeToString(svg),
  )}`;
  await image.decode();
  context.clearRect(0, 0, ICON_PX, ICON_PX);
  context.drawImage(image, 0, 0, ICON_PX, ICON_PX);
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'));
  return blob === null ? [] : Array.from(new Uint8Array(await blob.arrayBuffer()));
}

/** The PNG files, ICON_PX square, of the icons `ids`, by id. Unknown ids are left out. */
export async function iconImages(ids: readonly string[]): Promise<Record<string, number[]>> {
  await loadExtraIcons();
  const palette = currentPalette();
  const host = document.createElement('div');
  const root = createRoot(host);
  flushSync(() => {
    root.render(
      createElement(
        Fragment,
        null,
        ids.map((id) => createElement('span', { key: id, 'data-icon': id }, toolIcon(id))),
      ),
    );
  });
  const canvas = document.createElement('canvas');
  canvas.width = ICON_PX;
  canvas.height = ICON_PX;
  const context = canvas.getContext('2d');
  const images: Record<string, number[]> = {};
  if (context !== null) {
    for (const span of host.querySelectorAll<HTMLElement>('[data-icon]')) {
      const id = span.dataset['icon'];
      const svg = span.querySelector('svg');
      if (id !== undefined && svg !== null) {
        const png = await rasterize(svg, palette, canvas, context);
        if (png.length > 0) {
          images[id] = png;
        }
      }
    }
  }
  root.unmount();
  return images;
}
