/**
 * Shared clock, to make the displayed durations run.
 *
 * A single timer for the whole interface, which only ticks while a component
 * uses it. Only the components that read the clock are redrawn on each tick.
 */

import { type Ref, onScopeDispose, ref } from 'vue'

/**
 * Interval between two ticks, in milliseconds: the durations and the numbers
 * of rows being received change once per second.
 */
const TICK = 1000

/**
 * Present instant (`performance.now()`, in milliseconds with a decimal part,
 * insensitive to changes of the machine's time), refreshed on each tick.
 */
const now = ref(performance.now())

/**
 * Number of components that use the clock.
 */
let users = 0

/**
 * Running timer, absent when nobody uses the clock.
 */
let timer: ReturnType<typeof setInterval> | undefined

/**
 * Returns the shared clock; it stops when the last component that uses it disappears.
 */
export function useClock(): Ref<number> {
  users++
  if (timer === undefined) {
    now.value = performance.now()
    timer = setInterval(() => (now.value = performance.now()), TICK)
  }
  onScopeDispose(() => {
    users--
    if (users === 0) {
      clearInterval(timer)
      timer = undefined
    }
  })
  return now
}
