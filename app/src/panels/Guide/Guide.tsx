import { useEffect, useId, useRef, useState, type JSX } from 'react';

import { dialogsStrings } from '../../lib/strings/dialogs';
import { whyStrings } from '../../lib/strings/why';
import { whyTip } from '../../shell/Tooltip/model';
import { GUIDE_IDS, useGuideStore, type GuideId } from '../../state/guideStore';
import { GuideIcon } from '../GuideIcon/GuideIcon';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import {
  GUIDES,
  placeCard,
  ringBox,
  targetDock,
  targetForm,
  targetSelector,
  type Box,
  type GuideStep,
} from './model';

import './Guide.scss';

const text = dialogsStrings.guide;

/** How often the highlight follows its control as panels open and move, in ms. */
const FOLLOW_MS = 200;

/** The getting started guide: the welcome card, or the running walkthrough's step. */
export function Guide(): JSX.Element | null {
  const open = useGuideStore((state) => state.open);
  const guide = useGuideStore((state) => state.guide);
  const step = useGuideStore((state) => state.step);
  if (!open) {
    return null;
  }
  return guide === null ? <Welcome /> : <Walkthrough guide={guide} step={step} />;
}

function Welcome(): JSX.Element {
  const titleId = useId();
  const showAtStart = usePrefsStore((state) => state.prefs.general.showGuide);
  const update = usePrefsStore((state) => state.update);
  const { start, close } = useGuideStore.getState();
  return (
    <section aria-labelledby={titleId} className="guide guide__card guide__card--welcome">
      <h2 className="guide__title" id={titleId}>
        {text.welcomeTitle}
      </h2>
      <p className="guide__text">{text.welcomeText}</p>
      <ul aria-label={text.choices} className="guide__choices">
        {GUIDE_IDS.map((id) => (
          <li key={id}>
            <button
              className="guide__choice"
              data-tip={GUIDES[id].blurb}
              onClick={() => start(id)}
              type="button"
            >
              <GuideIcon large name={id} />
              <span className="guide__choice-text">
                <span className="guide__choice-title">{GUIDES[id].title}</span>
                <span className="guide__choice-blurb">{GUIDES[id].blurb}</span>
              </span>
            </button>
          </li>
        ))}
      </ul>
      <div className="guide__footer">
        <label className="guide__check" data-tip={text.dontShowTip}>
          <input
            checked={!showAtStart}
            onChange={(event) => void update({ general: { showGuide: !event.target.checked } })}
            type="checkbox"
          />
          {text.dontShow}
        </label>
        <button className="guide__button" data-tip={text.skipTip} onClick={close} type="button">
          <GuideIcon name="close" />
          {text.skip}
        </button>
      </div>
    </section>
  );
}

/** Shows the dock a step's control sits in, so the highlight can find it. */
function reveal(target: string | undefined): void {
  const dock = target === undefined ? null : targetDock(target);
  const ui = useUiStore.getState();
  if (dock === 'toolpaths' && !ui.toolpathsPinned && !ui.toolpathsOpen) {
    ui.showToolpaths();
  } else if (dock === 'design') {
    const form = ui.activeForm;
    if (ui.activeTab !== 'design' && form === null) {
      ui.setActiveTab('design');
    }
    if (!ui.leftPinned) {
      ui.setLeftOpen(true);
    }
  }
}

/** The box of the step's control on screen, followed as it moves; null when it is not showing. */
function useTargetBox(step: GuideStep): Box | null {
  const [box, setBox] = useState<Box | null>(null);
  useEffect(() => {
    const selector = step.target === undefined ? null : targetSelector(step.target);
    const measure = () => {
      const element = selector === null ? null : document.querySelector(selector);
      const rect = element?.getBoundingClientRect();
      const next =
        rect === undefined || rect.width === 0 || rect.height === 0
          ? null
          : { left: rect.left, top: rect.top, width: rect.width, height: rect.height };
      setBox((last) =>
        last === next ||
        (last !== null &&
          next !== null &&
          last.left === next.left &&
          last.top === next.top &&
          last.width === next.width &&
          last.height === next.height)
          ? last
          : next,
      );
    };
    measure();
    const timer = window.setInterval(measure, FOLLOW_MS);
    window.addEventListener('resize', measure);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener('resize', measure);
    };
  }, [step]);
  return box;
}

