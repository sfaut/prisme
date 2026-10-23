<!--
  Button of the interface.

  Three looks (`variant`): filled (`solid`), outlined (`outline`, the
  default) or discreet (`ghost`, without background: on hover, only its icon
  or its text darkens); three tones (`tone`): neutral (the default),
  action (`accent`) or danger (`error`). A Lucide icon goes on the left of the
  label; without a label, the button is square. During `loading`, a spinner
  replaces the icon and the button is inactive.

  Colors, height, padding and radius come from the theme's variables
  (`main.css`).
-->
<script setup lang="ts">
import { LoaderCircle } from '@lucide/vue'
import { type Component, computed } from 'vue'

const props = withDefaults(
  defineProps<{
    /**
     * Label; when absent, the button only shows its icon (provide `aria-label`).
     */
    label?: string
    /**
     * Lucide icon, on the left of the label.
     */
    icon?: Component
    /**
     * Look.
     */
    variant?: 'solid' | 'outline' | 'ghost'
    /**
     * Tone.
     */
    tone?: 'neutral' | 'accent' | 'error'
    /**
     * Small size (the navigator's "⋮" button).
     */
    small?: boolean
    /**
     * Action in progress: spinner instead of the icon, inactive button.
     */
    loading?: boolean
    /**
     * Inactive button.
     */
    disabled?: boolean
    /**
     * HTML type of the button.
     */
    type?: 'button' | 'submit'
  }>(),
  { variant: 'outline', tone: 'neutral', type: 'button' },
)

/**
 * Classes of each look, by tone. Written out so that Tailwind finds them in
 * the code.
 */
const LOOKS = {
  solid: {
    neutral: 'bg-fg-strong text-surface hover:bg-fg',
    accent: 'bg-accent text-on-accent hover:bg-accent-hover',
    error: 'bg-error text-on-accent hover:bg-error-hover',
  },
  outline: {
    neutral: 'border border-line bg-surface text-fg-strong hover:bg-surface-muted',
    accent: 'border border-accent-border text-accent hover:bg-accent-soft',
    error: 'border border-error-border text-error hover:bg-error-soft',
  },
  ghost: {
    neutral: 'text-fg-dimmed hover:text-fg-strong',
    accent: 'text-accent hover:text-accent-hover',
    error: 'text-error hover:text-error-hover',
  },
} as const

const classes = computed(() => [
  LOOKS[props.variant][props.tone],
  props.small ? 'h-control-sm' : 'h-control',
  props.label ? 'px-2' : props.small ? 'w-control-sm' : 'w-control',
])
</script>

<template>
  <button
    :type="type"
    :disabled="disabled || loading"
    class="inline-flex shrink-0 cursor-pointer items-center justify-center gap-2 rounded-control text-ui font-medium whitespace-nowrap transition-colors disabled:cursor-not-allowed disabled:opacity-50"
    :class="classes"
  >
    <LoaderCircle v-if="loading" class="size-icon shrink-0 animate-spin" />
    <component :is="icon" v-else-if="icon" class="size-icon shrink-0" />
    <span v-if="label" class="text-trim">{{ label }}</span>
  </button>
</template>
