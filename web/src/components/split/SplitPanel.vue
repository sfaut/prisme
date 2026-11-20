<!--
  Panel of a splitter of Prisme (`splitLayout.ts`). Its
  size is a CSS length: its default size (a theme variable), or the size kept
  for it, in CSS pixels, as its line left it (`v-model:size`); folded, it
  takes its folded size exactly (`v-model:folded`, owned by the parent). The
  panel that fills (`fill`) takes the room left by the others. A panel
  without a default size, nor a size kept, shares that room with it in equal
  parts: the line that next moves it, or the panel beside, keeps its size
  first (`isShared`). Each one keeps its smallest size; a window too small
  shrinks the panel that fills first, then the others, which find their size
  again once it grows.
  A change of size that does not come from a drag animates quickly (folding,
  unfolding, the panel that fills taking or giving the room, a key on a
  line), with Tailwind's default duration and curve: the three parts of
  `flex` interpolate, a growth going from 1 to 0 included. During the drag
  of a line (`data-resizing` on the group), the panel follows the pointer
  without delay; with reduced motion asked, it changes size at once. Its
  content switches at once, between the rail and the open panel.
-->
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, useTemplateRef } from 'vue'

import { type SplitPanelEntry, useSplitGroup } from './splitLayout'

const props = defineProps<{
  /**
   * Id of the panel, named by the line that resizes it.
   */
  id: string
  /**
   * Does the panel fill the room left by the others?
   */
  fill?: boolean
  /**
   * Default size, a CSS length of the theme (`var(--navigator-width)`).
   */
  defaultSize?: string
  /**
   * Smallest size, open.
   */
  minSize?: string
  /**
   * Folded size; the panel folds only with one.
   */
  foldedSize?: string
}>()

/**
 * Size kept, in CSS pixels; `null`: the default size.
 */
const size = defineModel<number | null>('size', { default: null })

/**
 * Is the panel folded?
 */
const folded = defineModel<boolean>('folded', { default: false })

const group = useSplitGroup('SplitPanel')

const element = useTemplateRef<HTMLElement>('element')

/**
 * Size shown during a drag, in CSS pixels.
 */
const shown = ref<number | null>(null)

/**
 * Size of the panel along the group, with its smallest size.
 */
const style = computed(() => {
  const vertical = group.direction() === 'vertical'
  const minProperty = vertical ? 'minBlockSize' : 'minInlineSize'
  if (folded.value && props.foldedSize !== undefined) {
    return { flex: `0 0 ${props.foldedSize}`, [minProperty]: '0' }
  }
  const min = { [minProperty]: props.minSize ?? '0' }
  if (props.fill) {
    return { flex: '1 1 0', ...min }
  }
  const pixels = shown.value ?? size.value
  if (pixels === null && props.defaultSize === undefined) {
    return { flex: '1 1 0', ...min }
  }
  const basis = pixels === null ? props.defaultSize : `${pixels}px`
  return { flex: `0 1 ${basis}`, ...min }
})

const entry: SplitPanelEntry = {
  id: () => props.id,
  element: () => element.value,
  isFill: () => props.fill === true,
  isFolded: () => folded.value && props.foldedSize !== undefined,
  isShared: () =>
    !props.fill &&
    !(folded.value && props.foldedSize !== undefined) &&
    size.value === null &&
    props.defaultSize === undefined,
  minSize: () => props.minSize,
  foldedSize: () => props.foldedSize,
  showSize: (pixels) => {
    shown.value = pixels
  },
  keepSize: (pixels) => {
    shown.value = null
    size.value = pixels
  },
  fold: (value) => {
    folded.value = value
  },
}

onMounted(() => group.register(entry))
onBeforeUnmount(() => group.unregister(entry))
</script>

<template>
  <div :id="id" ref="element" class="split-panel" :style="style">
    <slot />
  </div>
</template>

<style scoped>
.split-panel {
  overflow: hidden;
  transition-property: flex-grow, flex-shrink, flex-basis;
  transition-duration: var(--default-transition-duration);
  transition-timing-function: var(--default-transition-timing-function);
}

[data-resizing] > .split-panel {
  transition: none;
}

@media (prefers-reduced-motion: reduce) {
  .split-panel {
    transition: none;
  }
}
</style>
