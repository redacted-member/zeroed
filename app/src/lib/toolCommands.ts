// Lets menus send a key command (the same letters the node tool answers to)
// into whatever tool the drawing view is running, without importing the
// view. The view registers its handler while mounted.

export type ToolCommandHandler = (key: string) => void;

let handler: ToolCommandHandler | null = null;

export function setToolCommandHandler(next: ToolCommandHandler | null): void {
  handler = next;
}

export function sendToolCommand(key: string): void {
  handler?.(key);
}
