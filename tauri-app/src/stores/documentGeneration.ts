import type { StateCreator } from 'zustand';

/**
 * Document identity for discarding late backend replies.
 *
 * The generation is bumped whenever a different document replaces the open
 * one. A reply to a request made for an earlier document must not touch the
 * new document's state, selection, undo or preview.
 */
export let documentGeneration = 0;

export const getDocumentGeneration = () => documentGeneration;

export function beginNewDocument(): void {
  documentGeneration += 1;
}

const STALE_DOCUMENT_MARKER = '[stale_document]';

/** Thrown in place of a reply that arrived after a different document opened. */
export class StaleDocumentError extends Error {
  constructor() {
    super(`${STALE_DOCUMENT_MARKER} The reply belongs to a document that is no longer open.`);
    this.name = 'StaleDocumentError';
  }
}

/** True for a StaleDocumentError or any message produced from one. */
export function isStaleDocument(error: unknown): boolean {
  if (error instanceof StaleDocumentError) return true;
  const text = typeof error === 'string' ? error : error instanceof Error ? error.message : '';
  return text.includes(STALE_DOCUMENT_MARKER);
}

/**
 * Wrap a service so every call fails with StaleDocumentError when a different
 * document opened while it was in flight. Calls that themselves replace the
 * document are listed in `unguarded`.
 */
export function guardDocumentReplies<T extends object>(
  service: T,
  unguarded: readonly (keyof T)[] = [],
): T {
  const skip = new Set<PropertyKey>(unguarded);
  return new Proxy(service, {
    get(target, property, receiver) {
      const value = Reflect.get(target, property, receiver) as unknown;
      if (typeof value !== 'function' || skip.has(property)) return value;
      return (...args: unknown[]) => {
        const generation = documentGeneration;
        return Promise.resolve((value as (...a: unknown[]) => unknown).apply(target, args)).then(
          (result) => {
            if (generation !== documentGeneration) throw new StaleDocumentError();
            return result;
          },
          (error: unknown) => {
            if (generation !== documentGeneration) throw new StaleDocumentError();
            throw error;
          },
        );
      };
    },
  });
}

/** Store wrapper: a late failure from a replaced document never sets `error`. */
export function dropStaleDocumentErrors<T extends { error?: unknown }>(
  creator: StateCreator<T>,
): StateCreator<T> {
  return (set, get, api) => {
    const guardedSet = ((partial: unknown, replace?: boolean) => {
      if (
        typeof partial === 'object' &&
        partial !== null &&
        'error' in partial &&
        isStaleDocument((partial as { error: unknown }).error)
      ) {
        return;
      }
      (set as (p: unknown, r?: boolean) => void)(partial, replace);
    }) as typeof set;
    return creator(guardedSet, get, api);
  };
}
