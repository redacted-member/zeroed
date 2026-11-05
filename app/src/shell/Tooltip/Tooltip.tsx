import { useEffect, useLayoutEffect, useRef, useState, type JSX } from 'react';

import { usePrefsStore } from '../../state/prefsStore';
import { boxAt, DISABLED_TIP, placeTip, TIP_DELAY_MS, tipAllowed } from './model';

import './Tooltip.scss';

/**
 * The one tooltip of the app. Any element with a data-tip attribute gets
 * it: shown after a short wait on hover or keyboard focus, placed inside
 * the window, gone on leaving, clicking or Esc. A disabled button shows
 * its tooltip too (often why it is off). It lives in the top layer
 * (a manual popover) so it shows over modal dialogs too. Screen readers
 * keep the control's own aria-label.
 */
export function Tooltip(): JSX.Element {
  const ref = useRef<HTMLDivElement>(null);
  const target = useRef<Element | null>(null);
  const [text, setText] = useState('');

  useEffect(() => {
    let timer = 0;
    // Moving straight from one tooltip to the next shows the next at once.
    let open = false;
    let closedAt = -Infinity;
    // A click holds the tooltips off until the pointer moves.
    let clickedSinceMove = false;
    const hide = () => {
      window.clearTimeout(timer);
      if (open) {
        open = false;
        closedAt = performance.now();
      }
      target.current = null;
      setText('');
    };
    const arm = (from: EventTarget | null, viaKeyboard = false) => {
      const next = from instanceof Element ? from.closest('[data-tip]') : null;
      if (next === target.current) {
        return;
      }
      hide();
      if (
        next === null ||
        !tipAllowed(clickedSinceMove, viaKeyboard) ||
        !usePrefsStore.getState().prefs.general.showToolTips
      ) {
        return;
      }
      target.current = next;
      const wait = performance.now() - closedAt < TIP_DELAY_MS ? 0 : TIP_DELAY_MS;
      timer = window.setTimeout(() => {
        open = true;
        setText(next.getAttribute('data-tip') ?? '');
      }, wait);
    };
    // A disabled control with a tooltip under the pointer, found from the
    // element the pointer is over: the control itself gets no events.
    const disabledAt = (event: PointerEvent): Element | null => {
      const over = event.target;
      if (!(over instanceof Element) || over.matches(DISABLED_TIP)) {
        return null;
      }
      const controls = Array.from(over.querySelectorAll(DISABLED_TIP));
      if (controls.length === 0) {
        return null;
      }
      const index = boxAt(
        controls.map((control) => control.getBoundingClientRect()),
        event.clientX,
        event.clientY,
      );
      return controls[index] ?? null;
    };
    const onOver = (event: PointerEvent) => arm(disabledAt(event) ?? event.target);
    const onMove = (event: PointerEvent) => {
      const disabled = disabledAt(event);
      if (clickedSinceMove) {
        clickedSinceMove = false;
        arm(disabled ?? event.target);
      } else if (disabled !== null || target.current?.matches(DISABLED_TIP) === true) {
        // Onto a disabled control, or off one, without leaving its holder.
        arm(disabled ?? event.target);
      }
    };
    const onOut = (event: PointerEvent) => {
      if (event.relatedTarget === null) {
        hide();
      }
    };
    const onFocus = (event: FocusEvent) => {
      if (event.target instanceof Element && event.target.matches(':focus-visible')) {
        arm(event.target, true);
      }
    };
    // A click or Esc ends the tooltip for good, not a move to the next one.
    const dismiss = () => {
      hide();
      closedAt = -Infinity;
      clickedSinceMove = true;
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        dismiss();
      }
    };
    const listeners: [string, EventListener][] = [
      ['pointerover', onOver as EventListener],
      ['pointermove', onMove as EventListener],
      ['pointerout', onOut as EventListener],
      ['focusin', onFocus as EventListener],
      ['focusout', hide],
      ['pointerdown', dismiss],
      ['wheel', hide],
      ['keydown', onKey as EventListener],
    ];
    for (const [type, listener] of listeners) {
      document.addEventListener(type, listener, true);
    }
    window.addEventListener('blur', hide);
    return () => {
      hide();
      for (const [type, listener] of listeners) {
        document.removeEventListener(type, listener, true);
      }
      window.removeEventListener('blur', hide);
    };
  }, []);

  useLayoutEffect(() => {
    const tip = ref.current;
    if (tip === null || typeof tip.showPopover !== 'function') {
      return;
    }
    if (text === '' || target.current === null) {
      if (tip.matches(':popover-open')) {
        tip.hidePopover();
      }
      return;
    }
    if (!tip.matches(':popover-open')) {
      tip.showPopover();
    }
    const spot = placeTip(
      target.current.getBoundingClientRect(),
      { width: tip.offsetWidth, height: tip.offsetHeight },
      { width: window.innerWidth, height: window.innerHeight },
    );
    // The computed spot of the tooltip, which no class can hold.
    tip.style.left = `${spot.left}px`;
    tip.style.top = `${spot.top}px`;
  }, [text]);

  return (
    <div className="tooltip" id="app-tooltip" popover="manual" ref={ref} role="tooltip">
      {text}
    </div>
  );
}
