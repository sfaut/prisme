<!--
  Top bar of the application, across the whole width of the screen: name of
  the application, theme choice, and the "Options" menu, which opens the
  settings (row limit, language) and the user's profile.

  The windows it opens are its own: the menu only chooses which one shows.
-->
<script setup lang="ts">
import { Monitor, Moon, Settings, Sun, UserRound } from 'lucide-vue-next'
import { ref } from 'vue'

import { type ColorMode, colorMode } from '@/colorMode'

import ProfileDialog from './ProfileDialog.vue'
import SettingsDialog from './SettingsDialog.vue'
import UiButton from './ui/UiButton.vue'
import UiDropdownMenu from './ui/UiDropdownMenu.vue'
import UiSelect from './ui/UiSelect.vue'
import type { MenuItem } from './ui/menuItem'

/**
 * Choice of the theme.
 */
const COLOR_MODES: { label: string; value: ColorMode; icon: typeof Monitor }[] = [
  { label: 'Système', value: 'system', icon: Monitor },
  { label: 'Clair', value: 'light', icon: Sun },
  { label: 'Sombre', value: 'dark', icon: Moon },
]

/**
 * Settings window open or closed.
 */
const settingsOpen = ref(false)

/**
 * Profile window open or closed.
 */
const profileOpen = ref(false)

/**
 * Choices of the "Options" menu.
 */
const menuItems: MenuItem[] = [
  { label: 'Paramètres', icon: Settings, onSelect: () => (settingsOpen.value = true) },
  { label: 'Profil', icon: UserRound, onSelect: () => (profileOpen.value = true) },
]
</script>

<template>
  <header class="flex items-center gap-gap border-b border-line px-panel py-1">
    <h1 class="text-heading font-semibold text-fg-strong">Prisme</h1>
    <div class="ms-auto flex items-center gap-gap">
      <ui-select v-model="colorMode" :items="COLOR_MODES" label="Thème" class="w-28" />
      <ui-dropdown-menu :items="menuItems">
        <ui-button :icon="Settings" variant="ghost" aria-label="Options" title="Options" />
      </ui-dropdown-menu>
    </div>
    <settings-dialog v-model:open="settingsOpen" />
    <profile-dialog v-model:open="profileOpen" />
  </header>
</template>
