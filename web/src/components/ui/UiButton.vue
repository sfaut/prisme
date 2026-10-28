<!--
  Button of the interface.

  Three looks (`variant`): filled (`solid`), outlined (`outline`, the
  default) or discreet (`ghost`, without background: on hover, only its icon
  or its text darkens); three tones (`tone`): neutral (the default),
  action (`primary`) or danger (`error`). A Lucide icon goes on the left of the
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
    tone?: 'neutral' | 'primary' | 'error'
    /**
     * Small size (the navigator's "⋮" button): height, text, padding and gap
     * of the small controls. The icon keeps its size: an icon has the same
     * size wherever it appears, beside icons of the same size.
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
    primary: 'bg-primary text-on-primary hover:bg-primary-hover',
    error: 'bg-error text-on-primary hover:bg-error-hover',
  },
  outline: {
    neutral: 'border border-line bg-surface text-fg-strong hover:bg-surface-sunken',
    primary: 'border border-primary/50 text-primary hover:bg-primary/15',
    error: 'border border-error/50 text-error hover:bg-error/12',
  },
  ghost: {
    neutral: 'text-fg-faint hover:text-fg-strong',
    primary: 'text-primary hover:text-primary-hover',
    error: 'text-error hover:text-error-hover',
  },
} as const

const classes = computed(() => [
  LOOKS[props.variant][props.tone],
  props.small ? 'h-7 gap-1 text-sm' : 'h-8 gap-3 text-base',
  props.label ? (props.small ? 'px-1' : 'px-3') : props.small ? 'w-7' : 'w-8',
])
</script>

<template>
  <button
    :type="type"
    :disabled="disabled || loading"
    class="inline-flex shrink-0 cursor-pointer items-center justify-center rounded-lg font-medium whitespace-nowrap transition-colors disabled:cursor-not-allowed disabled:opacity-50"
    :class="classes"
  >
    <LoaderCircle v-if="loading" class="size-5 shrink-0 animate-spin" />
    <component :is="icon" v-else-if="icon" class="size-5 shrink-0" />
    <span v-if="label" class="text-trim">{{ label }}</span>
  </button>
</template>
