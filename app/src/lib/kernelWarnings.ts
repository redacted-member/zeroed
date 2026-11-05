// Words the warnings the kernel reports by code, from strings/kernelWarnings.
// A code this build has no sentence for, or one missing a value it needs,
// shows the English the kernel sent along with it. Start modules load this
// on first use (`import()`), so its sentences stay out of the start bundle.

import type { KernelWarning } from '../bridge/types';
import { kernelWarningsStrings } from './strings/kernelWarnings';

const text = kernelWarningsStrings.kernelWarnings;

/** A millimetre value as the kernel's English writes it. */
function mm(value: number): string {
  return text.length(value.toFixed(3));
}

/** The sentence for `warning` in the interface language. */
export function warningText(warning: KernelWarning): string {
  const p = warning.params;
  const num = (name: string): number | null => {
    const value = p[name];
    return typeof value === 'number' && Number.isFinite(value) ? value : null;
  };
  const str = (name: string): string | null => {
    const value = p[name];
    return typeof value === 'string' || typeof value === 'number' ? String(value) : null;
  };
  const worded = ((): string | null => {
    switch (warning.code) {
      case 'chamfer.multiPass': {
        const reach = num('reach');
        return reach === null ? null : text.chamferMultiPass(mm(reach));
      }
      case 'chamfer.passesLeftOut':
        return text.chamferPassesLeftOut;
      case 'prism.deeperThanCone': {
        const depth = num('coneDepth');
        return depth === null ? null : text.prismDeeperThanCone(mm(depth));
      }
      case 'prism.topFlat':
        return text.prismTopFlat;
      case 'thread.pitchDiffers': {
        const toolPitch = num('toolPitch');
        const pitch = num('pitch');
        return toolPitch === null || pitch === null
          ? null
          : text.threadPitchDiffers(mm(toolPitch), mm(pitch));
      }
      case 'profile.toolTooBig':
        return text.toolTooBig;
      case 'moulding.passLeftOut': {
        const offset = num('offset');
        return offset === null ? null : text.mouldingPassLeftOut(mm(offset));
      }
      case 'moulding.noRoom':
        return text.mouldingNoRoom;
      case 'toolpath.nothingLeft': {
        const tool = str('tool');
        return tool === null ? null : text.nothingLeft(tool);
      }
      case 'vgroove.noSmallerTool': {
        const tool = str('tool');
        return tool === null ? null : text.noSmallerTool(tool);
      }
      case 'toolpath.keepOut': {
        const zone = str('zone');
        return zone === null ? null : text.keepOut(zone);
      }
      case 'drill.openVectors':
        return text.drillOpenVectors;
      case 'texture.noStrokes':
        return text.textureNoStrokes;
      case 'fluting.rampShortened':
        return text.flutingRampShortened;
      case 'pocket.stepoverWide':
        return text.pocketStepoverWide;
      case 'toolpath.cutsThrough': {
        const depth = num('depth');
        const thickness = num('thickness');
        return depth === null || thickness === null
          ? null
          : text.cutsThrough(mm(depth), mm(thickness));
      }
      case 'fix.repaired': {
        const count = num('count');
        return count === null ? null : text.vectorsRepaired(count);
      }
      case 'fix.timedOut': {
        const seconds = num('seconds');
        return seconds === null ? null : text.fixingTimedOut(seconds);
      }
      case 'inlay.boundaryTooSmall':
        return text.inlayBoundaryTooSmall;
      case 'vgroove.flatsNeedPocket':
        return text.flatsNeedPocket;
      case 'toolpathImport.skipped':
        return str('word');
      case 'toolpathImport.skippedArcPlanes':
        return text.gcodeSkippedArcPlanes;
      case 'toolpathImport.skippedRotary':
        return text.gcodeSkippedRotary;
      case 'import.textSkipped':
        return text.importTextSkipped;
      case 'import.imageSkipped':
        return text.importImageSkipped;
      case 'import.useSkipped':
        return text.importUseSkipped;
      case 'import.blockMissing':
        return text.importBlockMissing;
      case 'import.blockTooDeep':
        return text.importBlockTooDeep;
      case 'import.fontSubstituted':
        return text.importFontSubstituted;
      default:
        return null;
    }
  })();
  return worded ?? warning.message;
}

/** The sentences for a reply's warnings, in order. */
export function warningTexts(warnings: readonly KernelWarning[]): string[] {
  return warnings.map(warningText);
}
