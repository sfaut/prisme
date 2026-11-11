<!--
  Panel of a splitter of Prisme (`splitLayout.ts`). Its
  size is a CSS length: its default size (a theme variable), or the size kept
  for it, in CSS pixels, as its line left it (`v-model:size`); folded, it
  takes its folded size exactly (`v-model:folded`, owned by the parent). The
  panel that fills (`fill`) takes the room left by the others. Each one keeps
  its smallest size; a window too small shrinks the panel that fills first,
  then the others, which find their size again once it grows.
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
  const basis = pixels === null ? (props.defaultSize ?? 'auto') : `${pixels}px`
  return { flex: `0 1 ${basis}`, ...min }
})

const entry: SplitPanelEntry = {
  id: () => props.id,
  element: () => element.value,
  isFill: () => props.fill === true,
  isFolded: () => folded.value && props.foldedSize !== undefined,
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
}
</style>
