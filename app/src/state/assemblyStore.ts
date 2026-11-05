// The Assembly View while its form is open: the pieces (one per part each
// sheet's toolpaths cut free, a sheet's raw model, or a component added as
// a piece of its own) and their reliefs, the components that can be
// added, the piece
// picked in the 3D view, a placement being dragged (shown before the kernel
// has it, and with the mates solved live, one solve at a time), snapping, a face to face mate being picked, the clearance check
// and the cut list. While a mate waits for a face, the face under the
// pointer is lit. The mates kept with the document come with the pieces,
// each with how it stands after the last solve. The form opens and closes
// it; while open it follows the document revision, so undo and redo move
// pieces back. A component being placed (dragged from the Component Tree,
// or after Place in the add list) shows as a ghost where it would land.

import { create } from 'zustand';

import { api } from '../bridge/formApi';
import {
  pieceKey,
  type AddableComponent,
  type ComponentPlace,
  type ComponentRelief,
  type DropLanding,
  type PiecePose,
  type ExplodeStep,
  type MateEdit,
  type MateKind,
  type MateRow,
  type PieceAction,
  type PieceCutRow,
  type PieceFace,
  type PieceFaceHit,
  type PieceIssue,
  type PieceRow,
  type Triple,
} from '../bridge/assembly';
import type { Heightfield, ToolpathSummary } from '../bridge/cam';
import { restingPosition } from '../views/View3D/pieceDrop';
import { furnitureStrings } from '../lib/strings/furniture';
import { faceTargetOf } from './furnitureFaces';
import { useDocumentStore } from './documentStore';
import { useFurnitureStore } from './furnitureStore';
import { useToolpathStore } from './toolpathStore';

/** A piece's place: where the middle of its box sits and how it is turned. */
export interface Place {
  position: Triple;
  rotation: Triple;
}

/**
 * A placement shown while a handle is dragged, with where the mates put
 * every piece (the dragged one too) for the last place the kernel solved.
 */
export interface LivePlacement {
  key: string;
  position: Triple;
  rotation: Triple;
  solved?: Readonly<Record<string, Place>>;
}

/** Which face a mate waits for: the one that stays, then the one that moves. */
export type MateStep = 'target' | 'moving' | null;

/** A mate being made: its kind, the picks so far and how the faces meet. */
export interface MateState {
  kind: MateKind;
  step: MateStep;
  target: PieceFace | null;
  moving: PieceFace | null;
  /** Gap between the faces, mm. */
  offset: number;
  /** Turn the moving piece over: its side opposite the picked face meets the target. */
  flip: boolean;
  /** Turn about the face normal, degrees; null lets the kernel pick. */
  turn: number | null;
  /** The mate the picks made, which Apply changes; null before one is made. */
  last: number | null;
}

export const NO_MATE: MateState = {
  kind: 'against',
  step: null,
  target: null,
  moving: null,
  offset: 0,
  flip: false,
  turn: null,
  last: null,
};

/**
 * A component being placed: dragged from the Component Tree (`dragging`)
 * or waiting for a click in the 3D view after Place; `at` is the landing
 * under the pointer, null while it is off the view; `pose` is where the
 * kernel last said it lands (null until it answers).
 */
export interface Placing {
  component: number;
  dragging: boolean;
  at: DropLanding | null;
  pose: PiecePose | null;
}

/** The ghost of a component being placed: its relief where it would land. */
export interface PlacingGhost {
  field: Heightfield;
  position: Triple;
  rotation: Triple;
  size: Triple;
}

/** The face under the pointer while a mate waits for one. */
export interface HoveredFace {
  key: string;
  face: PieceFaceHit;
}

