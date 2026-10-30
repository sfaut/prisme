<!--
  Dropdown menu, on Reka UI's `DropdownMenu` (keyboard, positioning, closing
  with Escape or a click outside, accessibility). The trigger is the content
  of the default slot (a button); each choice has a label, a Lucide icon and
  an action, or is inactive, and possibly an explanation in its tooltip. A
  choice can open a submenu of choices instead (`UiMenuEntries.vue`).
-->
<script setup lang="ts">
import {
  DropdownMenuContent,
  DropdownMenuPortal,
  DropdownMenuRoot,
  DropdownMenuTrigger,
} from 'reka-ui'

import { MENU_PANEL, type MenuItem } from './menuItem'
import UiMenuEntries from './UiMenuEntries.vue'

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
      <dropdown-menu-content align="end" :side-offset="OFFSET" :class="MENU_PANEL">
        <ui-menu-entries :items="items" />
      </dropdown-menu-content>
    </dropdown-menu-portal>
  </dropdown-menu-root>
</template>
