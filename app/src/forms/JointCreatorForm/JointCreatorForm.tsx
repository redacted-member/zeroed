import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type JSX,
} from 'react';

import { api } from '../../bridge/formApi';
import { toolDiameter, type Tool } from '../../bridge/cam';
import type { Path, Point } from '../../bridge/document';
import type {
  EdgeLine,
  Fit,
  HingePattern,
  JointList,
  JointPreview,
  JointSettings,
  PickedEdge,
  Relief,
} from '../../bridge/joints';
import { lengthText, lengthValue } from '../../lib/formFields';
import { jointsStrings } from '../../lib/strings/joints';
import { whyStrings } from '../../lib/strings/why';
import { unitSuffix } from '../../lib/units';
import { useFormState } from '../../lib/useFormState';
import { useLabels, useOverlays } from '../../lib/useDrawController';
import { useDocumentStore } from '../../state/documentStore';
import type { PreviewLabel, PreviewOverlay } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField } from '../ChoiceField/ChoiceField';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { jointFit } from '../HelperPicture/pictures/furniture';
import { HelperRow } from '../HelperRow/HelperRow';
import { IconButton } from '../IconButton/IconButton';
import { JointGallery, type GalleryItem } from '../JointGallery/JointGallery';
import { NumberField } from '../NumberField/NumberField';
import { SelectField } from '../SelectField/SelectField';
import { ToolPicker } from '../ToolPicker/ToolPicker';
import { JOINT_ICONS } from './jointIcons';
import {
  afterCreate,
  convertJointUnits,
  editState,
  fitLabel,
  initialJointState,
  JOINT_KINDS,
  jointsOf,
  kindInfo,
  paramsOf,
  sentence,
  shownLine,
  specOf,
  stage,
  stepBack,
  withKind,
  withOffset,
  type JointFormState,
  type NumberKey,
} from './model';
import { usePicks } from './usePicks';

import './JointCreatorForm.scss';

const JointView3D = lazy(() =>
  import('../../views/JointView3D/JointView3D').then((m) => ({ default: m.JointView3D })),
);

const text = jointsStrings.joints;

const FITS: readonly Fit[] = ['slip', 'press', 'glue', 'knockDown', 'custom'];
const RELIEFS: ReadonlyArray<{ value: Relief; icon: string }> = [
  { value: 'none', icon: 'reliefNone' },
  { value: 'dogBone', icon: 'reliefDogBone' },
  { value: 'tBone', icon: 'reliefTBone' },
];

const PATTERNS: ReadonlyArray<{ value: HingePattern; icon: string }> = [
  { value: 'straight', icon: 'hingeStraight' },
  { value: 'sinuous', icon: 'hingeSinuous' },
  { value: 'spiral', icon: 'hingeSpiral' },
];

const GALLERY: GalleryItem[] = JOINT_KINDS.map((info) => ({
  kind: info.kind,
  name: text.kinds[info.kind].name,
  line: text.kinds[info.kind].line,
  icon: JOINT_ICONS[info.kind] ?? '',
}));

/** A kernel answer and the question it answers. */
interface Answer<T> {
  key: string;
  value: T | null;
  problem: string | null;
}

const NO_PAIRS: Array<{ a: EdgeLine; b: EdgeLine }> = [];

/** The tool picked last, kept while the app runs. */
let lastTool: Tool | null = null;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function segmentPath(from: Point, to: Point): Path {
  return {
    subpaths: [{ start: from, segments: [{ kind: 'line', to }], closed: false }],
  };
}

function edgePath(edge: EdgeLine): Path {
  return segmentPath({ x: edge.from[0], y: edge.from[1] }, { x: edge.to[0], y: edge.to[1] });
}

