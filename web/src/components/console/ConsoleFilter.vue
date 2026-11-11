<!--
  Filter of a list of the console, on one field of its elements (action of
  a query, level or source of a line): a group of toggle buttons, one per
  value, after the name of the field. No button pressed, the list shows all
  the elements; buttons pressed, only those of their values.
-->
<script setup lang="ts" generic="V extends string">
import { ToggleGroupItem, ToggleGroupRoot } from 'reka-ui'

defineProps<{
  /**
   * Name of the field.
   */
  label: string
  /**
   * Values of the field, in the order of the buttons, with their labels.
   */
  labels: Record<V, string>
}>()

/**
 * Values kept, those of the buttons pressed.
 */
const model = defineModel<V[]>({ required: true })
</script>

<template>
  <div class="flex items-center gap-3">
    <span class="text-sm text-fg-secondary">{{ label }}</span>
    <toggle-group-root
      type="multiple"
      :model-value="model"
      class="flex flex-wrap items-center gap-1"
      :aria-label="label"
      @update:model-value="(values) => (model = values as V[])"
    >
      <toggle-group-item
        v-for="(text, value) in labels"
        :key="value"
        :value="value"
        class="flex h-7 cursor-pointer items-center rounded-lg border border-line px-1 text-sm font-medium whitespace-nowrap text-fg-secondary transition-colors select-none hover:bg-surface-sunken hover:text-fg-strong data-[state=on]:border-primary/50 data-[state=on]:bg-primary/15 data-[state=on]:text-primary"
      >
        <span class="text-trim">{{ text }}</span>
      </toggle-group-item>
    </toggle-group-root>
  </div>
</template>
