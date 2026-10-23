<!--
  Single-line input field.

  Binds with `v-model` to a text or a number: with a number (and
  `type="number"`), the input is returned as a number. A Lucide icon can go
  on the left (`icon`), content on the right (`trailing` slot, for example a
  button to clear the field). The attributes (`placeholder`,
  `maxlength`, `aria-label`...) go to the field; `class` goes to the wrapper.
-->
<script setup lang="ts">
import type { Component } from 'vue'

defineOptions({ inheritAttrs: false })

defineProps<{
  /**
   * Lucide icon on the left of the text.
   */
  icon?: Component
}>()

/**
 * Typed value.
 */
const model = defineModel<string | number>({ required: true })

/**
 * Passes the input on to the model, as a number if the model is one.
 *
 * @param event Input in the field.
 */
function onInput(event: Event): void {
  const value = (event.target as HTMLInputElement).value
  model.value = typeof model.value === 'number' ? Number(value) : value
}
</script>

<template>
  <div :class="$attrs.class" class="relative flex items-center">
    <component
      :is="icon"
      v-if="icon"
      class="pointer-events-none absolute inset-s-2 size-icon text-fg-dimmed"
    />
    <input
      v-bind="{ ...$attrs, class: undefined }"
      :value="model"
      class="h-control w-full min-w-0 rounded-control border border-line bg-surface px-2 text-ui text-fg-strong [appearance:textfield] placeholder:text-fg-dimmed focus-visible:outline-(length:--focus-width) focus-visible:outline-offset-(--focus-offset) focus-visible:outline-focus-ring"
      :class="{ 'ps-7': icon, 'pe-7': $slots.trailing }"
      @input="onInput"
    />
    <div v-if="$slots.trailing" class="absolute inset-e-1 flex items-center">
      <slot name="trailing" />
    </div>
  </div>
</template>
