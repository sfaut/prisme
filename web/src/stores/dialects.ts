/**
 * Dialects offered to the user, with their versions, read once from the
 * server (registry of the versions, `crates/dialects/versions/*.yaml`).
 */

import { defineStore } from 'pinia'
import { ref } from 'vue'

import { listDialects } from '@/api/dialects'
import type { DialectOption } from '@/api/generated/DialectOption'

export const useDialectsStore = defineStore('dialects', () => {
  /**
   * Dialects offered, empty before the first load.
   */
  const options = ref<DialectOption[]>([])

  /**
   * Reading in progress or done, shared by the callers.
   */
  let loading: Promise<void> | null = null

  /**
   * Reads the dialects, once; a failed reading is tried again on the next call.
   */
  function load(): Promise<void> {
    loading ??= listDialects().then(
      (dialects) => {
        options.value = dialects
      },
      (failure: unknown) => {
        loading = null
        throw failure
      },
    )
    return loading
  }

  return { options, load }
})
