<!--
  Dropdown menu, on Reka UI's `DropdownMenu` (keyboard, positioning, closing
  with Escape or a click outside, accessibility). The trigger is the content
  of the default slot (a button); each choice has a label, a Lucide icon and
  an action, or is inactive.
-->
<script setup lang="ts">
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuPortal,
  DropdownMenuRoot,
  DropdownMenuTrigger,
} from 'reka-ui'

import type { MenuItem } from './menuItem'

import { cssPixels } from '@/cssLength'

/**
 * Gap with the trigger, in pixels (`--popup-offset` of the theme).
 *
 * Fixed size: Reka UI's `side-offset` only takes pixels, so this theme
 * length is read once (`docs/DEVELOPER.md`, section "Lengths in pixels").
 */
const OFFSET = cssPixels('--popup-offset', 4)

defineProps<{
  /**
   * Choices of the menu.
   */
  items: MenuItem[]
}>()
</script>

<template>
  <dropdown-menu-root>
    <dropdown-menu-trigger as-child>
      <slot />
    </dropdown-menu-trigger>
    <dropdown-menu-portal>
      <dropdown-menu-content
        align="end"
        :side-offset="OFFSET"
        class="z-50 min-w-53 rounded-xl border border-line bg-surface p-1 text-base shadow-lg"
      >
        <dropdown-menu-item
          v-for="item in items"
          :key="item.label"
          :disabled="item.disabled"
          class="flex h-8 cursor-pointer items-center gap-3 rounded-lg px-3 select-none data-disabled:cursor-not-allowed data-disabled:opacity-50 data-highlighted:bg-surface-raised"
          :class="item.tone === 'error' ? 'text-error' : 'text-fg data-highlighted:text-fg-strong'"
          @select="item.onSelect?.()"
        >
          <component :is="item.icon" class="size-5 shrink-0" />
          <span class="text-trim">{{ item.label }}</span>
        </dropdown-menu-item>
      </dropdown-menu-content>
    </dropdown-menu-portal>
  </dropdown-menu-root>
</template>