function Walkthrough({ guide, step }: { guide: GuideId; step: number }): JSX.Element {
  const { steps, title } = GUIDES[guide];
  const index = Math.min(step, steps.length - 1);
  const current = steps[index] ?? steps[0];
  const last = index === steps.length - 1;
  const backWhy = index === 0 ? whyStrings.why.firstPage : undefined;
  const { goTo, close, showWelcome } = useGuideStore.getState();
  const next = last ? close : () => goTo(index + 1);
  const box = useTargetBox(current);
  const cardRef = useRef<HTMLElement | null>(null);
  const [size, setSize] = useState({ width: 300, height: 180 });
  const [view, setView] = useState({ width: window.innerWidth, height: window.innerHeight });
  const titleId = useId();
  const activeForm = useUiStore((state) => state.activeForm);
  const formOpen = activeForm !== null;

  useEffect(() => reveal(current.target), [current]);

  // A step that asks for a tool is done once the tool's form is open.
  const opens =
    current.click === true && current.target !== undefined ? targetForm(current.target) : null;
  useEffect(() => {
    if (opens !== null && activeForm === opens) {
      goTo(index + 1);
    }
  }, [opens, activeForm, goTo, index]);

  // The card's size and the window's, to place the card beside its control.
  useEffect(() => {
    const card = cardRef.current;
    if (card === null) {
      return undefined;
    }
    const measure = () => {
      setSize((prev) =>
        prev.width === card.offsetWidth && prev.height === card.offsetHeight
          ? prev
          : { width: card.offsetWidth, height: card.offsetHeight },
      );
      setView((prev) =>
        prev.width === window.innerWidth && prev.height === window.innerHeight
          ? prev
          : { width: window.innerWidth, height: window.innerHeight },
      );
    };
    const observer = new ResizeObserver(measure);
    observer.observe(card);
    window.addEventListener('resize', measure);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', measure);
    };
  }, []);

  // A step that waits for a click on its control goes on once the click has run.
  useEffect(() => {
    const selector =
      current.click === true && current.target !== undefined && opens === null
        ? targetSelector(current.target)
        : null;
    if (selector === null) {
      return undefined;
    }
    const onClick = (event: MouseEvent) => {
      const control = document.querySelector(selector);
      if (control !== null && event.target instanceof Node && control.contains(event.target)) {
        window.setTimeout(() => {
          const guide = useGuideStore.getState();
          if (last) {
            guide.close();
          } else {
            guide.goTo(index + 1);
          }
        }, 0);
      }
    };
    document.addEventListener('click', onClick, true);
    return () => document.removeEventListener('click', onClick, true);
  }, [current, opens, index, last]);

  const at = placeCard(box, size, view);
  const ring = box === null ? null : ringBox(box, view);
  const missing =
    current.target === undefined || box !== null
      ? null
      : current.target === 'form'
        ? text.noForm
        : formOpen
          ? text.hiddenForm
          : text.hidden;

  return (
    <div className="guide">
      {ring !== null ? (
        <div
          aria-hidden="true"
          className="guide__ring"
          style={{ left: ring.left, top: ring.top, width: ring.width, height: ring.height }}
        />
      ) : null}
      <section
        aria-labelledby={titleId}
        className="guide__card"
        ref={cardRef}
        style={{ left: at.left, top: at.top }}
      >
        <p className="guide__progress">
          {title}: {text.step(index + 1, steps.length)}
        </p>
        <h2 className="guide__title" id={titleId}>
          {current.title}
        </h2>
        <p className="guide__text">{current.text}</p>
        {missing !== null ? <p className="guide__hint">{missing}</p> : null}
        {current.click === true && box !== null ? (
          <p className="guide__hint">{text.clickHint}</p>
        ) : null}
        <div className="guide__footer">
          <button
            className="guide__button"
            data-tip={text.menuTip}
            onClick={showWelcome}
            type="button"
          >
            <GuideIcon name="menu" />
            {text.menu}
          </button>
          <span className="guide__spacer" />
          <button
            className="guide__button"
            data-tip={whyTip(text.backTip, backWhy)}
            disabled={backWhy !== undefined}
            onClick={() => goTo(index - 1)}
            type="button"
          >
            <GuideIcon name="back" />
            {text.back}
          </button>
          <button
            className="guide__button guide__button--primary"
            data-tip={last ? text.doneTip : text.nextTip}
            onClick={next}
            type="button"
          >
            {last ? text.done : text.next}
            <GuideIcon name="next" />
          </button>
          <button className="guide__button" data-tip={text.closeTip} onClick={close} type="button">
            <GuideIcon name="close" />
            {text.close}
          </button>
        </div>
      </section>
    </div>
  );
}
