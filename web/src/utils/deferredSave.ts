/**
 * Deferred save of the documents of the display saved on the server side
 * (the display of an editor, of the user, of a result tab): a change is sent
 * shortly after the last one (a scroll moves at every frame), the whole
 * document at once. A save waiting for its delay goes out at once when the
 * page is hidden or closed, which would lose it, and on demand (`flush`),
 * before its owner closes for example.
 */

/**
 * Delay between the last change of a document and its save, in milliseconds.
 */
export const SAVE_DELAY = 300

/**
 * Deferred saves of documents, by key.
 */
export interface DeferredSave<Key> {
  /**
   * Saves the document of a key shortly after its last change.
   */
  schedule: (key: Key) => void
  /**
   * Saves at once the document of a key if a save is waiting for its delay.
   */
  flush: (key: Key) => Promise<void>
  /**
   * Drops the waiting save of a key.
   */
  cancel: (key: Key) => void
}

/**
 * Makes the deferred saves of documents.
 *
 * @param send
 *     Sends the document of a key; `keepalive` lets the request outlive the
 *     page, for a save made as it closes. A failure is left to the
 *     synchronisation state of the page: the next change saves again.
 */
export function deferredSave<Key>(
  send: (key: Key, keepalive: boolean) => Promise<void>,
): DeferredSave<Key> {
  const timers = new Map<Key, ReturnType<typeof setTimeout>>()

  async function save(key: Key, keepalive: boolean): Promise<void> {
    clearTimeout(timers.get(key))
    timers.delete(key)
    await send(key, keepalive).catch(() => {
      // Shown by the synchronisation state; the next change saves again.
    })
  }

  function schedule(key: Key): void {
    clearTimeout(timers.get(key))
    timers.set(
      key,
      setTimeout(() => void save(key, false), SAVE_DELAY),
    )
  }

  async function flush(key: Key): Promise<void> {
    if (timers.has(key)) {
      await save(key, false)
    }
  }

  function cancel(key: Key): void {
    clearTimeout(timers.get(key))
    timers.delete(key)
  }

  function flushPending(): void {
    for (const key of timers.keys()) {
      void save(key, true)
    }
  }

  // The page can close within the delay of a save: the waiting ones go out first.
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') {
      flushPending()
    }
  })
  window.addEventListener('pagehide', flushPending)

  return { schedule, flush, cancel }
}

/**
 * Removes the keys of the browser's local storage that match, left by
 * settings now saved on the server side; no effect if storage is unavailable.
 *
 * @param matches Tells whether a key is to be removed.
 */
export function removeStoredKeys(matches: (key: string) => boolean): void {
  try {
    for (const key of Object.keys(window.localStorage).filter(matches)) {
      window.localStorage.removeItem(key)
    }
  } catch {
    // Storage unavailable: nothing was kept.
  }
}