/** The line on a part's face a face pick puts the first part's near face on. */
function facePath(edge: EdgeLine, offset: number, along: number, length: number): Path {
  const [x0, y0] = edge.from;
  const len = Math.hypot(edge.to[0] - x0, edge.to[1] - y0) || 1;
  const u = { x: (edge.to[0] - x0) / len, y: (edge.to[1] - y0) / len };
  // The part lies left of its edge.
  const n = { x: -u.y, y: u.x };
  const at = (s: number) => ({
    x: x0 + u.x * s + n.x * offset,
    y: y0 + u.y * s + n.y * offset,
  });
  return segmentPath(at(along - length / 2), at(along + length / 2));
}

function reasonText(reason: string): string {
  return reason === 'samePart' ? sentence(text.hints.pickBEdge) : sentence(reason);
}

/** Gadgets > Joint Creator: joints between flat parts, made on their vectors. */
export function JointCreatorForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const refresh = useDocumentStore((store) => store.refresh);
  const revision = useDocumentStore((store) => store.revision);
  const selection = useDocumentStore((store) => store.selection.ids);
  const { state, setState, stateRef, units, unitsRef } = useFormState(
    initialJointState,
    convertJointUnits,
  );
  const [list, setList] = useState<JointList | null>(null);
  // Kernel answers, each kept with the question it answers so a stale one
  // is never shown.
  const [previewResult, setPreviewResult] = useState<Answer<JointPreview> | null>(null);
  const [boxResult, setBoxResult] = useState<Answer<Array<{ a: EdgeLine; b: EdgeLine }>> | null>(
    null,
  );
  const [problem, setProblem] = useState<string | null>(null);
  const openedRef = useRef(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  const [explode, setExplode] = useState(0);
  const [tool, setToolState] = useState<Tool | null>(lastTool);
  const [thicknessText, setThicknessText] = useState<Record<number, string>>({});
  const [clearanceText, setClearanceText] = useState<string | null>(null);
  const [offsetText, setOffsetText] = useState<string | null>(null);
  const explodeId = useId();
  const unit = unitSuffix(units);
  const now = stage(state);
  const info = kindInfo(state.kind);

  // The job's joints and shared settings, again after every change. The
  // first time, a panel opened on a jointed part edits its joint.
  useEffect(() => {
    let live = true;
    void api.joints.list().then(
      (reply) => {
        if (!live) {
          return;
        }
        setList(reply);
        if (!openedRef.current) {
          openedRef.current = true;
          const ids = useDocumentStore.getState().selection.ids;
          const [row] =
            ids.length === 1 && ids[0] !== undefined ? jointsOf(reply.joints, ids[0]) : [];
          if (row !== undefined) {
            setState((current) => editState(row, unitsRef.current, current));
          }
        }
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [revision, setState, unitsRef]);

  const spec = useMemo(() => specOf(state, units), [state, units]);
  const { errors, params } = useMemo(() => paramsOf(state, units), [state, units]);

  // The preview of the joint as Create would make it.
  const previewKey = spec === null ? '' : JSON.stringify([spec, state.editing, revision]);
  useEffect(() => {
    if (spec === null) {
      return undefined;
    }
    let live = true;
    void api.joints.preview(spec, state.editing).then(
      (reply) => {
        if (live) {
          setPreviewResult({ key: previewKey, value: reply, problem: null });
        }
      },
      (error: unknown) => {
        if (live) {
          setPreviewResult({ key: previewKey, value: null, problem: sentence(message(error)) });
        }
      },
    );
    return () => {
      live = false;
    };
  }, [spec, state.editing, previewKey]);
  const previewNow =
    previewResult !== null && previewResult.key === previewKey ? previewResult : null;
  const preview = previewNow?.value ?? null;

  // A box: which edges would be joined.
  const boxKey =
    state.box === null ? '' : JSON.stringify([state.box, state.kind, params, revision]);
  useEffect(() => {
    if (state.box === null) {
      return undefined;
    }
    let live = true;
    void api.joints.box(state.box, state.kind, params, true).then(
      (reply) => {
        if (live) {
          setBoxResult({ key: boxKey, value: reply.pairs, problem: null });
        }
      },
      (error: unknown) => {
        if (live) {
          setBoxResult({ key: boxKey, value: null, problem: sentence(message(error)) });
        }
      },
    );
    return () => {
      live = false;
    };
  }, [state.box, state.kind, params, boxKey]);
  const boxNow = boxResult !== null && boxResult.key === boxKey ? boxResult : null;
  const boxPairs = boxNow?.value ?? NO_PAIRS;
  const shownProblem = problem ?? previewNow?.problem ?? boxNow?.problem ?? null;

  // Picks change the live state at once, so a quick second click already
  // sees the first.
  const update = (change: (current: JointFormState) => JointFormState) => {
    const next = change(stateRef.current);
    stateRef.current = next;
    setState(next);
  };

  const { hover, clearHover } = usePicks(stateRef, {
    pickA: (edge: PickedEdge) => {
      setNotice(null);
      setProblem(null);
      const row = edge.joint === null ? undefined : list?.joints.find((j) => j.id === edge.joint);
      update((current) =>
        row === undefined
          ? { ...current, a: edge, b: null, editing: null }
          : editState(row, units, current),
      );
    },
    pickB: (b) => {
      setProblem(null);
      update((current) => ({ ...current, b }));
    },
    back: () => {
      const current = stateRef.current;
      const back = stepBack(current);
      if (back === current) {
        return false;
      }
      update(() => back);
      return true;
    },
  });

  // What the 2D view draws: the picks, the hovered edge and the result.
  const overlays = useMemo<PreviewOverlay[]>(() => {
    const out: PreviewOverlay[] = [];
    if (preview !== null) {
      for (const part of preview.parts) {
        out.push({ path: part.path, tone: 'accent' });
      }
      for (const made of preview.made) {
        out.push({ path: made.path, tone: 'second', dashed: true });
      }
    }
    for (const pair of boxPairs) {
      out.push({ path: edgePath(pair.a), tone: 'first' });
      out.push({ path: edgePath(pair.b), tone: 'second' });
    }
    if (state.a !== null) {
      out.push({ path: edgePath(state.a), tone: 'first' });
    }
    if (state.b !== null) {
      out.push({
        path:
          state.b.kind === 'edge'
            ? edgePath(state.b.edge)
            : facePath(state.b.edge, state.b.offset, state.b.along, state.a?.length ?? 0),
        tone: 'second',
      });
    }
    if (hover !== null && hover.pick === 'a') {
      out.push({ path: edgePath(hover.edge), tone: 'first', dashed: true });
    }
    if (hover !== null && hover.pick === 'b') {
      out.push({
        path:
          hover.b.kind === 'edge'
            ? edgePath(hover.b.edge)
            : facePath(hover.b.edge, hover.b.offset, hover.b.along, state.a?.length ?? 0),
        tone: 'second',
        dashed: true,
      });
    }
    return out;
  }, [preview, boxPairs, state.a, state.b, hover]);
  useOverlays(overlays);

  const labels = useMemo<PreviewLabel[]>(
    () =>
      hover !== null && hover.pick === 'none' && hover.reason !== ''
        ? [{ at: hover.at, text: reasonText(hover.reason), tone: 'accent' }]
        : [],
    [hover],
  );
  useLabels(labels);

  const settings = list?.settings ?? null;
  const saveSettings = useCallback(
    async (next: JointSettings) => {
      setBusy(true);
      try {
        await api.joints.setSettings(next);
        setProblem(null);
      } catch (error) {
        setProblem(sentence(message(error)));
      } finally {
        setBusy(false);
        await refresh();
      }
    },
    [refresh],
  );

  const setTool = (picked: Tool) => {
    lastTool = picked;
    setToolState(picked);
    const diameter = toolDiameter(picked);
    if (settings !== null && diameter > 0 && Math.abs(diameter - settings.toolDiameter) > 1e-6) {
      void saveSettings({ ...settings, toolDiameter: diameter });
    }
  };

  // The sheets whose thickness matters now: the picked parts', else the
  // active sheet's.
  const activeSheet = useDocumentStore((store) => store.sheets.find((s) => s.active)?.id ?? 0);
  const sheetIds = useMemo(() => {
    const ids = [state.a?.sheet, state.b?.edge.sheet].filter(
      (id): id is number => id !== undefined && id !== 0,
    );
    const unique = [...new Set(ids)];
    return unique.length > 0 ? unique : [activeSheet];
  }, [state.a, state.b, activeSheet]);

  const commitThickness = (sheet: number) => {
    const typed = thicknessText[sheet];
    const row = list?.sheets.find((s) => s.id === sheet);
    if (typed === undefined || settings === null || row === undefined) {
      return;
    }
    const mm = lengthValue(typed, units);
    setThicknessText((current) => {
      const next = { ...current };
      delete next[sheet];
      return next;
    });
    if (mm === null || !(mm > 0) || Math.abs(mm - row.thickness) < 1e-6) {
      return;
    }
    const thickness = { ...settings.thickness };
    if (Math.abs(mm - row.nominal) < 1e-6) {
      delete thickness[String(sheet)];
    } else {
      thickness[String(sheet)] = mm;
    }
    void saveSettings({ ...settings, thickness });
  };

  const commitOffset = () => {
    if (offsetText === null) {
      return;
    }
    const mm = lengthValue(offsetText, units);
    setOffsetText(null);
    if (mm !== null) {
      setState((current) => withOffset(current, mm));
    }
  };

  const commitClearance = () => {
    if (clearanceText === null || settings === null) {
      return;
    }
    const mm = lengthValue(clearanceText, units);
    setClearanceText(null);
    if (mm !== null && mm >= 0 && Math.abs(mm - settings.customClearance) > 1e-6) {
      void saveSettings({ ...settings, customClearance: mm });
    }
  };

  const create = async () => {
    if (busy) {
      return;
    }
    setBusy(true);
    setNotice(null);
    try {
      await api.undo.beginGroup();
      // The joints must be laid out for the tool that cuts them.
      if (
        state.makeToolpaths &&
        tool !== null &&
        settings !== null &&
        Math.abs(toolDiameter(tool) - settings.toolDiameter) > 1e-6
      ) {
        await api.joints.setSettings({ ...settings, toolDiameter: toolDiameter(tool) });
      }
      if (state.box !== null) {
        await api.joints.box(state.box, state.kind, params);
      } else if (spec !== null) {
        if (state.editing === null) {
          await api.joints.add(spec);
        } else {
          await api.joints.update(state.editing, spec);
        }
      }
      if (state.makeToolpaths && tool !== null) {
        // The kernel also mates each joint's parts in the Assembly View.
        const toolpaths = await api.joints.makeToolpaths(tool);
        setNotice(text.toolpathsMade(toolpaths.ids.length, toolpaths.mates));
      } else {
        setNotice(text.hints.made);
      }
      setProblem(null);
      setState(afterCreate);
    } catch (error) {
      setProblem(sentence(message(error)));
    } finally {
      await api.undo.endGroup().catch(() => undefined);
      setBusy(false);
      await refresh();
    }
  };

  const remove = async () => {
    if (state.editing === null || busy) {
      return;
    }
    setBusy(true);
    try {
      await api.joints.remove(state.editing);
      setState(afterCreate);
      setProblem(null);
    } catch (error) {
      setProblem(sentence(message(error)));
    } finally {
      setBusy(false);
      await refresh();
    }
  };

  const coupon = async () => {
    if (busy) {
      return;
    }
    setBusy(true);
    try {
      await api.joints.coupon(state.kind, params);
      setProblem(null);
      setNotice(null);
    } catch (error) {
      setProblem(sentence(message(error)));
    } finally {
      setBusy(false);
      await refresh();
    }
  };

  const hint =
    now === 'box'
      ? text.hints.box(boxPairs.length)
      : state.editing !== null
        ? text.hints.editing(text.kinds[state.kind].name, state.editing)
        : now === 'pickA'
          ? info.target === 'none'
            ? text.hints.pickHinge
            : text.hints.pickA
          : now === 'pickB'
            ? info.target === 'edge'
              ? text.hints.pickBEdge
              : text.hints.pickBFace
            : previewNow !== null && previewNow.problem !== null
              ? text.hints.refused
              : text.hints.ready;

  const resolved = preview?.outcome?.resolved ?? {};
  const numberField = (key: NumberKey) => {
    const spec = info.numbers.find((n) => n.key === key);
    if (spec === undefined) {
      return null;
    }
    const worked = resolved[key];
    const placeholder =
      typeof worked === 'number'
        ? `${text.auto} ${spec.unit === 'length' ? lengthText(worked, units) : String(worked)}`
        : text.auto;
    return (
      <NumberField
        error={errors[key] === undefined ? undefined : sentence(errors[key])}
        key={key}
        label={text.labels[spec.label ?? key]}
        onChange={(value) =>
          setState((current) => ({ ...current, numbers: { ...current.numbers, [key]: value } }))
        }
        placeholder={placeholder}
        unit={spec.unit === 'length' ? unit : spec.unit === 'angle' ? text.degrees : undefined}
        value={state.numbers[key] ?? ''}
      />
    );
  };

  const closedSelected = selection.length >= 2;
  const canCreate = !busy && (now === 'box' ? boxPairs.length > 0 : preview !== null);
  const busyWhy = busy ? whyStrings.why.busy : undefined;
  // Until a joint can be made, the hint line says what to pick next.
  const createWhy = busyWhy ?? (canCreate ? undefined : hint);

  return (
    <div className="joint-creator">
      <p aria-live="polite" className="joint-creator__hint">
        {shownLine(notice, hint)}
      </p>
      {shownProblem !== null ? (
        <p className="joint-creator__problem" role="alert">
          {shownProblem}
        </p>
      ) : null}

      <JointGallery
        items={GALLERY}
        label={text.gallery}
        onChange={(kind) => {
          setNotice(null);
          setState((current) => withKind(current, kind));
        }}
        value={state.kind}
      />

      {closedSelected && now !== 'box' ? (
        <div className="joint-creator__box">
          <span className="joint-creator__box-text">{text.hints.selectBox(selection.length)}</span>
          <IconButton
            icon="jointBox"
            label={text.box}
            onClick={() => setState((current) => ({ ...current, box: [...selection] }))}
            title={text.boxTitle}
          />
        </div>
      ) : null}

      <fieldset className="joint-creator__group">
        <legend className="joint-creator__legend">{text.kinds[state.kind].name}</legend>
        <dl className="joint-creator__picks">
          <dt>{text.picks.a}</dt>
          <dd>
            {state.a === null
              ? text.picks.none
              : text.picks.edge(lengthText(state.a.length, units) + unit)}
          </dd>
          <dt>{text.picks.b}</dt>
          <dd>
            {state.b === null
              ? text.picks.none
              : state.b.kind === 'edge'
                ? text.picks.edge(lengthText(state.b.edge.length, units) + unit)
                : text.picks.face(lengthText(state.b.offset, units) + unit)}
          </dd>
        </dl>
        {preview !== null && preview.outcome !== null && preview.outcome.notes.length > 0 ? (
          <ul className="joint-creator__notes">
            {preview.outcome.notes.map((note) => (
              <li key={note}>{sentence(note)}</li>
            ))}
          </ul>
        ) : null}
        {state.b !== null && state.b.kind === 'face' ? (
          <div className="joint-creator__offset" onBlur={commitOffset}>
            <NumberField
              label={text.offset}
              onChange={setOffsetText}
              onEnter={commitOffset}
              unit={unit}
              value={offsetText ?? lengthText(state.b.offset, units)}
            />
          </div>
        ) : null}
        {info.toggles.includes('pattern') ? (
          <div className="joint-creator__relief">
            <span className="joint-creator__relief-label">{text.pattern}</span>
            <div className="joint-creator__relief-buttons">
              {PATTERNS.map((pattern) => (
                <IconButton
                  description={text.patterns[pattern.value].description}
                  icon={pattern.icon}
                  key={pattern.value}
                  label={text.patterns[pattern.value].label}
                  onClick={() => setState((current) => ({ ...current, pattern: pattern.value }))}
                  pressed={state.pattern === pattern.value}
                  title={text.patterns[pattern.value].title}
                />
              ))}
            </div>
          </div>
        ) : null}
        {info.numbers.filter((n) => n.essential === true).map((n) => numberField(n.key))}
        {info.toggles.includes('fitsOneWay') ? (
          <CheckField
            checked={state.fitsOneWay}
            label={text.fitsOneWay}
            onChange={(fitsOneWay) => setState((current) => ({ ...current, fitsOneWay }))}
          />
        ) : null}
        <details
          className="joint-creator__more"
          onToggle={(event) => setMoreOpen(event.currentTarget.open)}
          open={moreOpen}
        >
          <summary>{text.more}</summary>
          <div className="joint-creator__more-body">
            {info.numbers.filter((n) => n.essential !== true).map((n) => numberField(n.key))}
            {info.toggles.includes('start') ? (
              <ChoiceField
                label={text.start}
                onChange={(start) => setState((current) => ({ ...current, start }))}
                options={[
                  { value: 'finger', label: text.starts.finger },
                  { value: 'gap', label: text.starts.gap },
                ]}
                value={state.start}
              />
            ) : null}
            {info.toggles.includes('ends') ? (
              <ChoiceField
                label={text.ends}
                onChange={(ends) => setState((current) => ({ ...current, ends }))}
                options={[
                  { value: 'square', label: text.endChoices.square },
                  { value: 'rounded', label: text.endChoices.rounded },
                ]}
                value={state.ends}
              />
            ) : null}
            {info.toggles.includes('through') ? (
              <CheckField
                checked={state.through}
                label={text.through}
                onChange={(through) => setState((current) => ({ ...current, through }))}
              />
            ) : null}
            {info.toggles.includes('blind') ? (
              <CheckField
                checked={state.blind}
                label={text.blind}
                onChange={(blind) => setState((current) => ({ ...current, blind }))}
              />
            ) : null}
            {settings !== null ? (
              <>
                {/* The joints are laid out for the settings' tool: the one
                    picked first is that size when the database has it. */}
                <ToolPicker
                  kinds={['endMill']}
                  onChange={setTool}
                  prefer="endMill"
                  preferDiameter={settings.toolDiameter}
                  tool={tool}
                  units={units}
                />
                <p className="joint-creator__small">
                  {text.toolDiameter(lengthText(settings.toolDiameter, units) + unit)}
                </p>
              </>
            ) : null}
          </div>
        </details>
      </fieldset>

      {settings !== null ? (
        <fieldset className="joint-creator__group" disabled={busy}>
          <legend className="joint-creator__legend">{text.settings}</legend>
          <p className="joint-creator__small">{text.settingsHint}</p>
          <HelperRow picture={<HelperPicture picture={jointFit} />}>
            <SelectField
              label={text.fit}
              onChange={(fit) => void saveSettings({ ...settings, fit })}
              options={FITS.map((fit) => ({ value: fit, label: fitLabel(fit, units) }))}
              value={settings.fit}
            />
            {settings.fit === 'custom' ? (
              <div className="joint-creator__clearance" onBlur={commitClearance}>
                <NumberField
                  label={text.clearance}
                  onChange={setClearanceText}
                  onEnter={commitClearance}
                  unit={unit}
                  value={clearanceText ?? lengthText(settings.customClearance, units)}
                />
              </div>
            ) : null}
          </HelperRow>
          <div className="joint-creator__relief">
            <span className="joint-creator__relief-label">{text.relief}</span>
            <div className="joint-creator__relief-buttons">
              {RELIEFS.map((relief) => (
                <IconButton
                  icon={relief.icon}
                  key={relief.value}
                  label={text.reliefs[relief.value]}
                  onClick={() => void saveSettings({ ...settings, relief: relief.value })}
                  pressed={settings.relief === relief.value}
                  title={text.reliefTitles[relief.value]}
                />
              ))}
            </div>
          </div>
          {sheetIds.map((sheet) => {
            const row = list?.sheets.find((s) => s.id === sheet);
            if (row === undefined) {
              return null;
            }
            return (
              <div
                className="joint-creator__thickness"
                key={sheet}
                onBlur={() => commitThickness(sheet)}
              >
                <NumberField
                  label={
                    (list?.sheets.length ?? 0) > 1 ? text.thicknessOf(row.name) : text.thickness
                  }
                  onChange={(value) =>
                    setThicknessText((current) => ({ ...current, [sheet]: value }))
                  }
                  onEnter={() => commitThickness(sheet)}
                  unit={unit}
                  value={thicknessText[sheet] ?? lengthText(row.thickness, units)}
                />
              </div>
            );
          })}
          <p className="joint-creator__small">{text.thicknessHint}</p>
        </fieldset>
      ) : null}

      {preview !== null && preview.solid.bToA !== null && info.target !== 'none' ? (
        <section className="joint-creator__view">
          <Suspense fallback={<p className="joint-creator__small">{text.viewLoading}</p>}>
            <JointView3D
              a={preview.solid.a}
              b={preview.solid.b}
              bToA={preview.solid.bToA}
              explode={explode}
              label={text.view3d}
            />
          </Suspense>
          <label className="joint-creator__explode" htmlFor={explodeId}>
            {text.explode}
            <input
              id={explodeId}
              max={1}
              min={0}
              onChange={(event) => setExplode(Number(event.target.value))}
              step={0.01}
              type="range"
              value={explode}
            />
          </label>
        </section>
      ) : null}

      <CheckField
        checked={state.makeToolpaths}
        disabled={tool === null}
        label={text.makeToolpaths}
        onChange={(makeToolpaths) => setState((current) => ({ ...current, makeToolpaths }))}
      />
      <div className="joint-creator__buttons">
        <IconButton
          why={createWhy}
          icon="jointCreate"
          label={state.editing === null ? text.create : text.apply}
          onClick={() => void create()}
          title={text.createTitle}
        />
        {state.editing !== null ? (
          <IconButton
            why={busyWhy}
            icon="editDelete"
            label={text.remove}
            onClick={() => void remove()}
            title={text.removeTitle}
          />
        ) : (
          <IconButton
            why={busyWhy}
            icon="jointCoupon"
            label={text.coupon}
            onClick={() => void coupon()}
            title={text.couponTitle}
          />
        )}
        <IconButton
          icon="dismiss"
          label={text.close}
          onClick={() => {
            clearHover();
            closeForm();
          }}
        />
      </div>

      <section className="joint-creator__list" aria-label={text.list}>
        <h3 className="joint-creator__list-title">{text.list}</h3>
        {list === null || list.joints.length === 0 ? (
          <p className="joint-creator__small">{text.listEmpty}</p>
        ) : (
          <ul className="joint-creator__rows">
            {list.joints.map((row) => (
              <li key={row.id}>
                <button
                  aria-pressed={state.editing === row.id}
                  className="joint-creator__row"
                  onClick={() => setState((current) => editState(row, units, current))}
                  type="button"
                >
                  {text.row(text.kinds[row.kind].name, row.id, row.ok ? '' : text.broken)}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
