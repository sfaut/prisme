<!--
  Horizontal bar between the SQL editor (with its buttons) and the results,
  drawn with a line like the border of the left panel: dragging it changes
  the share of the height given to the editor, a double click resets it to
  the top third. A grip, in the middle, shows where to grab it. The share is
  remembered from one visit to the next
  (`editorSplit.ts`).
-->
<script setup lang="ts">
import { ref } from 'vue'

import { DEFAULT_EDITOR_SPLIT, clampEditorSplit, saveEditorSplit } from './editorSplit'

const props = defineProps<{
  /**
   * Shared element (the cell): its height is the reference of the share.
   */
  container: HTMLElement | null
}>()

/**
 * Share of the height given to the editor, from 0 to 1.
 */
const split = defineModel<number>({ required: true })

/**
 * Is a drag in progress?
 */
const dragging = ref(false)

/**
 * Starts the drag; the pointer stays captured even outside the bar.
 *
 * @param event Press on the bar.
 */
function start(event: PointerEvent): void {
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  dragging.value = true
}

/**
 * Follows the pointer: the bar goes to its height in the cell.
 *
 * @param event Pointer move.
 */
function move(event: PointerEvent): void {
  const box = props.container?.getBoundingClientRect()
  if (dragging.value && box && box.height > 0) {
    split.value = clampEditorSplit((event.clientY - box.top) / box.height)
  }
}

/**
 * Ends the drag and remembers the share.
 */
function stop(): void {
  if (dragging.value) {
    dragging.value = false
    saveEditorSplit(split.value)
  }
}

/**
 * Resets the default share.
 */
function reset(): void {
  split.value = DEFAULT_EDITOR_SPLIT
  saveEditorSplit(split.value)
}
</script>

<template>
  <div
    class="editor-resizer"
    :class="{ dragging }"
    role="separator"
    aria-orientation="horizontal"
    aria-label="Partage entre l'éditeur et les résultats"
    title="Glisser pour partager la hauteur, double clic pour revenir à la position initiale"
    @pointerdown.prevent="start"
    @pointermove="move"
    @pointerup="stop"
    @pointercancel="stop"
    @dblclick="reset"
  />
</template>

<style scoped>
.editor-resizer {
  position: relative;
  flex: none;
  block-size: var(--resize-handle-width);
  border-radius: var(--resize-handle-width);
  cursor: row-resize;
  touch-action: none;

  /* Line visible at rest, in the middle of the grab area. It is a border,
     like the edge of the left panel: the browser rounds its thickness to a
     whole number of screen pixels, whatever the display scaling (a box
     filled with 1 px could spread over two screen pixels and look
     thicker). */
  &::before {
    position: absolute;
    inset-inline: 0;
    inset-block-start: 50%;
    border-block-start: var(--line-width) solid var(--line);
    content: '';
  }

  /* Central grip, placed on the line. */
  &::after {
    position: absolute;
    inset-block-start: calc(50% - var(--grip-thickness) / 2);
    inset-inline-start: calc(50% - var(--grip-length) / 2);
    inline-size: var(--grip-length);
    block-size: var(--grip-thickness);
    border-radius: var(--grip-thickness);
    background: var(--grip-color);
    content: '';
  }

  &:hover,
  &.dragging {
    background: var(--resize-hover);

    &::after {
      background: var(--accent);
    }
  }
}
</style>
