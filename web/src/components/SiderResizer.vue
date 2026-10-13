<!--
  Poignée verticale posée sur le bord droit du panneau de gauche : la faire
  glisser change la largeur du panneau, un double clic la remet à sa valeur
  par défaut. La largeur est retenue d'une visite à l'autre (`siderWidth.ts`).
-->
<script setup lang="ts">
import { ref } from 'vue'

import { DEFAULT_SIDER_WIDTH, clampSiderWidth, saveSiderWidth } from './siderWidth'

/**
 * Largeur du panneau, en pixels.
 */
const width = defineModel<number>({ required: true })

/**
 * Glissement en cours : position de départ du pointeur et largeur de départ.
 */
const drag = ref<{ x: number; width: number } | null>(null)

/**
 * Commence le glissement ; le pointeur reste capturé même hors de la poignée.
 *
 * @param event Appui sur la poignée.
 */
function start(event: PointerEvent): void {
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  drag.value = { x: event.clientX, width: width.value }
}

/**
 * Suit le pointeur pendant le glissement.
 *
 * @param event Déplacement du pointeur.
 */
function move(event: PointerEvent): void {
  if (drag.value !== null) {
    width.value = clampSiderWidth(
      drag.value.width + event.clientX - drag.value.x,
      window.innerWidth,
    )
  }
}

/**
 * Termine le glissement et retient la largeur.
 */
function stop(): void {
  if (drag.value !== null) {
    drag.value = null
    saveSiderWidth(width.value)
  }
}

/**
 * Remet la largeur par défaut.
 */
function reset(): void {
  width.value = DEFAULT_SIDER_WIDTH
  saveSiderWidth(width.value)
}
</script>

<template>
  <div
    class="sider-resizer"
    :class="{ dragging: drag !== null }"
    :style="{ insetInlineStart: `${width - 3}px` }"
    title="Glisser pour élargir, double clic pour la largeur par défaut"
    @pointerdown.prevent="start"
    @pointermove="move"
    @pointerup="stop"
    @pointercancel="stop"
    @dblclick="reset"
  />
</template>

<style scoped>
.sider-resizer {
  position: absolute;
  inset-block: 0;
  z-index: 10;
  inline-size: 6px;
  cursor: col-resize;
  touch-action: none;

  &:hover,
  &.dragging {
    background: color-mix(in oklab, var(--accent) 30%, transparent);
  }
}
</style>
