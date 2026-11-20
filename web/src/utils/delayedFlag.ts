/**
 * Flag that follows another one only once it has stayed true for a delay.
 *
 * It tells a state that lasts: a notice of a work under way ("Calcul…") shows
 * only when the work is long, and short works repeated close together do not
 * make it blink. The flag falls back at once when its source does.
 */

import { type Ref, type WatchSource, onScopeDispose, readonly, ref, watch } from 'vue'

/**
 * Flag true once its source has stayed true for the delay, false as soon as
 * the source is false. Its timer stops with the scope that creates it (a
 * component, an `effectScope`).
 *
 * @param source State followed.
 * @param delayMs Time the source stays true before the flag follows it, in milliseconds.
 */
export function delayedFlag(source: WatchSource<boolean>, delayMs: number): Readonly<Ref<boolean>> {
  const flag = ref(false)
  let timer: ReturnType<typeof setTimeout> | null = null

  /**
   * Stops the timer under way.
   */
  function stop(): void {
    if (timer !== null) {
      clearTimeout(timer)
      timer = null
    }
  }

  watch(
    source,
    (now) => {
      stop()
      flag.value = false
      if (now) {
        timer = setTimeout(() => {
          timer = null
          flag.value = true
        }, delayMs)
      }
    },
    { immediate: true },
  )
  onScopeDispose(stop)
  return readonly(flag)
}