interface AssemblyState {
  active: boolean;
  pieces: PieceRow[];
  /** Components not yet pieces of their own, which can be added. */
  components: AddableComponent[];
  /** Simulation relief per piece key. */
  reliefs: Readonly<Record<string, Heightfield>>;
  loading: boolean;
  selected: string | null;
  /** The piece shown alone (a view state: the saved hidden flags stay as they are). */
  isolated: string | null;
  /** The placement a drag shows; the other pieces show ghosted meanwhile. */
  live: LivePlacement | null;
  /** Whether a dragged piece snaps against the pieces near it. */
  snap: boolean;
  mate: MateState;
  /** The mates kept with the document, as they stand. */
  mates: MateRow[];
  /** The mate picked in the list. */
  selectedMate: number | null;
  /** The named explode steps, in the order they play. */
  explodeSteps: ExplodeStep[];
  /** The face under the pointer while a mate waits for a face. */
  hover: HoveredFace | null;
  /** A component being placed; its ghost follows the pointer. */
  placing: Placing | null;
  /** Reliefs of components that are not pieces yet, for their ghosts. */
  ghosts: Readonly<Record<number, ComponentRelief>>;
  /** Smallest wanted gap between pieces, mm. */
  clearance: number;
  issues: PieceIssue[] | null;
  checking: boolean;
  cutList: PieceCutRow[] | null;
  error: string | null;
  notice: string | null;

