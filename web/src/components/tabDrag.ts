/**
 * Moving a tab by dragging, same gesture as for the grid's columns: past a
 * threshold of a few pixels, pressing a tab becomes a move, a marker shows
 * where it will be inserted (`dropAt`), and releasing calls `onMove` with the
 * old and the new position.
 *
 * Used by the result tabs and the editor tabs. The tabs of a bar are the
 * `[role="tab"]` siblings of the grabbed tab.
 */

import { type Ref, ref } from 'vue'

import { MOVE_THRESHOLD, dropPosition, moveColumn } from './resultGrid'

/**
 * New order of a list after moving an item.
 *
 * @param items List in the current order.
 * @param from Position of the moved item.
 * @param to Insertion position, before the move.
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
 * Handling of the dragging of a bar's tabs.
 *
 * @param onMove Called on release, with the old position and the insertion position.
 */
export function useTabDrag(onMove: (from: number, to: number) => void): {
  dropAt: Ref<number | null>
  onDown: (event: PointerEvent, index: number) => void
  onMove: (event: PointerEvent) => void
  onDrop: () => void
} {
  /**
   * Move in progress: position of the grabbed tab, starting abscissa of the
   * pointer, element of the tab and pointer, and has the drag passed the
   * threshold?
   */
  let moving: {
    from: number
    x: number
    element: HTMLElement
    pointerId: number
    started: boolean
  } | null = null

  /**
   * Position where the moved tab would be inserted, `null` outside a move.
   */
  const dropAt = ref<number | null>(null)

  /**
   * Press on a tab: start of a possible move. The buttons and the renaming field
   * of the tab keep their own behavior.
   *
   * @param event Press on the tab.
   * @param index Position of the tab.
   */
  function onDown(event: PointerEvent, index: number): void {
    if (event.button !== 0 || (event.target as HTMLElement).closest('button, input')) {
      return
    }
    const element = event.currentTarget as HTMLElement
    moving = { from: index, x: event.clientX, element, pointerId: event.pointerId, started: false }
  }

  /**
   * Follows the pointer: past the threshold, the press becomes a move (the
   * pointer then stays captured by the tab) and the marker follows the pointer.
   *
   * @param event Pointer move.
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
   * Release: the tab takes its new place.
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
