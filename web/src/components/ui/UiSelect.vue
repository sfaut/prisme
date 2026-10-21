<!--
  List of choices, on Reka UI's `Select` (keyboard, positioning of the list,
  accessibility). Each choice has a label, a value and, when needed, a Lucide
  icon, repeated in the button for the current choice.
-->
<script setup lang="ts" generic="T extends string | number">
import { Check, ChevronDown } from '@lucide/vue'
import {
  SelectContent,
  SelectIcon,
  SelectItem,
  SelectItemIndicator,
  SelectItemText,
  SelectPortal,
  SelectRoot,
  SelectTrigger,
  SelectViewport,
} from 'reka-ui'
import { type Component, computed } from 'vue'

import { cssPixels } from '@/cssLength'

/**
 * Gap with the trigger, in pixels (`--popup-offset` of the theme).
 */
const OFFSET = cssPixels('--popup-offset', 4)

const props = defineProps<{
  /**
   * Offered choices.
   */
  items: { label: string; value: T; icon?: Component; disabled?: boolean }[]
  /**
   * Name of the field, read by screen readers (`aria-label`).
   */
  label: string
}>()

/**
 * Chosen value; when absent, the button stays empty.
 */
const model = defineModel<T | undefined>()

/**
 * Current choice, for its label and its icon in the button.
 */
const current = computed(() => props.items.find((item) => item.value === model.value))
</script>

<template>
  <select-root v-model="model">
    <select-trigger
      :aria-label="label"
      class="inline-flex h-control cursor-pointer items-center gap-2 rounded-control border border-line bg-surface px-2 text-ui text-fg-strong hover:bg-surface-muted"
    >
      <component :is="current.icon" v-if="current?.icon" class="size-icon shrink-0" />
      <span class="min-w-0 flex-1 truncate text-start text-trim">{{ current?.label }}</span>
      <select-icon as-child>
        <ChevronDown class="size-icon shrink-0 text-fg-dimmed" />
      </select-icon>
    </select-trigger>
    <select-portal>
      <select-content
        position="popper"
        :side-offset="OFFSET"
        class="z-50 max-h-(--reka-select-content-available-height) min-w-(--reka-select-trigger-width) overflow-hidden rounded-panel border border-line bg-surface p-1 text-ui shadow-lg"
      >
        <select-viewport>
          <select-item
            v-for="item in items"
            :key="item.value"
            :value="item.value"
            :disabled="item.disabled"
            class="flex h-control cursor-pointer items-center gap-2 rounded-control px-2 text-fg outline-none select-none data-highlighted:bg-surface-elevated data-highlighted:text-fg-strong"
          >
            <component :is="item.icon" v-if="item.icon" class="size-icon shrink-0" />
            <select-item-text class="flex-1 text-trim">{{ item.label }}</select-item-text>
            <select-item-indicator>
              <Check class="size-icon text-accent" />
            </select-item-indicator>
          </select-item>
        </select-viewport>
      </select-content>
    </select-portal>
  </select-root>
</template>
