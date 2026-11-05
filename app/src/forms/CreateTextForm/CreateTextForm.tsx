import { useCallback, useEffect, useMemo, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { Bbox, GlyphBox, Path, PlainTextArgs, TextPreviewArgs } from '../../bridge/document';
import { lengthText } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { circlePath, rectPath } from '../../lib/geometry';
import { unitSuffix } from '../../lib/units';
import { useDrawController, useOverlays } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import { useTextPreview } from '../../lib/useTextPreview';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController, PreviewOverlay } from '../../state/drawStore';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertTextSpecUnits,
  initialTextSpecState,
  stateFromSpec,
  textSpec,
  validateTextSpec,
} from '../TextSpecFields/model';
import { TextAnchorField } from '../TextAnchorField/TextAnchorField';
import { archHandles, clickGap, gapAt } from '../TextSelectionForm/model';
import { TextSpecFields } from '../TextSpecFields/TextSpecFields';
import {
  autoLayoutSpec,
  boxBounds,
  boxErrors,
  boxFromDrag,
  boxState,
  convertBoxUnits,
  initialAutoLayoutState,
  layoutBounds,
  stateFromLayout,
  type AutoLayoutState,
} from './boxModel';
import { LiveText } from './liveText';
import {
  adjustmentsOf,
  anchorHandle,
  convertTextPosition,
  initialTextPosition,
  positionAt,
  startAnchor,
  textPosition,
  validateTextPosition,
  withAdjustments,
  wrapWidth,
  type TextAdjustments,
} from './model';

import './CreateTextForm.scss';

const text = strings.text.create;
const box = strings.text.autoLayout;

/** Create Text makes plain text, or text that fills a box. */
type TextMode = 'text' | 'box';

const MODES: ReadonlyArray<Choice<TextMode>> = [
  { value: 'text', label: text.modeText },
  { value: 'box', label: text.modeBox },
];

const MARGINS: ReadonlyArray<Choice<AutoLayoutState['margin']>> = [
  { value: 'none', label: box.marginNone },
  { value: 'normal', label: box.marginNormal },
  { value: 'wide', label: box.marginWide },
];

const VERTICAL: ReadonlyArray<Choice<AutoLayoutState['vertical']>> = [
  { value: 'none', label: box.verticalNone },
  { value: 'lineSpacing', label: box.lineSpacing },
  { value: 'characterHeight', label: box.characterHeight },
];

const HORIZONTAL: ReadonlyArray<Choice<AutoLayoutState['horizontal']>> = [
  { value: 'none', label: box.horizontalNone },
  { value: 'wordSpacing', label: box.wordSpacing },
  { value: 'letterSpacing', label: box.letterSpacing },
  { value: 'characterWidth', label: box.characterWidth },
  { value: 'characterSize', label: box.characterSize },
];

/** A drag shorter than this many pixels is a click. */
const DRAG_PX = 4;

/** Distance in pixels within which a press picks up the anchor handle. */
const HANDLE_PX = 8;

/** The single text entity selected when the form opened, which it then edits. */
function selectedText(): number | null {
  const doc = useDocumentStore.getState();
  const only = doc.selection.ids.length === 1 ? doc.selection.ids[0] : undefined;
  const entity = doc.entities.find((e) => e.id === only);
  return entity !== undefined && entity.kind === 'text' ? entity.id : null;
}

/** Remembers the anchor of new plain text for the next Create Text. */
function rememberAnchor(args: PlainTextArgs): void {
  const anchor = args.spec.anchor;
  const prefs = usePrefsStore.getState();
  if (
    anchor !== undefined &&
    args.spec.autoLayout === undefined &&
    anchor !== prefs.prefs.general.textAnchor
  ) {
    void prefs.update({ general: { textAnchor: anchor } });
  }
}

/** The job unit now, for pointer handlers that run outside of render. */
function jobUnits() {
  return useDocumentStore.getState().job.units;
}

