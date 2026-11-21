<!--
  Dropdown menu, on Reka UI's `DropdownMenu` (keyboard, positioning, closing
  with Escape or a click outside, accessibility). The trigger is the content
  of the default slot (a button); each choice has a label, a Lucide icon and
  an action, or is inactive, and possibly an explanation in its tooltip. The
  choices come as a list, or gathered in groups (`UiMenuEntries.vue`, which
  also draws the radio choices, the checkboxes, the values of the settings
  and the submenus). A header (`header` slot) can come above the choices,
  set apart by a line from edge to edge: a label that the keyboard skips
  (the user of the account menu). The menu opens by its trigger, or by the code
  (`v-model:open`), possibly with the submenu of a choice open (`openSub`).
  Closed, the menu gives the focus back to its trigger, unless the parent
  prevents it (`closeAutoFocus`), to give it to a field opened by a choice.
  Removed while open, with the element of its trigger, it tells its parent
  that it is closed, which it could not do once removed.
-->
<script setup lang="ts">
import {
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuPortal,
  DropdownMenuRoot,
  DropdownMenuTrigger,
} from 'reka-ui'
import { onBeforeUnmount } from 'vue'

import { MENU_FRAME, MENU_PANEL, type MenuGroup, type MenuItem } from './menuItem'
import UiMenuEntries from './UiMenuEntries.vue'

import { cssPixels } from '@/utils/cssLength'

/**
 * Gap with the trigger, in pixels (`--popup-offset` of the theme).
 *
 * Fixed size: Reka UI's `side-offset` only takes pixels, so this theme
 * length is read once (`docs/DEVELOPER.md`, section "Lengths in pixels").
 */
const OFFSET = cssPixels('--popup-offset', 4)

defineProps<{
  /**
   * Choices of the menu, without groups.
   */
  items?: MenuItem[]
  /**
   * Choices of the menu, gathered in groups, instead of `items`.
   */
  groups?: MenuGroup[]
  /**
   * Label of the choice whose submenu is open when the menu opens.
   */
  openSub?: string
}>()

/**
 * Is the menu open?
 */
const open = defineModel<boolean>('open', { default: false })

const emit = defineEmits<{
  /**
   * The menu closes and gives the focus back to its trigger, unless the
   * event is prevented.
   */
  closeAutoFocus: [event: Event]
}>()

onBeforeUnmount(() => (open.value = false))
</script>

<template>
  <dropdown-menu-root v-model:open="open">
    <dropdown-menu-trigger as-child>
      <slot />
    </dropdown-menu-trigger>
    <dropdown-menu-portal>
      <dropdown-menu-content
        v-if="$slots.header"
        align="end"
        :side-offset="OFFSET"
        :class="[MENU_FRAME, 'flex max-w-96 flex-col']"
        @close-auto-focus="emit('closeAutoFocus', $event)"
      >
        <dropdown-menu-label class="border-b border-line p-4">
          <slot name="header" />
        </dropdown-menu-label>
        <div class="p-1" :class="{ 'flex flex-col gap-1': groups }">
          <ui-menu-entries :items="items" :groups="groups" :open-sub="openSub" />
        </div>
      </dropdown-menu-content>
      <dropdown-menu-content
        v-else
        align="end"
        :side-offset="OFFSET"
        :class="[MENU_PANEL, { 'flex flex-col gap-1': groups }]"
        @close-auto-focus="emit('closeAutoFocus', $event)"
      >
        <ui-menu-entries :items="items" :groups="groups" :open-sub="openSub" />
      </dropdown-menu-content>
    </dropdown-menu-portal>
  </dropdown-menu-root>
</template>
