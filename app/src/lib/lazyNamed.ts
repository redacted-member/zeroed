import { lazy, type ComponentType, type LazyExoticComponent } from 'react';

/**
 * React.lazy for a module that exports its component by name. Every call
 * with the same loader shares one chunk, so a module with two forms loads once.
 */
export function lazyNamed<K extends string, C extends ComponentType>(
  load: () => Promise<Record<K, C>>,
  name: K,
): LazyExoticComponent<C> {
  return lazy(async () => ({ default: (await load())[name] }));
}
