/**
 * Transient messages ("Sélection copiée"...), shown at the bottom right by
 * `UiToaster.vue` then removed after a few seconds. Each one also goes to
 * the debug log of the console. A message can offer an action ("Export prêt"
 * and its "Télécharger" button): it then stays until the user closes it or
 * runs the action.
 *
 * Any module adds one with `notify`, without a component to mount.
 */

import { ref } from 'vue'

import { logNotification } from '@/api/console'

/**
 * Action offered by a message: a button beside its text.
 */
export interface ToastAction {
  /**
   * Label of the button.
   */
  label: string
  /**
   * Another way to do the action, read by screen readers, as the message
   * cannot be reached with the keyboard ("Télécharger depuis le gestionnaire
   * d’exports").
   */
  alternative: string
  /**
   * Runs the action; the message closes.
   */
  run: () => void
}

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
  /**
   * Action offered, if any.
   */
  action?: ToastAction
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
 * @param action Action offered: the message then stays until closed.
 */
export function notify(title: string, tone: Toast['tone'] = 'success', action?: ToastAction): void {
  toasts.value = [...toasts.value, { id: nextId++, title, tone, action }]
  logNotification({ level: tone === 'error' ? 'error' : 'info', message: title })
}

/**
 * Removes a message, once its duration has elapsed or when the user closes it.
 *
 * @param id Id of the message.
 */
export function dismiss(id: number): void {
  toasts.value = toasts.value.filter((toast) => toast.id !== id)
}
