<!--
  Single-line input field.

  Binds with `v-model` to a text or a number: with a number (and
  `type="number"`), the input is returned as a number. An input that is not a
  number yet (empty, or `2.` on the way to `2.5`, which the browser reads as
  empty) leaves the model as it is: written back, it would erase the typing.
  The `required` attribute then stops the form. A Lucide icon can go
  on the left (`icon`), content on the right (`trailing` slot, for example a
  button to clear the field). A small size (`small`) takes the height and
  text of the small controls. The attributes (`placeholder`,
  `maxlength`, `aria-label`...) go to the field; `class` goes to the wrapper.
-->
<script setup lang="ts">
import type { Component } from 'vue'

import { INPUT_FIELD, INPUT_FIELD_SMALL } from './inputField'

defineOptions({ inheritAttrs: false })

defineProps<{
  /**
   * Lucide icon on the left of the text.
   */
  icon?: Component
  /**
   * Small size: height and text of the small controls; the icon keeps its
   * size, one step from the edge.
   */
  small?: boolean
}>()

/**
 * Typed value.
 */
const model = defineModel<string | number>({ required: true })

/**
 * Passes the input on to the model, as a number if the model is one, and only
 * once the input is a number.
 *
 * @param event Input in the field.
 */
function onInput(event: Event): void {
  const field = event.target as HTMLInputElement
  if (typeof model.value !== 'number') {
    model.value = field.value
  } else if (field.value !== '' && Number.isFinite(Number(field.value))) {
    model.value = Number(field.value)
  }
}
</script>

<template>
  <div :class="$attrs.class" class="relative flex items-center">
    <component
      :is="icon"
      v-if="icon"
      class="pointer-events-none absolute size-5 text-fg-faint"
      :class="small ? 'inset-s-1' : 'inset-s-2'"
    />
    <input
      v-bind="{ ...$attrs, class: undefined }"
      :value="model"
      :class="
        small
          ? [INPUT_FIELD_SMALL, { 'ps-7': icon, 'pe-8': $slots.trailing }]
          : [INPUT_FIELD, { 'ps-9': icon, 'pe-9': $slots.trailing }]
      "
      @input="onInput"
    />
    <div v-if="$slots.trailing" class="absolute inset-e-1 flex items-center">
      <slot name="trailing" />
    </div>
  </div>
</template>
