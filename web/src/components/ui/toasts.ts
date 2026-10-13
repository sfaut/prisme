/**
 * Messages éphémères (« Sélection copiée »...), affichés en bas à droite
 * par `UiToaster.vue` puis retirés au bout de quelques secondes.
 *
 * N'importe quel module en ajoute un par `notify`, sans composant à monter.
 */

import { ref } from 'vue'

/**
 * Un message éphémère.
 */
export interface Toast {
  /**
   * Identifiant, pour l'affichage en liste.
   */
  id: number
  /**
   * Texte du message.
   */
  title: string
  /**
   * Ton : succès ou erreur.
   */
  tone: 'success' | 'error'
}

/**
 * Messages en cours d'affichage.
 */
export const toasts = ref<Toast[]>([])

/**
 * Prochain identifiant.
 */
let nextId = 1

/**
 * Affiche un message éphémère.
 *
 * @param title Texte du message.
 * @param tone Ton du message.
 */
export function notify(title: string, tone: Toast['tone'] = 'success'): void {
  toasts.value = [...toasts.value, { id: nextId++, title, tone }]
}

/**
 * Retire un message, une fois sa durée écoulée ou fermé par l'utilisateur.
 *
 * @param id Identifiant du message.
 */
export function dismiss(id: number): void {
  toasts.value = toasts.value.filter((toast) => toast.id !== id)
}
