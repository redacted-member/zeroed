import { describe, expect, it } from 'vitest';

import {
  IMPORT_VECTOR_EXTENSIONS,
  TEMPLATE_EXTENSION,
  VECTOR_EXTENSIONS,
  exportFormat,
  fileName,
  openKind,
  withExtension,
} from './fileCommands';

describe('file commands', () => {
  it('adds the extension only when the path does not have an accepted one', () => {
    expect(withExtension('/jobs/sign', 'carve')).toBe('/jobs/sign.carve');
    expect(withExtension('/jobs/sign.CARVE', 'carve')).toBe('/jobs/sign.CARVE');
    expect(withExtension('/out/part.dxf', 'svg', ['svg', 'dxf'])).toBe('/out/part.dxf');
    expect(withExtension('/out/part', 'svg', ['svg', 'dxf'])).toBe('/out/part.svg');
  });

  it('exports DXF only when asked for by extension', () => {
    expect(exportFormat('/out/part.DXF')).toBe('dxf');
    expect(exportFormat('/out/part.svg')).toBe('svg');
  });

  it('picks EPS, Illustrator and PDF by extension and keeps their extensions', () => {
    expect(exportFormat('/out/part.eps')).toBe('eps');
    expect(exportFormat('/out/part.AI')).toBe('ai');
    expect(exportFormat('/out/part.pdf')).toBe('pdf');
    expect(exportFormat('/out/part.png')).toBe('svg');
    expect(withExtension('/out/part.pdf', 'svg', VECTOR_EXTENSIONS)).toBe('/out/part.pdf');
    expect(withExtension('/out/part.ai', 'svg', VECTOR_EXTENSIONS)).toBe('/out/part.ai');
    expect(withExtension('/out/part', 'svg', VECTOR_EXTENSIONS)).toBe('/out/part.svg');
  });

  it('reads DWG and GIF or TIFF bitmaps but never writes DWG', () => {
    expect(IMPORT_VECTOR_EXTENSIONS).toContain('dwg');
    expect(VECTOR_EXTENSIONS).not.toContain('dwg');
    expect(openKind('/cad/plan.DWG')).toBe('vectors');
    expect(openKind('/scans/logo.tif')).toBe('bitmap');
    expect(openKind('/scans/logo.gif')).toBe('bitmap');
    expect(exportFormat('/out/part.dwg')).toBe('svg');
    expect(openKind('/rhino/vase.3dm')).toBe('mesh');
  });

  it('names untitled jobs and strips folders', () => {
    expect(fileName(null)).toBe('Untitled');
    expect(fileName('/Users/me/jobs/sign.carve')).toBe('sign.carve');
    expect(fileName('C:\\jobs\\sign.carve')).toBe('sign.carve');
  });

  it('gives toolpath templates their own extension', () => {
    expect(withExtension('/out/signs', TEMPLATE_EXTENSION)).toBe('/out/signs.cstemplate');
    expect(withExtension('/out/signs.cstemplate', TEMPLATE_EXTENSION)).toBe(
      '/out/signs.cstemplate',
    );
  });
});