  open: () => Promise<void>;
  close: () => void;
  refresh: () => Promise<void>;
  select: (key: string | null) => void;
  /** Shows only this piece (null: every piece not hidden again); not saved, no undo step. */
  isolate: (key: string | null) => void;
  setLive: (live: LivePlacement | null) => void;
  place: (key: string, position: Triple, rotation: Triple) => Promise<boolean>;
  setSnap: (snap: boolean) => void;
  startMate: () => void;
  cancelMate: () => void;
  /** A face picked in the view while a mate waits for one. */
  pickFace: (face: PieceFace) => Promise<void>;
  setMateOptions: (options: Partial<Pick<MateState, 'kind' | 'offset' | 'flip' | 'turn'>>) => void;
  /** The pointer is over this point of a piece (null: over none) while a mate waits. */
  hoverFace: (at: PieceFace | null) => void;
  /** Makes a sheet's piece its raw model (true) or its simulation's parts. */
  setSource: (sheet: number, model: boolean) => Promise<void>;
  /** Adds components as pieces of their own (one undo step) and picks the first. */
  addComponents: (ids: number[], place?: ComponentPlace) => Promise<void>;
  /** Starts placing a component: its ghost follows the pointer until it lands or Esc. */
  startPlacing: (component: number, dragging: boolean) => void;
  /**
   * The landing under the pointer while placing (null: off the view); the
   * kernel is asked where it lands, one request at a time.
   */
  movePlacing: (at: DropLanding | null) => void;
  cancelPlacing: () => void;
  /**
   * Lands a component where it is let go (one undo step): flush against
   * the face under the pointer, clear of every piece, as a new piece or
   * moving the piece it is already.
   */
  dropComponent: (component: number, at: DropLanding) => Promise<void>;
  /** Takes a component piece out of the view (one undo step). */
  removeComponent: (id: number) => Promise<void>;
  /** Makes the picked mate, or changes the one the picks made. */
  applyMate: () => Promise<boolean>;
  selectMate: (id: number | null) => void;
  /** Changes a mate and solves again (one undo step). */
  editMate: (id: number, edit: MateEdit) => Promise<boolean>;
  /** Deletes a mate (one undo step). */
  deleteMate: (id: number) => Promise<void>;
  /** Keeps the explode steps (one undo step named `label`); answers the kept steps, or null. */
  setExplodeSteps: (steps: ExplodeStep[], label: string) => Promise<ExplodeStep[] | null>;
  /** Hides, shows or grounds a piece; Show All takes no piece and ends Isolate (one undo step). */
  setPieceState: (action: PieceAction, key: string | null) => Promise<void>;
  setClearance: (mm: number) => void;
  check: () => Promise<void>;
  loadCutList: () => Promise<void>;
  setNotice: (notice: string | null) => void;
  setError: (error: string | null) => void;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Whether the view shows `piece`: the isolated one alone, else every piece not hidden. */
export function isShown(piece: PieceRow, isolated: string | null): boolean {
  return isolated === null ? piece.hidden !== true : pieceKey(piece) === isolated;
}

/**
 * The pieces with a live placement applied: each where the last solve of
 * the drag put it, else the dragged one where the pointer has it.
 */
export function shownPieces(pieces: readonly PieceRow[], live: LivePlacement | null): PieceRow[] {
  if (live === null) {
    return [...pieces];
  }
  return pieces.map((piece) => {
    const key = pieceKey(piece);
    const solved = live.solved?.[key];
    if (solved !== undefined) {
      return { ...piece, position: solved.position, rotation: solved.rotation };
    }
    return live.key === key
      ? { ...piece, position: live.position, rotation: live.rotation }
      : piece;
  });
}

/** Whether a drag needs solving live: some mate is switched on. */
export function solvesLive(mates: readonly MateRow[]): boolean {
  return mates.some((m) => !m.suppressed);
}

/**
 * Pieces whose relief must be (re)loaded: every one when `all`, else the
 * ones missing and the ones whose part box changed size since `before`.
 */
export function reliefsToLoad(
  pieces: readonly PieceRow[],
  before: readonly PieceRow[],
  reliefs: Readonly<Record<string, Heightfield>>,
  all: boolean,
): string[] {
  return pieces
    .filter((piece) => {
      const key = pieceKey(piece);
      const old = before.find((p) => pieceKey(p) === key);
      return (
        all ||
        reliefs[key] === undefined ||
        old === undefined ||
        old.size.some((v, i) => v !== piece.size[i])
      );
    })
    .map(pieceKey);
}

/** The key of the piece a component is, when it is one. */
export function componentPieceKey(component: number): string {
  return pieceKey({ sheet: 0, part: 1, component });
}

/**
 * Where the ghost of a component being placed shows: where the kernel
 * said it lands, or until it answers resting on the point (a component
 * that is a piece already turned as it is, a new one unturned). A piece
 * shows its own relief, a new one the relief it would have. Null while off
 * the view or not loaded.
 */
export function placingGhost(
  placing: Placing | null,
  pieces: readonly PieceRow[],
  reliefs: Readonly<Record<string, Heightfield>>,
  ghosts: Readonly<Record<number, ComponentRelief>>,
): PlacingGhost | null {
  if (placing === null || placing.at === null) {
    return null;
  }
  const key = componentPieceKey(placing.component);
  const piece = pieces.find((p) => pieceKey(p) === key);
  if (piece !== undefined) {
    const field = reliefs[key];
    return field === undefined
      ? null
      : {
          field,
          position: placing.pose?.position ?? restingPosition(piece, placing.at.point),
          rotation: placing.pose?.rotation ?? piece.rotation,
          size: piece.size,
        };
  }
  const ghost = ghosts[placing.component];
  if (ghost === undefined) {
    return null;
  }
  const unturned: Triple = [0, 0, 0];
  return {
    field: ghost,
    position:
      placing.pose?.position ??
      restingPosition(
        { position: [0, 0, 0], rotation: unturned, size: ghost.size },
        placing.at.point,
      ),
    rotation: placing.pose?.rotation ?? unturned,
    size: ghost.size,
  };
}

/** Which components are pieces of their own, in order. */
export function componentPiecesKey(pieces: readonly PieceRow[]): string {
  return pieces
    .flatMap((p) => (p.component === undefined || p.component === null ? [] : [p.component]))
    .join(',');
}

/** What in the toolpath list changes what the sheets' simulations cut. */
export function toolpathsKey(toolpaths: readonly ToolpathSummary[]): string {
  return toolpaths
    .map((t) =>
      [t.id, t.visible, t.stale, t.sheet ?? 0, t.side ?? '', t.moves, t.estimatedSeconds].join(':'),
    )
    .join(',');
}

/**
 * A mate after a face is picked: the first face is the one that stays, the
 * second (on another piece) the one that moves to meet it. `ready` says the
 * mate has both faces and should be applied.
 */
export function withFace(mate: MateState, face: PieceFace): { mate: MateState; ready: boolean } {
  if (mate.step === 'target') {
    return { mate: { ...mate, step: 'moving', target: face, moving: null }, ready: false };
  }
  if (mate.step === 'moving' && mate.target !== null) {
    if (pieceKey(mate.target) === pieceKey(face)) {
      return { mate, ready: false };
    }
    return { mate: { ...mate, step: null, moving: face, turn: null, last: null }, ready: true };
  }
  return { mate, ready: false };
}

export const useAssemblyStore = create<AssemblyState>((set, get) => {
  /** Reloads the piece list and, when `allReliefs`, every relief. */
  let loads = 0;
  const load = async (allReliefs: boolean): Promise<void> => {
    const ticket = ++loads;
    set({ loading: true });
    try {
      const {
        pieces,
        components = [],
        mates = [],
        explodeSteps = [],
      } = await api.assembly.pieces();
      if (ticket !== loads || !get().active) {
        return;
      }
      // A component joining or leaving the pieces changes its sheet's raw model.
      const all = allReliefs || componentPiecesKey(pieces) !== componentPiecesKey(get().pieces);
      const keys = reliefsToLoad(pieces, get().pieces, get().reliefs, all);
      const reliefs: Record<string, Heightfield> = {};
      for (const piece of pieces) {
        const key = pieceKey(piece);
        const kept = get().reliefs[key];
        if (!keys.includes(key) && kept !== undefined) {
          reliefs[key] = kept;
        }
      }
      // The list first, so the form shows the pieces while they load. A
      // ghost of a component whose shape changed is stale.
      const selectedMate = get().selectedMate;
      const before = new Map(get().components.map((c) => [c.id, c.look]));
      const ghosts = Object.fromEntries(
        Object.entries(get().ghosts).filter(([id]) => {
          const now = components.find((c) => c.id === Number(id));
          return now === undefined || now.look === before.get(now.id);
        }),
      );
      set({
        ghosts,
        pieces,
        components,
        mates,
        explodeSteps,
        reliefs: { ...reliefs },
        selectedMate: mates.some((m) => m.id === selectedMate) ? selectedMate : null,
      });
      for (const piece of pieces) {
        const key = pieceKey(piece);
        if (!keys.includes(key)) {
          continue;
        }
        const relief = await api.assembly.pieceRelief(piece);
        if (ticket !== loads || !get().active) {
          return;
        }
        reliefs[key] = relief;
        set({ reliefs: { ...reliefs } });
      }
      const selected = get().selected;
      if (selected !== null && !pieces.some((p) => pieceKey(p) === selected)) {
        set({ selected: null });
      }
      const isolated = get().isolated;
      if (isolated !== null && !pieces.some((p) => pieceKey(p) === isolated)) {
        set({ isolated: null });
      }
      set({ error: null });
    } catch (error) {
      if (ticket === loads) {
        set({ error: message(error) });
      }
    } finally {
      if (ticket === loads) {
        set({ loading: false });
      }
    }
  };

  const refFor = (key: string) => get().pieces.find((p) => pieceKey(p) === key);

  // One live solve at a time while a piece is dragged; the newest place
  // waits for the one running.
  let solving = false;
  let nextSolve: LivePlacement | null = null;
  const solveLive = async (live: LivePlacement): Promise<void> => {
    const piece = refFor(live.key);
    if (piece === undefined) {
      return;
    }
    solving = true;
    try {
      const reply = await api.assembly.solvePlace(piece, live.position, live.rotation);
      const now = get().live;
      if (now !== null && now.key === live.key && get().active) {
        const solved: Record<string, Place> = {};
        for (const place of reply.places) {
          solved[pieceKey(place.piece)] = { position: place.position, rotation: place.rotation };
        }
        set({ live: { ...now, solved } });
      }
    } catch {
      // A place the kernel refuses shows unsolved; the release reports it.
    } finally {
      solving = false;
      const queued = nextSolve;
      nextSolve = null;
      if (queued !== null && get().live?.key === queued.key) {
        void solveLive(queued);
      }
    }
  };

  // One landing query at a time while placing; the newest waits.
  let landing = false;
  let nextLanding: { component: number; at: DropLanding } | null = null;
  const askLanding = async (component: number, at: DropLanding): Promise<void> => {
    landing = true;
    try {
      const { position, rotation } = await api.assembly.dropPlace(component, at);
      const now = get().placing;
      if (get().active && now !== null && now.component === component && now.at !== null) {
        set({ placing: { ...now, pose: { position, rotation } } });
      }
    } catch {
      // It shows resting on the point; letting go reports what the kernel says.
    } finally {
      landing = false;
      const queued = nextLanding;
      nextLanding = null;
      if (queued !== null && get().placing?.component === queued.component) {
        void askLanding(queued.component, queued.at);
      }
    }
  };

  // One face query at a time; the newest point waits for the one running.
  let hovering = false;
  let nextHover: PieceFace | null | undefined;
  const askFace = async (at: PieceFace | null): Promise<void> => {
    hovering = true;
    try {
      if (at === null || get().mate.step === null) {
        set({ hover: null });
        return;
      }
      const { face } = await api.assembly.pieceFace(at);
      const stillPicking = get().active && get().mate.step !== null;
      set({ hover: face === null || !stillPicking ? null : { key: pieceKey(at), face } });
    } catch {
      set({ hover: null });
    } finally {
      hovering = false;
      const queued = nextHover;
      nextHover = undefined;
      if (queued !== undefined) {
        void askFace(queued);
      }
    }
  };

  return {
    active: false,
    pieces: [],
    components: [],
    reliefs: {},
    loading: false,
    selected: null,
    isolated: null,
    live: null,
    snap: true,
    mate: NO_MATE,
    mates: [],
    selectedMate: null,
    explodeSteps: [],
    hover: null,
    placing: null,
    ghosts: {},
    clearance: 0.5,
    issues: null,
    checking: false,
    cutList: null,
    error: null,
    notice: null,

    open: async () => {
      set({
        active: true,
        issues: null,
        cutList: null,
        notice: null,
        error: null,
        mate: { ...NO_MATE, kind: get().mate.kind },
        selectedMate: null,
        isolated: null,
      });
      await load(true);
    },

    close: () => {
      loads += 1;
      set({
        active: false,
        pieces: [],
        components: [],
        reliefs: {},
        loading: false,
        selected: null,
        isolated: null,
        live: null,
        mate: { ...NO_MATE, kind: get().mate.kind },
        mates: [],
        selectedMate: null,
        explodeSteps: [],
        hover: null,
        placing: null,
        ghosts: {},
        issues: null,
        cutList: null,
      });
    },

    refresh: () => load(false),

    select: (key) => set({ selected: key }),

    isolate: (key) => set({ isolated: key }),

    setLive: (live) => {
      // A new place keeps the last solve on show until the next one comes.
      const before = get().live;
      const solved = live !== null && before?.key === live.key ? before.solved : undefined;
      set({ live: live === null ? null : { ...live, solved: live.solved ?? solved } });
      if (live === null || !solvesLive(get().mates)) {
        nextSolve = null;
        return;
      }
      if (solving) {
        nextSolve = live;
        return;
      }
      void solveLive(live);
    },

    place: async (key, position, rotation) => {
      const piece = refFor(key);
      if (piece === undefined) {
        return false;
      }
      // Show it there at once, the other pieces where the drag's last
      // solve put them; the revision change reloads the list.
      const solved = get().live?.key === key ? get().live?.solved : undefined;
      nextSolve = null;
      set({
        pieces: get().pieces.map((p) => {
          const at = solved?.[pieceKey(p)];
          if (pieceKey(p) === key) {
            return { ...p, position, rotation };
          }
          return at === undefined ? p : { ...p, position: at.position, rotation: at.rotation };
        }),
        live: null,
        issues: null,
      });
      try {
        await api.assembly.placePiece(piece, position, rotation);
        // Picks up the new revision, so Undo offers the move.
        await useDocumentStore.getState().refresh();
        return true;
      } catch (error) {
        set({ error: message(error) });
        await load(false);
        return false;
      }
    },

    setSnap: (snap) => set({ snap }),

    startMate: () =>
      set({ mate: { ...get().mate, step: 'target', target: null, moving: null, last: null } }),

    cancelMate: () => set({ mate: { ...get().mate, step: null }, hover: null }),

    pickFace: async (face) => {
      const { mate, ready } = withFace(get().mate, face);
      set({ mate, selected: pieceKey(face), hover: mate.step === null ? null : get().hover });
      if (ready) {
        await get().applyMate();
      }
    },

    setMateOptions: (options) => set({ mate: { ...get().mate, ...options } }),

    hoverFace: (at) => {
      if (at === null && get().hover === null && !hovering) {
        return;
      }
      if (hovering) {
        nextHover = at;
        return;
      }
      void askFace(at);
    },

    setSource: async (sheet, model) => {
      set({ error: null, issues: null, cutList: null });
      try {
        await api.assembly.pieceSource(sheet, model);
        await useDocumentStore.getState().refresh();
        // The sheet's new piece stays picked, so the choice can be changed back.
        await load(false);
        const first = get().pieces.find((p) => p.sheet === sheet);
        set({ selected: first === undefined ? null : pieceKey(first) });
      } catch (error) {
        set({ error: message(error) });
      }
    },

    addComponents: async (ids, place) => {
      if (ids.length === 0) {
        return;
      }
      set({ error: null, issues: null, cutList: null });
      try {
        await api.assembly.addComponentPieces(ids, place);
        await useDocumentStore.getState().refresh();
        await load(false);
        const first = ids[0];
        set({
          selected:
            first === undefined
              ? get().selected
              : pieceKey({ sheet: 0, part: 1, component: first }),
        });
      } catch (error) {
        set({ error: message(error) });
      }
    },

    startPlacing: (component, dragging) => {
      nextLanding = null;
      set({ placing: { component, dragging, at: null, pose: null }, error: null });
      const isPiece = get().pieces.some((p) => pieceKey(p) === componentPieceKey(component));
      if (isPiece || get().ghosts[component] !== undefined) {
        return;
      }
      void api.assembly.componentRelief(component).then(
        (relief) => {
          if (get().active) {
            set({ ghosts: { ...get().ghosts, [component]: relief } });
          }
        },
        () => {
          // No ghost then; the drop still reports what the kernel says.
        },
      );
    },

    movePlacing: (at) => {
      const placing = get().placing;
      if (placing === null) {
        return;
      }
      // The last answer stays on show until the next one comes.
      set({ placing: { ...placing, at, pose: at === null ? null : placing.pose } });
      if (at === null) {
        nextLanding = null;
      } else if (landing) {
        nextLanding = { component: placing.component, at };
      } else {
        void askLanding(placing.component, at);
      }
    },

    cancelPlacing: () => {
      nextLanding = null;
      set({ placing: null });
    },

    dropComponent: async (component, at) => {
      nextLanding = null;
      set({ placing: null });
      // Let go over a furniture panel's broad face: the component goes on
      // that face; a narrow edge is refused.
      if (at.onto !== null) {
        const pick = faceTargetOf(at.onto, at.point);
        if (pick?.kind === 'edge') {
          set({ notice: furnitureStrings.furniture.face.edgeRefused });
          return;
        }
        if (pick?.kind === 'face') {
          await useFurnitureStore.getState().addModelToFace(pick.target, { component });
          return;
        }
      }
      let pose: PiecePose;
      try {
        // Asked again for the point let go, so it lands where the kernel says.
        const { position, rotation } = await api.assembly.dropPlace(component, at);
        pose = { position, rotation };
      } catch (error) {
        // A grounded piece stays put: the kernel refuses and says so.
        set({ error: message(error) });
        return;
      }
      const key = componentPieceKey(component);
      if (!get().pieces.some((p) => pieceKey(p) === key)) {
        await get().addComponents([component], { pose });
        return;
      }
      set({ selected: key });
      await get().place(key, pose.position, pose.rotation);
    },

    removeComponent: async (id) => {
      set({ error: null, issues: null, cutList: null });
      try {
        await api.assembly.removeComponentPiece(id);
        await useDocumentStore.getState().refresh();
        if (get().selected === pieceKey({ sheet: 0, part: 1, component: id })) {
          set({ selected: null });
        }
        await load(false);
      } catch (error) {
        set({ error: message(error) });
      }
    },

    applyMate: async () => {
      const { kind, target, moving, offset, flip, turn, last } = get().mate;
      if (target === null || moving === null) {
        return false;
      }
      set({ issues: null, error: null });
      try {
        const reply = await api.assembly.matePieces({
          kind,
          target,
          moving,
          offset,
          flip,
          turn,
          replace: last,
        });
        const key = pieceKey(reply.moved);
        const { position, rotation } = reply;
        set({
          mate: { ...get().mate, turn: reply.turn, last: reply.mate },
          selectedMate: reply.mate,
          pieces:
            position === null || rotation === null
              ? get().pieces
              : get().pieces.map((p) => (pieceKey(p) === key ? { ...p, position, rotation } : p)),
        });
        // The solve may have moved other pieces too: the revision reloads them.
        await useDocumentStore.getState().refresh();
        return true;
      } catch (error) {
        set({ error: message(error) });
        return false;
      }
    },

    selectMate: (id) => set({ selectedMate: id }),

    editMate: async (id, edit) => {
      set({ issues: null, error: null });
      try {
        await api.assembly.editMate(id, edit);
        await useDocumentStore.getState().refresh();
        return true;
      } catch (error) {
        set({ error: message(error) });
        return false;
      }
    },

    deleteMate: async (id) => {
      set({ error: null });
      try {
        await api.assembly.deleteMate(id);
        const { mate, selectedMate } = get();
        set({
          selectedMate: selectedMate === id ? null : selectedMate,
          mate: mate.last === id ? { ...mate, last: null } : mate,
        });
        await useDocumentStore.getState().refresh();
      } catch (error) {
        set({ error: message(error) });
      }
    },

    setExplodeSteps: async (steps, label) => {
      set({ error: null });
      try {
        const reply = await api.assembly.setExplodeSteps(steps, label);
        set({ explodeSteps: reply.steps });
        await useDocumentStore.getState().refresh();
        return reply.steps;
      } catch (error) {
        set({ error: message(error) });
        return null;
      }
    },

    setPieceState: async (action, key) => {
      const piece = key === null ? null : (refFor(key) ?? null);
      if (key !== null && piece === null) {
        return;
      }
      set({ error: null });
      if (action === 'showAll') {
        set({ isolated: null });
      }
      try {
        await api.assembly.pieceState(action, piece);
        await useDocumentStore.getState().refresh();
      } catch (error) {
        set({ error: message(error) });
      }
    },

    setClearance: (mm) => set({ clearance: mm, issues: null }),

    check: async () => {
      set({ checking: true, issues: null, error: null });
      try {
        const { issues } = await api.assembly.checkPieces(get().clearance);
        set({ issues });
      } catch (error) {
        set({ error: message(error) });
      } finally {
        set({ checking: false });
      }
    },

    loadCutList: async () => {
      try {
        const { rows } = await api.assembly.pieceCutList();
        set({ cutList: rows, error: null });
      } catch (error) {
        set({ error: message(error) });
      }
    },

    setNotice: (notice) => set({ notice }),
    setError: (error) => set({ error }),
  };
});

// Undo, redo and every other document change can move pieces or change a
// sheet, and a check made before no longer holds; toolpath changes change
// the reliefs, the parts and the cut list.
useDocumentStore.subscribe((state, previous) => {
  if (state.revision !== previous.revision && useAssemblyStore.getState().active) {
    useAssemblyStore.setState({ issues: null });
    void useAssemblyStore.getState().refresh();
  }
});
useToolpathStore.subscribe((state, previous) => {
  const store = useAssemblyStore.getState();
  if (
    store.active &&
    state.toolpaths !== previous.toolpaths &&
    toolpathsKey(state.toolpaths) !== toolpathsKey(previous.toolpaths)
  ) {
    useAssemblyStore.setState({
      reliefs: {},
      issues: null,
      cutList: null,
      mate: { ...NO_MATE, kind: store.mate.kind },
    });
    void store.refresh();
  }
});
