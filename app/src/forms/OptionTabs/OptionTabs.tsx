import { useId, useRef, useState, type JSX, type KeyboardEvent, type ReactNode } from 'react';

import './OptionTabs.scss';

export interface OptionTab<T extends string> {
  id: T;
  label: string;
  content: ReactNode;
}

interface OptionTabsProps<T extends string> {
  label: string;
  tabs: ReadonlyArray<OptionTab<T>>;
  initial?: T;
}

/** A row of tabs over one panel, for a form's secondary option pages. */
export function OptionTabs<T extends string>({
  label,
  tabs,
  initial,
}: OptionTabsProps<T>): JSX.Element | null {
  const base = useId();
  const [active, setActive] = useState<T | undefined>(initial ?? tabs[0]?.id);
  const buttons = useRef<Partial<Record<string, HTMLButtonElement | null>>>({});
  const current = tabs.find((tab) => tab.id === active) ?? tabs[0];
  if (current === undefined) {
    return null;
  }
  const onKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    const index = tabs.findIndex((tab) => tab.id === current.id);
    const count = tabs.length;
    const target =
      event.key === 'ArrowRight'
        ? tabs[(index + 1) % count]
        : event.key === 'ArrowLeft'
          ? tabs[(index + count - 1) % count]
          : event.key === 'Home'
            ? tabs[0]
            : event.key === 'End'
              ? tabs[count - 1]
              : undefined;
    if (target !== undefined) {
      event.preventDefault();
      setActive(target.id);
      buttons.current[target.id]?.focus();
    }
  };
  return (
    <div className="option-tabs">
      <div aria-label={label} className="option-tabs__list" role="tablist">
        {tabs.map((tab) => (
          <button
            aria-controls={`${base}-panel`}
            aria-selected={tab.id === current.id}
            className="option-tabs__tab"
            id={`${base}-${tab.id}`}
            key={tab.id}
            onClick={() => setActive(tab.id)}
            onKeyDown={onKey}
            ref={(element) => {
              buttons.current[tab.id] = element;
            }}
            role="tab"
            tabIndex={tab.id === current.id ? 0 : -1}
            type="button"
          >
            {tab.label}
          </button>
        ))}
      </div>
      <div
        aria-labelledby={`${base}-${current.id}`}
        className="option-tabs__panel"
        id={`${base}-panel`}
        role="tabpanel"
      >
        {current.content}
      </div>
    </div>
  );
}
