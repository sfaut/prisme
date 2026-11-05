<!--
  Choices of a dropdown menu (`UiDropdownMenu.vue`), drawn inside its
  content or inside one of its submenus: a choice with its own choices opens
  a submenu, on Reka UI's `DropdownMenuSub` (hover, arrow keys), which draws
  them the same way. A choice that tells whether it is the one made
  (`checked`) is a radio choice of its group (`DropdownMenuRadioItem`).
-->
<script setup lang="ts">
import { ChevronRight } from '@lucide/vue'
import {
  DropdownMenuItem,
  DropdownMenuPortal,
  DropdownMenuRadioItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from 'reka-ui'

import { MENU_ENTRY, MENU_PANEL, type MenuItem } from './menuItem'
import { vTooltip } from './tooltip'

defineProps<{
  /**
   * Choices to draw.
   */
  items: MenuItem[]
}>()
</script>

<template>
  <template v-for="item in items" :key="item.label">
    <dropdown-menu-sub v-if="item.items">
      <dropdown-menu-sub-trigger
        :disabled="item.disabled"
        :class="[
          MENU_ENTRY,
          'text-fg data-highlighted:text-fg-strong data-[state=open]:bg-surface-raised',
        ]"
        v-tooltip="item.hint"
      >
        <component :is="item.icon" class="size-5 shrink-0" />
        <span class="text-trim">{{ item.label }}</span>
        <chevron-right class="ms-auto size-5 shrink-0" />
      </dropdown-menu-sub-trigger>
      <dropdown-menu-portal>
        <dropdown-menu-sub-content :class="MENU_PANEL">
          <ui-menu-entries :items="item.items" />
        </dropdown-menu-sub-content>
      </dropdown-menu-portal>
    </dropdown-menu-sub>
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
      @select="item.onSelect?.()"
    >
      <component :is="item.icon" class="size-5 shrink-0" />
      <span class="text-trim">{{ item.label }}</span>
    </component>
  </template>
</template>
