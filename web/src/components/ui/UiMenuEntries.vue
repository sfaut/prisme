<!--
  Choices of a dropdown menu (`UiDropdownMenu.vue`), drawn inside its
  content or inside one of its submenus, as a list or gathered in groups, each
  under its heading and apart from the previous one by a line. A group of
  exclusive choices, which tell which one is made (`checked`), is a group of
  radio choices (`DropdownMenuRadioGroup`); a choice ticked or not (`ticked`)
  is a checkbox, which shows its box. A choice can show the current value of
  the setting it opens, on the right of its label.

  A choice with its own choices opens a submenu, on Reka UI's `DropdownMenuSub`
  (hover, arrow keys), which draws them the same way; a choice with a panel
  (`MenuPanel`) opens a submenu that holds fields, whose first field takes the
  focus. Such a submenu keeps the keys of its fields from the menu, which would
  otherwise move among its choices (arrows, Home, End) or close (Escape):
  Escape closes it alone and gives the focus back to its choice, and Tab and
  Shift+Tab go round its fields, which Reka UI keeps inside the menu. A
  submenu named by `openSub` is open from the start.
-->
<script setup lang="ts">
import { ChevronRight, Square, SquareCheck } from '@lucide/vue'
import {
  DropdownMenuCheckboxItem,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuPortal,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from 'reka-ui'
import { nextTick, reactive } from 'vue'

import { MENU_ENTRY, MENU_HEADING, MENU_PANEL, type MenuGroup, type MenuItem } from './menuItem'
import { vTooltip } from './tooltip'

const props = defineProps<{
  /**
   * Choices to draw, without groups.
   */
  items?: MenuItem[]
  /**
   * Choices to draw, gathered in groups, instead of `items`.
   */
  groups?: MenuGroup[]
  /**
   * Label of the choice whose submenu is open from the start.
   */
  openSub?: string
}>()

/**
 * Is the submenu of each choice open, by its label?
 */
const open = reactive<Record<string, boolean>>(
  props.openSub === undefined ? {} : { [props.openSub]: true },
)

/**
 * Elements that can take the focus with Tab, in their order.
 */
const FOCUSABLE = 'input, button, select, textarea, [tabindex]:not([tabindex="-1"])'

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

/**
 * Choice made: its action, the menu staying open if the choice asks it, or if it is a checkbox.
 *
 * @param event Choice of the entry, which closes the menu unless it is prevented.
 * @param item Choice.
 */
function select(event: Event, item: MenuItem): void {
  if (item.keepOpen === true || item.ticked !== undefined) {
    event.preventDefault()
  }
  item.onSelect?.()
}

/**
 * Opening of a submenu: one that holds fields gives the focus to the first one; the others let
 * Reka UI place it. Reka UI gives the focus to the submenu itself after this event when the
 * keyboard opens it: the field takes it once the event is over.
 *
 * @param event Focus of the opening.
 * @param item Choice of the submenu.
 */
function focusField(event: Event, item: MenuItem): void {
  const field = (event.target as HTMLElement).querySelector('input')
  if (item.panel !== undefined && field !== null) {
    void nextTick(() => field.focus())
  }
}

/**
 * Key pressed in a submenu that holds fields: the menu does not hear it. Tab and Shift+Tab go to
 * the next or the previous field, round; Escape closes the submenu alone and gives the focus back
 * to its choice.
 *
 * @param event Key pressed.
 * @param item Choice of the submenu.
 */
function keepKeys(event: KeyboardEvent, item: MenuItem): void {
  event.stopPropagation()
  const panel = event.currentTarget as HTMLElement
  if (event.key === 'Tab') {
    event.preventDefault()
    const fields = [...panel.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
      (field) => !field.matches(':disabled'),
    )
    const at = fields.findIndex((field) => field.contains(document.activeElement))
    const step = event.shiftKey ? -1 : 1
    fields[(at + step + fields.length) % fields.length]?.focus()
  } else if (event.key === 'Escape') {
    event.preventDefault()
    const content = panel.closest('[role="menu"]')
    const trigger = document.querySelector<HTMLElement>(`[aria-controls="${content?.id}"]`)
    open[item.label] = false
    trigger?.focus()
  }
}
</script>

<template>
  <template v-for="item in items" :key="item.label">
    <dropdown-menu-sub
      v-if="item.items || item.groups || item.panel"
      :open="open[item.label] ?? false"
      @update:open="open[item.label] = $event"
    >
      <dropdown-menu-sub-trigger
        :disabled="item.disabled"
        :class="[
          MENU_ENTRY,
          'text-fg data-highlighted:text-fg-strong data-[state=open]:bg-surface-raised',
        ]"
        v-tooltip="item.hint"
      >
        <component :is="item.icon" class="size-5 shrink-0" />
        <span class="min-w-0 flex-1 truncate text-trim">{{ item.label }}</span>
        <span v-if="item.value !== undefined" class="max-w-40 truncate text-fg-secondary text-trim">
          {{ item.value }}
        </span>
        <chevron-right class="size-5 shrink-0" />
      </dropdown-menu-sub-trigger>
      <dropdown-menu-portal>
        <dropdown-menu-sub-content
          :class="[MENU_PANEL, { 'flex flex-col gap-1': item.groups }]"
          @open-auto-focus="focusField($event, item)"
        >
          <!-- The keys of the fields stay in the submenu (`keepKeys`). -->
          <div v-if="item.panel" class="flex flex-col p-2" @keydown="keepKeys($event, item)">
            <component :is="item.panel.component" v-bind="item.panel.props" />
          </div>
          <ui-menu-entries v-else :items="item.items" :groups="item.groups" />
        </dropdown-menu-sub-content>
      </dropdown-menu-portal>
    </dropdown-menu-sub>
    <dropdown-menu-checkbox-item
      v-else-if="item.ticked !== undefined"
      :model-value="item.ticked"
      :disabled="item.disabled"
      :class="[MENU_ENTRY, 'text-fg data-highlighted:text-fg-strong']"
      v-tooltip="item.hint"
      @select="select($event, item)"
    >
      <component :is="item.ticked ? SquareCheck : Square" class="size-5 shrink-0" />
      <span class="text-trim">{{ item.label }}</span>
    </dropdown-menu-checkbox-item>
    <!-- A radio choice is one of its group, whose value is the label of the choice made. -->
    <component
      :is="item.checked === undefined ? DropdownMenuItem : DropdownMenuRadioItem"
      v-else
      v-bind="item.checked === undefined ? {} : { value: item.label }"
      :disabled="item.disabled"
      :class="[
        MENU_ENTRY,
        item.tone === 'error' ? 'text-error' : 'text-fg data-highlighted:text-fg-strong',
      ]"
      v-tooltip="item.hint"
      @select="select($event, item)"
    >
      <component :is="item.icon" class="size-5 shrink-0" />
      <span class="text-trim">{{ item.label }}</span>
    </component>
  </template>
  <component
    :is="radioValue(group) === undefined ? DropdownMenuGroup : DropdownMenuRadioGroup"
    v-for="(group, index) in groups"
    :key="group.label"
    v-bind="radioValue(group) === undefined ? {} : { modelValue: radioValue(group) }"
    :class="{ 'border-t border-line pt-1': index > 0 }"
  >
    <dropdown-menu-label :class="MENU_HEADING">{{ group.label }}</dropdown-menu-label>
    <ui-menu-entries :items="group.items" :open-sub="openSub" />
  </component>
</template>
