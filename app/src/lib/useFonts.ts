// The font families the kernel can lay text out with, fetched once per
// session and shared by every text form.

import { useEffect, useState } from 'react';

import { api } from '../bridge/formApi';

/** Every family, and the single line (engraving) ones among them. */
export interface FontList {
  families: string[];
  singleLine: string[];
}

const EMPTY: FontList = { families: [], singleLine: [] };

let fonts: Promise<FontList> | null = null;

function loadFonts(): Promise<FontList> {
  if (fonts === null) {
    fonts = api.text
      .fonts()
      .then((reply) => ({ families: reply.families, singleLine: reply.singleLine ?? [] }))
      .catch(() => {
        fonts = null;
        return EMPTY;
      });
  }
  return fonts;
}

/** The font families, or null until they have arrived. */
export function useFonts(): FontList | null {
  const [list, setList] = useState<FontList | null>(null);
  useEffect(() => {
    let live = true;
    void loadFonts().then((loaded) => {
      if (live) {
        setList(loaded);
      }
    });
    return () => {
      live = false;
    };
  }, []);
  return list;
}
