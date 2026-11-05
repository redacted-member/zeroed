import {
  useEffect,
  useId,
  useRef,
  useState,
  type Dispatch,
  type JSX,
  type SetStateAction,
} from 'react';

import type { Units } from '../../bridge/types';
import { SPELL_CHECKED } from '../../lib/spelling';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useFonts } from '../../lib/useFonts';
import { usePrefsStore } from '../../state/prefsStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { NumberField } from '../NumberField/NumberField';
import { ResizableSelect, type SelectGroup } from '../ResizableSelect/ResizableSelect';
import type { ListSize } from '../ResizableSelect/model';
import {
  defaultFamily,
  filteredFamilies,
  initialFontFilter,
  shownRecentFonts,
  type FontFilter,
  withRecentFont,
  type TextAlignment,
  type TextSpecErrors,
  type TextSpecState,
} from './model';

import './TextSpecFields.scss';

const text = strings.text;

const FONT_FILTERS: ReadonlyArray<Choice<FontFilter>> = [
  { value: 'all', label: text.allFontsFilter },
  { value: 'singleLine', label: text.singleLine },
];

const ALIGNMENTS: ReadonlyArray<Choice<TextAlignment>> = [
  { value: 'left', label: text.left },
  { value: 'centre', label: text.centre },
  { value: 'right', label: text.right },
];

interface TextSpecFieldsProps {
  state: TextSpecState;
  setState: Dispatch<SetStateAction<TextSpecState>>;
  units: Units;
  errors: TextSpecErrors;
  /** Offer alignment, which only means something for straight text. */
  multiline?: boolean;
  /** Leave out the text height, for text sized to fill a box. */
  hideSize?: boolean;
  /**
   * Puts the keyboard focus in the text box each time this changes to a
   * value above zero: on opening, and after a click in the view places the
   * text, so typing goes into the text at once.
   */
  focusSerial?: number;
}

/** The text, font and spacing fields shared by the text forms. */
export function TextSpecFields({
  state,
  setState,
  units,
  errors,
  multiline = true,
  hideSize = false,
  focusSerial = 0,
}: TextSpecFieldsProps): JSX.Element {
  const areaRef = useRef<HTMLTextAreaElement | null>(null);
  const lineRef = useRef<HTMLInputElement | null>(null);
  useEffect(() => {
    if (focusSerial > 0) {
      (areaRef.current ?? lineRef.current)?.focus();
    }
  }, [focusSerial]);
  const fonts = useFonts();
  const families = fonts === null ? null : fonts.families;
  const singleLine = fonts === null ? [] : fonts.singleLine;
  const [filter, setFilter] = useState<FontFilter | null>(null);
  const shownFilter = filter ?? initialFontFilter(state.fontFamily, singleLine);
  const contentId = useId();
  const fontId = useId();

  useEffect(() => {
    if (families !== null && families.length > 0 && !families.includes(state.fontFamily)) {
      setState((current) => ({
        ...current,
        fontFamily: defaultFamily(families, current.fontFamily),
      }));
    }
  }, [families, setState, state.fontFamily]);

  const set = <K extends keyof TextSpecState>(key: K, value: TextSpecState[K]) =>
    setState((current) => ({ ...current, [key]: value }));

  const recent = usePrefsStore((store) => store.prefs.general.recentFonts);
  const listWidth = usePrefsStore((store) => store.prefs.window.fontListWidth);
  const listHeight = usePrefsStore((store) => store.prefs.window.fontListHeight);
  const updatePrefs = usePrefsStore((store) => store.update);
  const listed = filteredFamilies(families ?? [], singleLine, shownFilter);
  const all = listed.map((family) => ({ value: family, label: family }));
  const lately = shownRecentFonts(recent, listed).map((family) => ({
    value: family,
    label: family,
  }));
  const fontGroups: SelectGroup[] =
    lately.length === 0
      ? [{ options: all }]
      : [
          { label: text.recentFonts, options: lately },
          { label: text.allFonts, options: all },
        ];
  const pickFont = (family: string) => {
    set('fontFamily', family);
    void updatePrefs({ general: { recentFonts: withRecentFont(recent, family) } });
  };
  const changeFilter = (next: FontFilter) => {
    setFilter(next);
    const first = singleLine[0];
    if (next === 'singleLine' && !singleLine.includes(state.fontFamily) && first !== undefined) {
      pickFont(first);
    }
  };
  const resizeFontList = (size: ListSize | null) => {
    void updatePrefs({
      window: { fontListWidth: size?.width ?? 0, fontListHeight: size?.height ?? 0 },
    });
  };

  return (
    <fieldset className="text-spec">
      <legend className="text-spec__legend">{text.content}</legend>
      <label className="text-spec__label" htmlFor={contentId}>
        {text.content}
      </label>
      {multiline ? (
        <textarea
          aria-describedby={errors.text === undefined ? undefined : `${contentId}-error`}
          aria-invalid={errors.text !== undefined}
          className="text-spec__content"
          id={contentId}
          onChange={(event) => set('text', event.target.value)}
          ref={areaRef}
          rows={3}
          value={state.text}
          {...SPELL_CHECKED}
        />
      ) : (
        <input
          aria-describedby={errors.text === undefined ? undefined : `${contentId}-error`}
          aria-invalid={errors.text !== undefined}
          className="text-spec__content"
          id={contentId}
          onChange={(event) => set('text', event.target.value)}
          ref={lineRef}
          type="text"
          value={state.text}
          {...SPELL_CHECKED}
        />
      )}
      {errors.text === undefined ? null : (
        <p className="text-spec__error" id={`${contentId}-error`}>
          {errors.text}
        </p>
      )}
      <ChoiceField
        disabled={families === null}
        label={text.fontFilter}
        onChange={changeFilter}
        options={FONT_FILTERS.map((option) =>
          option.value === 'singleLine' ? { ...option, disabled: singleLine.length === 0 } : option,
        )}
        value={shownFilter}
      />
      <label className="text-spec__label" htmlFor={fontId}>
        {text.font}
      </label>
      <ResizableSelect
        disabled={families === null}
        groups={fontGroups}
        id={fontId}
        onChange={pickFont}
        onResize={resizeFontList}
        placeholder={families === null ? text.loadingFonts : ''}
        resizeLabel={text.resizeFontList}
        size={listWidth > 0 && listHeight > 0 ? { width: listWidth, height: listHeight } : null}
        value={state.fontFamily}
      />
      <div className="text-spec__styles">
        <CheckField checked={state.bold} label={text.bold} onChange={(v) => set('bold', v)} />
        <CheckField checked={state.italic} label={text.italic} onChange={(v) => set('italic', v)} />
      </div>
      {hideSize ? null : (
        <NumberField
          error={errors.size}
          label={text.height}
          onChange={(v) => set('size', v)}
          unit={unitSuffix(units)}
          value={state.size}
        />
      )}
      {multiline ? (
        <ChoiceField
          label={text.alignment}
          onChange={(v) => set('alignment', v)}
          options={ALIGNMENTS}
          value={state.alignment}
        />
      ) : null}
    </fieldset>
  );
}
