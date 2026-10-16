/**
 * Transient messages ("Sélection copiée"...), shown at the bottom right by
 * `UiToaster.vue` then removed after a few seconds.
 *
 * Any module adds one with `notify`, without a component to mount.
 */

import { ref } from 'vue'

/**
 * A transient message.
 */
export interface Toast {
  /**
   * Id, for display in a list.
   */
  id: number
  /**
   * Text of the message.
   */
  title: string
  /**
   * Tone: success or error.
   */
  tone: 'success' | 'error'
}

/**
 * Messages being displayed.
 */
export const toasts = ref<Toast[]>([])

/**
 * Next id.
 */
let nextId = 1

/**
 * Shows a transient message.
 *
 * @param title Text of the message.
 * @param tone Tone of the message.
 */
export function notify(title: string, tone: Toast['tone'] = 'success'): void {
  toasts.value = [...toasts.value, { id: nextId++, title, tone }]
}

/**
 * Removes a message, once its duration has elapsed or when the user closes it.
 *
 * @param id Id of the message.
 */
export function dismiss(id: number): void {
  toasts.value = toasts.value.filter((toast) => toast.id !== id)
}
