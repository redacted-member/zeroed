import type { KeyboardEvent } from 'react';

/**
 * Key handler for a modal dialog: keys typed in it are not drawing
 * shortcuts, and Escape closes it. The desktop web view does not always
 * turn Escape into the dialog's cancel event, so it is handled here.
 */
export function dialogKeyDown(close: () => void): (event: KeyboardEvent<HTMLElement>) => void {
  return (event) => {
    event.stopPropagation();
    if (event.key === 'Escape' && !event.defaultPrevented) {
      event.preventDefault();
      close();
    }
  };
}

/**
 * True while a modal dialog is open. Window listeners (the views,
 * sculpting, the shell's shortcuts) leave keys alone then, so Escape
 * reaches the dialog instead of cancelling a tool behind it and Delete
 * cannot reach the drawing. A dialog opened with `show()` works beside the
 * views and carries `data-modeless`, so it does not count.
 */
export function modalDialogOpen(): boolean {
  return document.querySelector('dialog[open]:not([data-modeless])') !== null;
}
