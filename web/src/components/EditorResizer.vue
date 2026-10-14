<!--
  Barre horizontale entre l'éditeur SQL (avec ses boutons) et les
  résultats, tracée d'un trait comme la bordure du panneau de gauche : la
  faire glisser change la part de la hauteur donnée à l'éditeur, un double
  clic la remet au tiers du haut. La part est retenue d'une visite à l'autre
  (`editorSplit.ts`).
-->
<script setup lang="ts">
import { ref } from 'vue'

import { DEFAULT_EDITOR_SPLIT, clampEditorSplit, saveEditorSplit } from './editorSplit'

const props = defineProps<{
  /**
   * Élément partagé (la cellule) : sa hauteur sert de référence à la part.
   */
  container: HTMLElement | null
}>()

/**
 * Part de la hauteur donnée à l'éditeur, de 0 à 1.
 */
const split = defineModel<number>({ required: true })

/**
 * Glissement en cours ?
 */
const dragging = ref(false)

/**
 * Commence le glissement ; le pointeur reste capturé même hors de la barre.
 *
 * @param event Appui sur la barre.
 */
function start(event: PointerEvent): void {
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  dragging.value = true
}

/**
 * Suit le pointeur : la barre se place à sa hauteur dans la cellule.
 *
 * @param event Déplacement du pointeur.
 */
function move(event: PointerEvent): void {
  const box = props.container?.getBoundingClientRect()
  if (dragging.value && box && box.height > 0) {
    split.value = clampEditorSplit((event.clientY - box.top) / box.height)
  }
}

/**
 * Termine le glissement et retient la part.
 */
function stop(): void {
  if (dragging.value) {
    dragging.value = false
    saveEditorSplit(split.value)
  }
}

/**
 * Remet la part par défaut.
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
    title="Glisser pour partager la hauteur, double clic pour revenir au tiers du haut"
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

  /* Trait visible au repos, au milieu de la zone de prise. C'est une
     bordure, comme le bord du panneau de gauche : le navigateur arrondit
     son épaisseur à un nombre entier de pixels d'écran, quelle que soit la
     mise à l'échelle de l'affichage (une boîte remplie de 1 px pourrait
     s'étaler sur deux pixels d'écran et paraître plus épaisse). */
  &::before {
    position: absolute;
    inset-inline: 0;
    inset-block-start: 50%;
    border-block-start: var(--line-width) solid var(--line);
    content: '';
  }

  &:hover,
  &.dragging {
    background: color-mix(in oklab, var(--accent) 30%, transparent);
  }
}
</style>
