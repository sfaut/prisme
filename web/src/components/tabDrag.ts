/**
 * Déplacement d'un onglet par glissement, même geste que pour les colonnes
 * de la grille : passé un seuil de quelques pixels, l'appui sur un onglet
 * devient un déplacement, un repère montre où il sera inséré (`dropAt`), et
 * le lâcher appelle `onMove` avec l'ancienne et la nouvelle position.
 *
 * Sert aux onglets de résultat et aux onglets d'éditeur. Les onglets d'une
 * même barre sont les éléments `[role="tab"]` frères de l'onglet pris.
 */

import { type Ref, ref } from 'vue'

import { MOVE_THRESHOLD, dropPosition, moveColumn } from './resultGrid'

/**
 * Nouvel ordre d'une liste après le déplacement d'un élément.
 *
 * @param items Liste dans l'ordre actuel.
 * @param from Position de l'élément déplacé.
 * @param to Position d'insertion, avant le déplacement.
 */
export function movedItems<T>(items: T[], from: number, to: number): T[] {
  const moved = moveColumn(
    items.map((_, index) => index),
    from,
    to,
  )
  return moved.order.map((index) => items[index]!)
}

/**
 * Gestion du glissement des onglets d'une barre.
 *
 * @param onMove Appelée au lâcher, avec l'ancienne position et la position d'insertion.
 */
export function useTabDrag(onMove: (from: number, to: number) => void): {
  dropAt: Ref<number | null>
  onDown: (event: PointerEvent, index: number) => void
  onMove: (event: PointerEvent) => void
  onDrop: () => void
} {
  /**
   * Déplacement en cours : position de l'onglet pris, abscisse de départ du
   * pointeur, élément de l'onglet et pointeur, et le glissement a-t-il
   * dépassé le seuil ?
   */
  let moving: {
    from: number
    x: number
    element: HTMLElement
    pointerId: number
    started: boolean
  } | null = null

  /**
   * Position où l'onglet déplacé serait inséré, `null` hors déplacement.
   */
  const dropAt = ref<number | null>(null)

  /**
   * Appui sur un onglet : début d'un éventuel déplacement. Les boutons et le
   * champ de renommage de l'onglet gardent leur propre comportement.
   *
   * @param event Appui sur l'onglet.
   * @param index Position de l'onglet.
   */
  function onDown(event: PointerEvent, index: number): void {
    if (event.button !== 0 || (event.target as HTMLElement).closest('button, input')) {
      return
    }
    const element = event.currentTarget as HTMLElement
    moving = { from: index, x: event.clientX, element, pointerId: event.pointerId, started: false }
  }

  /**
   * Suit le pointeur : passé le seuil, l'appui devient un déplacement (le
   * pointeur reste alors capturé par l'onglet) et le repère suit le pointeur.
   *
   * @param event Déplacement du pointeur.
   */
  function onPointerMove(event: PointerEvent): void {
    if (moving === null) {
      return
    }
    if (!moving.started && Math.abs(event.clientX - moving.x) > MOVE_THRESHOLD) {
      moving.started = true
      moving.element.setPointerCapture(moving.pointerId)
    }
    if (moving.started) {
      const tabs = moving.element.parentElement?.querySelectorAll('[role="tab"]') ?? []
      const centers = Array.from(tabs, (tab) => {
        const rect = tab.getBoundingClientRect()
        return rect.left + rect.width / 2
      })
      dropAt.value = dropPosition(centers, event.clientX)
    }
  }

  /**
   * Lâcher : l'onglet prend sa nouvelle place.
   */
  function onDrop(): void {
    if (moving?.started && dropAt.value !== null) {
      onMove(moving.from, dropAt.value)
    }
    moving = null
    dropAt.value = null
  }

  return { dropAt, onDown, onMove: onPointerMove, onDrop }
}
