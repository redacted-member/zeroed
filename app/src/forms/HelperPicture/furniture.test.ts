import { createElement } from 'react';
import { describe, expect, it } from 'vitest';

import type { FurnitureParam } from '../../bridge/furniture';
import { AssemblyExplode } from '../AssemblyExplode/AssemblyExplode';
import { AssemblyFit } from '../AssemblyFit/AssemblyFit';
import { FurnitureParams, groupPicture } from '../FurnitureParams/FurnitureParams';
import {
  assemblyExplode,
  assemblyFit,
  furnitureSize,
  furnitureThickness,
  jointFit,
} from './pictures/furniture';
import { distinct, pictureMarkup, renderedHelpers } from './renderHelpers';

const param = (key: string, kind: FurnitureParam['kind'], group: string): FurnitureParam => ({
  key,
  label: key,
  kind,
  default: 18,
  min: 1,
  max: 100,
  group,
});

const PARAMS: FurnitureParam[] = [
  param('width', 'length', 'Size'),
  param('height', 'length', 'Size'),
  param('t', 'thickness', 'Material'),
  param('shelves', 'count', 'Layout'),
];

describe('furniture, joint and assembly helpers', () => {
  it('the designer shows the overall size and the stock thickness beside their groups', () => {
    const Params = () =>
      createElement(FurnitureParams, {
        params: PARAMS,
        fields: {},
        errors: {},
        units: 'mm',
        onChange: () => undefined,
        onRelease: () => undefined,
      });
    expect(renderedHelpers(Params)).toEqual(['furniture-size', 'furniture-thickness']);
    expect(groupPicture([param('shelves', 'count', 'Layout')])).toBeNull();
    // A thickness in a group of sizes shows the stock: the board end on.
    expect(groupPicture([param('width', 'length', ''), param('t', 'thickness', '')])).toBe(
      furnitureThickness,
    );
  });

  it('the Assembly View shows Explode and Verify Fit pictures', () => {
    expect(renderedHelpers(AssemblyExplode)).toEqual(['assembly-explode']);
    expect(renderedHelpers(AssemblyFit)).toEqual(['assembly-fit']);
  });

  it('every picture is its own, with its own description', () => {
    const all = [furnitureSize, furnitureThickness, jointFit, assemblyExplode, assemblyFit];
    expect(distinct(all)).toEqual({ ids: 5, alts: 5, count: 5 });
    expect(pictureMarkup(jointFit)).toContain('data-helper="joint-fit"');
  });
});
