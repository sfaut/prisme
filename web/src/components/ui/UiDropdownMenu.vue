<!--
  Dropdown menu, on Reka UI's `DropdownMenu` (keyboard, positioning, closing
  with Escape or a click outside, accessibility). The trigger is the content
  of the default slot (a button); each choice has a label, a Lucide icon and
  an action, or is inactive, and possibly an explanation in its tooltip. A
  choice can open a submenu of choices instead (`UiMenuEntries.vue`). The
  choices come as a list, or gathered in groups, each under its heading and
  apart from the previous one by a line. A group of exclusive choices, which
  tell which one is made (`checked`), is a group of radio choices
  (`DropdownMenuRadioGroup`). Closed, the menu gives the focus
  back to its trigger, unless the parent prevents it (`closeAutoFocus`), to
  give it to a field opened by a choice.
-->
<script setup lang="ts">
import {
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuPortal,
  DropdownMenuRadioGroup,
  DropdownMenuRoot,
  DropdownMenuTrigger,
} from 'reka-ui'

import { MENU_HEADING, MENU_PANEL, type MenuGroup, type MenuItem } from './menuItem'
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
   * Choices of the menu, without groups.
   */
  items?: MenuItem[]
  /**
   * Choices of the menu, gathered in groups, instead of `items`.
   */
  groups?: MenuGroup[]
}>()

const emit = defineEmits<{
  /**
   * The menu closes and gives the focus back to its trigger, unless the
   * event is prevented.
   */
  closeAutoFocus: [event: Event]
}>()

/**
 * Value of a group of radio choices: the label of the choice made, `''`
 * without one; `undefined` for a group of other choices.
 *
 * @param group Group of choices.
 */
function radioValue(group: MenuGroup): string | undefined {
  return group.items.some((item) => item.checked !== undefined)
    ? (group.items.find((item) => item.checked)?.label ?? '')
    : undefined
}
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
        :class="[MENU_PANEL, { 'flex flex-col gap-1': groups }]"
        @close-auto-focus="emit('closeAutoFocus', $event)"
      >
        <ui-menu-entries v-if="items" :items="items" />
        <component
          :is="radioValue(group) === undefined ? DropdownMenuGroup : DropdownMenuRadioGroup"
          v-for="(group, index) in groups"
          :key="group.label"
          v-bind="radioValue(group) === undefined ? {} : { modelValue: radioValue(group) }"
          :class="{ 'border-t border-line pt-1': index > 0 }"
        >
          <dropdown-menu-label :class="MENU_HEADING">{{ group.label }}</dropdown-menu-label>
          <ui-menu-entries :items="group.items" />
        </component>
      </dropdown-menu-content>
    </dropdown-menu-portal>
  </dropdown-menu-root>
</template>
