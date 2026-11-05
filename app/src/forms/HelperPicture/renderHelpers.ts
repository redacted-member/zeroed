import { createElement, type ComponentType } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';

import { HelperPicture } from './HelperPicture';
import type { Picture } from './kit';

/** The data-helper ids a rendered component shows, in order (tests). */
export function renderedHelpers(Component: ComponentType): string[] {
  const html = renderToStaticMarkup(createElement(Component));
  return [...html.matchAll(/data-helper="([^"]+)"/g)].map((match) => match[1] ?? '');
}

/** The markup of one picture (tests). */
export function pictureMarkup(picture: Picture): string {
  return renderToStaticMarkup(createElement(HelperPicture, { picture }));
}

/** Ids and descriptions that differ for every picture given (tests). */
export function distinct(pictures: Picture[]): { ids: number; alts: number; count: number } {
  return {
    ids: new Set(pictures.map((picture) => picture.id)).size,
    alts: new Set(pictures.map((picture) => picture.alt)).size,
    count: pictures.length,
  };
}
