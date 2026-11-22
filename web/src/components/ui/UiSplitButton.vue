<!--
  Split button: a main action and, joined on its right, a chevron that opens
  a menu of related actions (`UiDropdownMenu`). A click on the main part
  always does the main action; the menu only offers the others, the main one
  included, so that it can be found there too. Both parts are buttons of
  Prisme (`UiButton`), filled in the same tone, a line of the tone of their
  text between them; each part has its own label for assistive
  technologies.
-->
<script setup lang="ts">
import { ChevronDown } from '@lucide/vue'
import type { Component } from 'vue'

import type { MenuItem } from './menuItem'
import UiButton from './UiButton.vue'
import UiDropdownMenu from './UiDropdownMenu.vue'

defineProps<{
  /**
   * Label of the main action.
   */
  label: string
  /**
   * Lucide icon of the main action, on the left of its label.
   */
  icon?: Component
  /**
   * Choices of the menu.
   */
  items: MenuItem[]
  /**
   * Label of the chevron, for assistive technologies ("Choisir quoi créer").
   */
  menuLabel: string
}>()

const emit = defineEmits<{
  /**
   * The main part is clicked.
   */
  click: []
}>()
</script>

<template>
  <div class="flex">
    <ui-button
      :icon="icon"
      :label="label"
      variant="solid"
      tone="primary"
      wide
      class="rounded-e-none"
      @click="emit('click')"
    />
    <ui-dropdown-menu :items="items">
      <ui-button
        :icon="ChevronDown"
        variant="solid"
        tone="primary"
        :aria-label="menuLabel"
        class="rounded-s-none border-s border-on-primary/40"
      />
    </ui-dropdown-menu>
  </div>
</template>
