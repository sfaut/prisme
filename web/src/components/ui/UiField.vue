<!--
  Form field: label on the left, in a narrow column (28 steps of the spacing scale),
  control on the right, over the whole remaining width.
  The label wraps the control: a click on it gives the focus to the field. With
  `top`, the label aligns with the top of the control (multi-line field).
-->
<script setup lang="ts">
defineProps<{
  /**
   * Label of the field.
   */
  label: string
  /**
   * Aligns the label with the top of the control instead of its middle.
   */
  top?: boolean
}>()
</script>

<template>
  <label
    class="grid grid-cols-[--spacing(37)_minmax(0,1fr)] gap-3"
    :class="top ? 'items-start' : 'items-center'"
  >
    <span class="text-fg-secondary" :class="{ 'pt-1': top }">{{ label }}</span>
    <slot />
  </label>
</template>
