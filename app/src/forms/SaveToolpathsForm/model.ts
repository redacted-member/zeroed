// Pure rules of the Save Toolpaths form: which toolpaths an output option
// saves, which posts a machine offers and what stops a file holding more
// than one tool.

import type { PostSummary, ToolpathSummary, TrialCut } from '../../bridge/cam';
import type { MachinePrefs, RotaryWiring } from '../../bridge/prefs';
import type { KernelClient, RotaryAxis, Units } from '../../bridge/types';
import { lengthValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { formatDisplay } from '../../lib/units';
import { activeSavePost } from '../../panels/MachineConfiguration/model';

/** The three output options, as the kernel names them. */
export type SaveOutput = 'selected' | 'oneFile' | 'multipleFiles';

/** The output choice the form opens with: Selected Toolpath when it was
 * opened for one toolpath, otherwise the choice it last saved with. */
export function initialOutput(choice: readonly number[] | null, last: SaveOutput): SaveOutput {
  return choice !== null && choice.length === 1 ? 'selected' : last;
}

/** The driver Output direct to machine sends to, or null when the post
 * names none or the host cannot start programs (the browser). */
export function canOutputDirect(
  post: PostSummary | null,
  client: Pick<KernelClient, 'directOutputPath' | 'sendToDriver'>,
): string | null {
  const driver = post?.directOutput?.trim() ?? '';
  if (driver === '' || client.directOutputPath === undefined || client.sendToDriver === undefined) {
    return null;
  }
  return driver;
}

const text = strings.toolpaths.save;

/** The toolpaths an option saves, in cutting order. */
export function savedToolpaths(
  output: SaveOutput,
  toolpaths: readonly ToolpathSummary[],
  selectedId: number | null,
): ToolpathSummary[] {
  if (output === 'selected') {
    return toolpaths.filter((t) => t.id === selectedId);
  }
  return toolpaths.filter((t) => t.visible);
}

/** The posts the machine is set up with, or every post when it has none. */
export function postsForMachine(
  posts: readonly PostSummary[],
  machines: MachinePrefs,
  machine: string,
): PostSummary[] {
  const config = machines.list.find((m) => m.name === machine);
  const offered =
    config === undefined || config.posts.length === 0
      ? [...posts]
      : posts.filter((p) => config.posts.includes(p.id));
  // By name, so a post is found where a reader expects it.
  return offered.sort((a, b) => a.name.localeCompare(b.name));
}

/** The generic G code post in a job's unit, the one to start from. */
export function genericPost(units: Units): string {
  return units === 'inch' ? 'generic_gcode_inch' : 'generic_gcode_mm';
}

/**
 * The post the form picks first for a machine: its default, else the
 * generic G code post in the job's unit, else its first by name. Laser
 * toolpaths take a post that drives a laser: the default when it does,
 * else the machine's first laser post in the job's unit, else any of its
 * laser posts; with none the default stands and the form says why it
 * cannot save.
 */
export function firstPost(
  posts: readonly PostSummary[],
  machines: MachinePrefs,
  machine: string,
  units: Units = 'mm',
  laser = false,
  /** The way a rotary job's blank lies, null for a flat job. */
  rotaryAxis: RotaryAxis | null = null,
): string {
  const offered = postsForMachine(posts, machines, machine);
  const saved = activeSavePost({ ...machines, active: machine });
  const preferred = offered.find((p) => p.id === saved);
  const generic = offered.find((p) => p.id === genericPost(units));
  if (rotaryAxis !== null) {
    // A rotary job takes a post that wraps it: the machine's default when it
    // does, else one for the same controller, in the job's unit first.
    const wrapping = offered.filter((p) => wrapLetter(p, rotaryAxis) !== null);
    const family = saved?.split('_')[0] ?? '';
    const kin = wrapping.filter((p) => family !== '' && p.id.startsWith(`${family}_`));
    const wrap =
      wrapping.find((p) => p.id === saved) ??
      kin.find((p) => p.units === units) ??
      kin[0] ??
      wrapping.find((p) => p.units === units) ??
      wrapping[0];
    if (wrap !== undefined) {
      return wrap.id;
    }
  }
  if (laser) {
    const lasers = offered.filter((p) => p.jet === true);
    const beam =
      lasers.find((p) => p.id === saved) ?? lasers.find((p) => p.units === units) ?? lasers[0];
    if (beam !== undefined) {
      return beam.id;
    }
  }
  return (preferred ?? generic ?? offered[0])?.id ?? '';
}

/**
 * Why the toolpaths cannot go in one file, or null: more than one tool needs
 * a post that changes tools, and each cutter needs its own tool number.
 */
export function toolChangeProblem(
  output: SaveOutput,
  saved: readonly ToolpathSummary[],
  post: PostSummary | null,
): string | null {
  if (output === 'multipleFiles' || post === null) {
    return null;
  }
  const byNumber = new Map<number, string>();
  for (const toolpath of saved) {
    const other = byNumber.get(toolpath.toolNumber);
    if (other !== undefined && other !== toolpath.toolName) {
      return text.numberClash(toolpath.toolNumber, other, toolpath.toolName);
    }
    byNumber.set(toolpath.toolNumber, toolpath.toolName);
  }
  if (byNumber.size > 1 && post.toolChange !== true) {
    return text.noToolChange(post.name);
  }
  return null;
}

/**
 * Why the post cannot write these toolpaths, or null: laser toolpaths need
 * a post that drives a laser. The machine's capabilities are the posts it
 * is set up with.
 */
export function capabilityProblem(
  saved: readonly ToolpathSummary[],
  post: PostSummary | null,
): string | null {
  if (post !== null && saved.some((t) => t.kind === 'laser') && post.jet !== true) {
    return text.noLaser(post.name);
  }
  if (post !== null && saved.some((t) => t.strategy === 'drillBank') && post.drillBank !== true) {
    return strings.toolpaths.drillBank.noBankPost(post.name);
  }
  return null;
}

/** The letter `post` writes a blank along `axis` on, or null when it cannot. */
export function wrapLetter(post: PostSummary | null, axis: RotaryAxis): string | null {
  return (axis === 'x' ? post?.rotaryWrapY : post?.rotaryWrapX) ?? null;
}

/** A warning that does not stop saving: a rotary job through a post that
 * cannot wrap this blank's way comes out flat. `axis` is the way the job's
 * blank lies, null for a flat job. */
export function rotaryWarning(post: PostSummary | null, axis: RotaryAxis | null): string | null {
  if (post === null || axis === null || wrapLetter(post, axis) !== null) {
    return null;
  }
  return text.noRotary(post.name, axis.toUpperCase());
}

/** What a rotary job's file will turn on this machine, for the line under the
 * post: null for a flat job or a post that cannot wrap it. */
export function rotaryNote(
  post: PostSummary | null,
  axis: RotaryAxis | null,
  wiring: Required<RotaryWiring>,
): string | null {
  if (axis === null || wrapLetter(post, axis) === null) {
    return null;
  }
  return text.wrapsOnto(wiring.letter ?? 'A', wiring.units === 'linear', wiring.reversed);
}

/** The name the save dialog suggests. */
export function suggestedName(
  output: SaveOutput,
  saved: readonly ToolpathSummary[],
  extension: string,
  /** Add side to toolpath name: the side of a double sided job, such as "Top". */
  side: string | null = null,
): string {
  const first = saved[0];
  const base =
    output === 'selected' && first !== undefined
      ? first.name
      : output === 'multipleFiles'
        ? text.multiplePrefix
        : (first?.name ?? text.multiplePrefix);
  return side === null ? `${base}.${extension}` : `${base} (${side}).${extension}`;
}

/** The trial run choice: the real program, an air cut or a scrap cut. */
export type TrialChoice = 'none' | 'air' | 'scrap';

/** The height an air cut starts with (25 mm, 1 in) and the depth a scrap
 * cut starts with (0.5 mm, 0.02 in), as typed in the job's units. */
export function trialDefault(choice: TrialChoice, units: Units): string {
  if (choice === 'none') {
    return '';
  }
  const inch = units === 'inch';
  return formatDisplay(choice === 'air' ? (inch ? 1 : 25) : inch ? 0.02 : 0.5, units);
}

/** The trial the kernel posts, in millimetres, or an error for a height or
 * depth that is not a positive length. */
export function trialCut(
  choice: TrialChoice,
  typed: string,
  units: Units,
): { trial: TrialCut | null; error: string | null } {
  if (choice === 'none') {
    return { trial: null, error: null };
  }
  const mm = lengthValue(typed, units);
  if (mm === null || !(mm > 0)) {
    return { trial: null, error: choice === 'air' ? text.airError : text.scrapError };
  }
  return {
    trial: choice === 'air' ? { kind: 'air', height: mm } : { kind: 'scrap', depth: mm },
    error: null,
  };
}

/** The file name with the trial's label before its extension, so the trial
 * file is never taken for the real one. */
export function trialName(name: string, choice: TrialChoice): string {
  if (choice === 'none') {
    return name;
  }
  const suffix = choice === 'air' ? '_AIR_CUT' : '_SCRAP_CUT';
  const dot = name.lastIndexOf('.');
  return dot <= 0 ? `${name}${suffix}` : `${name.slice(0, dot)}${suffix}${name.slice(dot)}`;
}
