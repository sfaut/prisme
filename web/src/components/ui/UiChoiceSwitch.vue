<!--
  Switch between two or three choices, on Reka UI's `RadioGroup` (arrow keys,
  roving focus, radio semantics): a pill-shaped track whose thumb slides
  under the checked choice. Each choice shows its Lucide icon alone; its
  label is its accessible name.

  The track has neither border nor padding: the choices are squares of the
  thumb's size, `size-8` like the buttons and the avatar of the top bar, and
  the thumb, a circle as large as a choice, carries the only line. Choices
  and icons (`size-4`) are multiples of 4 steps, whole screen pixels at 125%
  and 150% (`docs/DEVELOPER.md`, section "Lines"), so that the circle is
  drawn round. The thumb moves by whole widths, placed by `--switch-index`,
  the rank of the checked choice.
-->
<script setup lang="ts" generic="T extends string">
import { RadioGroupItem, RadioGroupRoot } from 'reka-ui'
import { type Component, computed } from 'vue'

const props = defineProps<{
  /**
   * Offered choices, in their order on screen.
   */
  items: { label: string; value: T; icon: Component }[]
  /**
   * Name of the group, read by screen readers (`aria-label`).
   */
  label: string
}>()

/**
 * Checked value.
 */
const model = defineModel<T>({ required: true })

/**
 * Rank of the checked choice, -1 when none is.
 */
const index = computed(() => props.items.findIndex((item) => item.value === model.value))
</script>

<template>
  <radio-group-root
    v-model="model"
    orientation="horizontal"
    :aria-label="label"
    class="relative flex rounded-full bg-surface-sunken"
    :style="{ '--switch-index': index }"
  >
    <span
      v-if="index >= 0"
      class="switch-thumb absolute start-0 top-0 size-8 rounded-full border border-line bg-surface"
      aria-hidden="true"
    />
    <radio-group-item
      v-for="item in items"
      :key="item.value"
      :value="item.value"
      :aria-label="item.label"
      class="relative grid size-8 cursor-pointer place-items-center rounded-full text-fg-secondary hover:text-fg-strong data-[state=checked]:text-primary"
    >
      <component :is="item.icon" class="size-4" aria-hidden="true" />
    </radio-group-item>
  </radio-group-root>
</template>

<style scoped>
@reference "@/assets/main.css";

/* Thumb under the checked choice: as wide as a choice, moved by whole widths. */
.switch-thumb {
  translate: calc(var(--switch-index) * 100%) 0;
  transition: translate 200ms ease-out;
}

@media (prefers-reduced-motion: reduce) {
  .switch-thumb {
    transition: none;
  }
}
</style>
