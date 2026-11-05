import { useEffect, useRef, useState, type JSX, type KeyboardEvent } from 'react';

import './RenameField.scss';

interface RenameFieldProps {
  name: string;
  label: string;
  /** Called with the trimmed new name, or null when editing was cancelled or unchanged. */
  onDone: (name: string | null) => void;
}

/** Inline name editor for a list row: Enter or leaving commits, Escape cancels. */
export function RenameField({ name, label, onDone }: RenameFieldProps): JSX.Element {
  const [draft, setDraft] = useState(name);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const doneRef = useRef(false);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  const finish = (commit: boolean) => {
    if (doneRef.current) {
      return;
    }
    doneRef.current = true;
    const trimmed = draft.trim();
    onDone(commit && trimmed !== '' && trimmed !== name ? trimmed : null);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      finish(true);
    } else if (event.key === 'Escape') {
      event.stopPropagation();
      finish(false);
    }
  };

  return (
    <input
      aria-label={label}
      className="rename-field"
      onBlur={() => finish(true)}
      onChange={(event) => setDraft(event.target.value)}
      onKeyDown={onKeyDown}
      ref={inputRef}
      type="text"
      value={draft}
    />
  );
}