/** New text starts with no words: nothing is made until some are typed. */
function emptyTextSpec(units: Parameters<typeof initialTextSpecState>[0]) {
  return { ...initialTextSpecState(units), text: '' };
}

export function CreateTextForm(): JSX.Element {
  const job = useDocumentStore((store) => store.job);
  const closeForm = useUiStore((store) => store.closeForm);
  const pxPerMm = useViewStore((store) => store.camera.pxPerMm);
  // The text the form edits: the selected one, one clicked in the view, or
  // the one the first change made; null before any is made.
  const [editing, setEditing] = useState<number | null>(null);
  // The text box takes the focus when the form opens and again after a
  // click places the text, so the typed letters go into it.
  const [focusSerial, setFocusSerial] = useState(1);
  // The text whose fields were last loaded, so its own layout is not sent
  // back as an edit.
  const [loaded, setLoaded] = useState<number | null>(null);
  const baseline = useRef<number | null>(null);
  const spec = useFormState(emptyTextSpec, convertTextSpecUnits);
  const position = useFormState(
    (units) =>
      initialTextPosition(units, job, false, startAnchor(usePrefsStore.getState().prefs.general)),
    convertTextPosition,
  );
  // Text in a Box starts from the selected vectors (else the material),
  // taken before new text takes the selection.
  const textBox = useFormState(
    (units) => boxState(layoutBounds(useDocumentStore.getState().selection.bbox, job), units),
    convertBoxUnits,
  );
  const units = spec.units;
  const [mode, setMode] = useState<TextMode>('text');
  const [layout, setLayout] = useState<AutoLayoutState>(initialAutoLayoutState);
  const [dragBox, setDragBox] = useState<Bbox | null>(null);
  // Kerning, line gaps and arch set with the handles in the view.
  const [kept, setKept] = useState<TextAdjustments>({});
  // The drawn letters of the text being edited, for the kerning clicks.
  const [letters, setLetters] = useState<{
    id: number;
    glyphs: GlyphBox[];
    height: number;
  } | null>(null);
  const setSpecState = spec.setState;
  const setPosition = position.setState;
  const setTextBox = textBox.setState;

  const live = useMemo(
    () =>
      new LiveText({
        create: async (args) => {
          const made = await api.text.create(args);
          await api.selection.set([made.id]);
          const doc = useDocumentStore.getState();
          await doc.refresh();
          rememberAnchor(args);
          return { id: made.id, revision: useDocumentStore.getState().revision };
        },
        update: async (id, args, merge) => {
          const reply = await api.text.update(id, args.spec, args.x, args.y, merge ?? undefined);
          await useDocumentStore.getState().refresh();
          return reply.revision;
        },
        onTarget: setEditing,
        onError: () => {
          // The text went (an Undo while the form is open): start afresh.
          live.retarget(null);
          setEditing(null);
          void useDocumentStore.getState().refresh();
        },
      }),
    [],
  );

  /** Loads text `id` into the form, which edits it from then on. */
  const load = useCallback(
    (id: number) => {
      void api.entities.get(id).then((entity) => {
        if (entity.kind !== 'text' || entity.spec.onCurve !== undefined) {
          return;
        }
        const u = jobUnits();
        live.retarget(id);
        baseline.current = id;
        setSpecState(stateFromSpec(entity.spec, u));
        setKept(adjustmentsOf(entity.spec));
        const layoutOf = entity.spec.autoLayout;
        if (layoutOf !== undefined) {
          setMode('box');
          setLayout(stateFromLayout(layoutOf));
          setTextBox(boxState(layoutOf.bounds, u));
        } else {
          setMode('text');
          setPosition((current) => {
            const moved = {
              ...positionAt(current, entity.origin, u),
              anchor: entity.spec.anchor ?? null,
            };
            // Text made as a wrapped block keeps its column width.
            const width = entity.spec.wrapWidth;
            return width === undefined
              ? { ...moved, wrap: false }
              : { ...moved, wrap: true, width: lengthText(width, u) };
          });
        }
        setEditing(id);
        setLoaded(id);
        // The text being edited shows selected, as a click in the view would.
        if (useDocumentStore.getState().selection.ids.join() !== String(id)) {
          void useDocumentStore.getState().select([id]);
        }
      });
    },
    [live, setPosition, setSpecState, setTextBox],
  );

  useEffect(() => {
    const first = selectedText();
    if (first !== null) {
      load(first);
    }
  }, [load]);

  const specErrors = useMemo(() => validateTextSpec(spec.state, units), [spec.state, units]);
  const positionErrors = useMemo(
    () => validateTextPosition(position.state, units),
    [position.state, units],
  );
  const sizeErrors = useMemo(() => boxErrors(textBox.state, units), [textBox.state, units]);
  const bounds = boxBounds(textBox.state, units);

  let built = null;
  let at: { x: number; y: number } | null = null;
  if (mode === 'box') {
    const plain = textSpec({ ...spec.state, size: spec.state.size || '10' }, units);
    built = plain === null || bounds === null ? null : autoLayoutSpec(plain, bounds, layout);
    at = bounds === null ? null : { x: bounds[0], y: bounds[1] };
  } else {
    const plain = textSpec(spec.state, units);
    const width = wrapWidth(position.state, units);
    const anchor = position.state.anchor;
    const adjusted = plain === null ? null : withAdjustments(plain, kept);
    const anchored = adjusted === null || anchor === null ? adjusted : { ...adjusted, anchor };
    built =
      anchored === null || width === null
        ? null
        : width === undefined
          ? anchored
          : { ...anchored, wrapWidth: width };
    at = textPosition(position.state, units);
  }
  const args: PlainTextArgs | null = built === null || at === null ? null : { spec: built, ...at };
  // Until the first change makes the text, the kernel lays it out as a
  // dashed preview; afterwards the text itself shows every change.
  const preview: TextPreviewArgs | null =
    args === null || editing !== null ? null : { kind: 'plain', ...args };
  useTextPreview(preview);

  const argsKey = args === null ? '' : JSON.stringify(args);
  useEffect(() => {
    if (argsKey === '') {
      return;
    }
    const wanted = JSON.parse(argsKey) as PlainTextArgs;
    if (baseline.current !== null) {
      // Wait for the loaded fields, then take them as the text's own layout.
      if (loaded === baseline.current) {
        live.retarget(baseline.current, wanted);
        baseline.current = null;
      }
      return;
    }
    live.apply(wanted);
  }, [argsKey, live, loaded]);

  const revision = useDocumentStore((store) => store.revision);
  const editedBox = useDocumentStore(
    (store) => store.entities.find((e) => e.id === editing && e.kind === 'text')?.bbox ?? null,
  );
  useEffect(() => {
    if (editing === null || mode !== 'text') {
      return undefined;
    }
    let current = true;
    void api.text
      .glyphs(editing)
      .then((reply) => {
        if (current) {
          setLetters({ id: editing, glyphs: reply.glyphs, height: reply.height });
        }
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [editing, mode, revision]);

  const shownBox = mode === 'box' ? (dragBox ?? bounds) : null;
  const handleAt = mode === 'text' && editing !== null ? at : null;
  const handleX = handleAt?.x ?? null;
  const handleY = handleAt?.y ?? null;
  const arch = mode === 'text' && editedBox !== null ? archHandles(editedBox) : null;
  const archKey = arch === null ? '' : JSON.stringify(arch);
  const archRef = useRef(arch);
  useEffect(() => {
    archRef.current = archKey === '' ? null : (JSON.parse(archKey) as typeof arch);
  }, [archKey]);
  const overlays = useMemo<PreviewOverlay[]>(() => {
    if (handleX !== null && handleY !== null) {
      const marks: PreviewOverlay[] = [
        { path: anchorHandle({ x: handleX, y: handleY }, pxPerMm), tone: 'accent', fill: true },
      ];
      if (archKey !== '') {
        const ends = JSON.parse(archKey) as NonNullable<typeof arch>;
        const r = (HANDLE_PX * 0.5) / pxPerMm;
        for (const end of [ends.top, ends.bottom]) {
          marks.push({ path: circlePath(end.x, end.y, r), tone: 'second', fill: true });
        }
      }
      return marks;
    }
    if (shownBox === null) {
      return [];
    }
    const path: Path = rectPath(
      shownBox[0],
      shownBox[1],
      shownBox[2] - shownBox[0],
      shownBox[3] - shownBox[1],
    );
    return [{ path, tone: 'accent', dashed: true }];
  }, [shownBox, handleX, handleY, pxPerMm, archKey]);
  useOverlays(overlays);

  const modeRef = useRef(mode);
  useEffect(() => {
    modeRef.current = mode;
  }, [mode]);
  const startRef = useRef<{ x: number; y: number } | null>(null);
  const draggingAnchor = useRef(false);
  // An arch drag: where it started and the arch then.
  const archDrag = useRef<{ fromY: number; arch: number } | null>(null);
  const keptRef = useRef(kept);
  const lettersRef = useRef(letters);
  const argsRef = useRef(args);
  useEffect(() => {
    keptRef.current = kept;
    lettersRef.current = letters;
    argsRef.current = args;
  });
  const handleRef = useRef<{ x: number; y: number } | null>(null);
  useEffect(() => {
    handleRef.current = handleX === null || handleY === null ? null : { x: handleX, y: handleY };
  }, [handleX, handleY]);
  const controller = useMemo<DrawController>(() => {
    const perMm = () => useViewStore.getState().camera.pxPerMm;
    const minimum = () => DRAG_PX / perMm();
    return {
      cursor: 'crosshair',
      pointerDown: (p) => {
        if (modeRef.current === 'box') {
          startRef.current = p;
          return;
        }
        const reach = HANDLE_PX / perMm();
        const handle = handleRef.current;
        if (handle !== null && Math.hypot(p.x - handle.x, p.y - handle.y) <= reach) {
          draggingAnchor.current = true;
          return;
        }
        const raw = { x: p.rawX, y: p.rawY };
        const ends = archRef.current;
        if (
          handle !== null &&
          ends !== null &&
          [ends.top, ends.bottom].some((h) => Math.hypot(h.x - raw.x, h.y - raw.y) <= reach)
        ) {
          archDrag.current = { fromY: raw.y, arch: keptRef.current.arch ?? 0 };
          return;
        }
        // A click between letters or lines of the text kerns or spaces them.
        const drawn = lettersRef.current;
        const now = argsRef.current;
        if (drawn !== null && drawn.id === live.target && now !== null) {
          const gap = gapAt(drawn.glyphs, raw, drawn.height);
          if (gap !== null) {
            const keys = { shift: p.shift, ctrl: p.ctrl, alt: p.alt === true };
            setKept(adjustmentsOf(clickGap(now.spec, drawn.glyphs, gap, keys)));
            return;
          }
        }
        // A click on other text edits it; elsewhere it places new text.
        void api.entities
          .hitTest({ x: p.x, y: p.y, tolerance: HANDLE_PX / perMm() })
          .then(({ ids }) => {
            const doc = useDocumentStore.getState();
            const hit = ids
              .map((id) => doc.entities.find((e) => e.id === id))
              .find((e) => e !== undefined && e.kind === 'text');
            if (hit !== undefined && hit.id !== live.target) {
              load(hit.id);
              return;
            }
            if (hit !== undefined) {
              return;
            }
            if (live.target !== null) {
              live.retarget(null);
              baseline.current = null;
              setEditing(null);
              setKept({});
              setSpecState((current) => ({ ...current, text: '' }));
            }
            setPosition((current) => positionAt(current, p, jobUnits()));
            setFocusSerial((serial) => serial + 1);
          });
      },
      pointerMove: (p) => {
        const arching = archDrag.current;
        if (arching !== null) {
          const next = arching.arch + (p.rawY - arching.fromY);
          setKept((current) => {
            const rest = { ...current };
            delete rest.arch;
            return Math.abs(next) < 1e-6 ? rest : { ...rest, arch: next };
          });
          return;
        }
        if (draggingAnchor.current) {
          setPosition((current) => positionAt(current, p, jobUnits()));
          return;
        }
        const start = startRef.current;
        if (start !== null) {
          setDragBox(boxFromDrag(start, p, minimum()));
        }
      },
      pointerUp: (p) => {
        if (archDrag.current !== null) {
          archDrag.current = null;
          return;
        }
        if (draggingAnchor.current) {
          draggingAnchor.current = false;
          return;
        }
        const clicked = startRef.current;
        if (clicked === null) {
          return;
        }
        const drawn = boxFromDrag(clicked, p, minimum());
        startRef.current = null;
        setDragBox(null);
        const u = jobUnits();
        setTextBox((current) =>
          drawn === null ? { ...current, x: clicked.x, y: clicked.y } : boxState(drawn, u),
        );
        setFocusSerial((serial) => serial + 1);
      },
      cancel: () => {
        startRef.current = null;
        draggingAnchor.current = false;
        archDrag.current = null;
        setDragBox(null);
      },
    };
  }, [live, load, setPosition, setSpecState, setTextBox]);
  useDrawController(controller);

  const unit = unitSuffix(units);
  const hint = editing !== null ? text.editHint : mode === 'box' ? text.boxHint : text.hint;
  return (
    <form className="create-text-form" onSubmit={(event) => event.preventDefault()}>
      <ChoiceField label={text.mode} onChange={setMode} options={MODES} value={mode} />
      <p className="create-text-form__hint">{hint}</p>
      <TextSpecFields
        errors={{
          ...specErrors,
          // An empty box only means nothing has been typed yet.
          text: undefined,
          size: mode === 'box' ? undefined : specErrors.size,
        }}
        focusSerial={focusSerial}
        hideSize={mode === 'box'}
        setState={setSpecState}
        state={spec.state}
        units={units}
      />
      {mode === 'text' ? (
        <fieldset className="create-text-form__group">
          <legend className="create-text-form__legend">{text.position}</legend>
          <NumberField
            error={positionErrors.x}
            label={text.x}
            onChange={(x) => setPosition((current) => ({ ...current, x }))}
            unit={unit}
            value={position.state.x}
          />
          <NumberField
            error={positionErrors.y}
            label={text.y}
            onChange={(y) => setPosition((current) => ({ ...current, y }))}
            unit={unit}
            value={position.state.y}
          />
          <TextAnchorField
            name="create-text-anchor"
            onChange={(value) => setPosition((current) => ({ ...current, anchor: value }))}
            value={position.state.anchor}
          />
        </fieldset>
      ) : (
        <>
          <fieldset className="create-text-form__group">
            <legend className="create-text-form__legend">{box.box}</legend>
            <NumberField
              error={sizeErrors.width}
              label={text.boxWidth}
              onChange={(width) => setTextBox((current) => ({ ...current, width }))}
              unit={unit}
              value={textBox.state.width}
            />
            <NumberField
              error={sizeErrors.height}
              label={text.boxHeight}
              onChange={(height) => setTextBox((current) => ({ ...current, height }))}
              unit={unit}
              value={textBox.state.height}
            />
          </fieldset>
          <ChoiceField
            label={box.margin}
            onChange={(margin) => setLayout((current) => ({ ...current, margin }))}
            options={MARGINS}
            value={layout.margin}
          />
          <ChoiceField
            label={box.vertical}
            onChange={(vertical) => setLayout((current) => ({ ...current, vertical }))}
            options={VERTICAL}
            value={layout.vertical}
          />
          <ChoiceField
            label={box.horizontal}
            onChange={(horizontal) => setLayout((current) => ({ ...current, horizontal }))}
            options={HORIZONTAL}
            value={layout.horizontal}
          />
        </>
      )}
      <FormButtons onClose={closeForm} />
    </form>
  );
}
