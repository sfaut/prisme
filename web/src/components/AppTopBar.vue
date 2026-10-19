<!--
  Top bar of the application, across the whole width of the screen: name of
  the application with its version beneath (that of the executable, read
  from the server), synchronisation state
  (`SyncStatus`), theme choice, and the "Options" menu, which opens the
  settings (row limit, language) and the user's profile.

  The windows it opens are its own: the menu only chooses which one shows.
-->
<script setup lang="ts">
import { Monitor, Moon, Settings, Sun, UserRound } from 'lucide-vue-next'
import { onMounted, ref } from 'vue'

import { getVersion } from '@/api/version'
import { type ColorMode, colorMode } from '@/colorMode'

import ProfileDialog from './ProfileDialog.vue'
import SyncStatus from './SyncStatus.vue'
import SettingsDialog from './SettingsDialog.vue'
import UiButton from './ui/UiButton.vue'
import UiDropdownMenu from './ui/UiDropdownMenu.vue'
import UiSelect from './ui/UiSelect.vue'
import type { MenuItem } from './ui/menuItem'

/**
 * Version of Prisme shown beneath its name ("v0.2.0"), `null` as long as the
 * server has not given it.
 */
const version = ref<string | null>(null)

onMounted(async () => {
  try {
    version.value = `v${(await getVersion()).version}`
  } catch {
    // Server unreachable: the version stays hidden, nothing else depends on it.
  }
})

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
    <!-- Name and version without leading: together, the height of a control. -->
    <div class="flex flex-col gap-0.5">
      <h1 class="text-heading leading-none font-semibold text-fg-strong">Prisme</h1>
      <!-- A non-breaking space keeps the room of the version while waiting for it. -->
      <span class="text-small leading-none text-fg-dimmed" title="Version de Prisme">
        {{ version ?? '\u00a0' }}
      </span>
    </div>
    <div class="ms-auto flex items-center gap-gap">
      <sync-status />
      <ui-select v-model="colorMode" :items="COLOR_MODES" label="Thème" class="w-28" />
      <ui-dropdown-menu :items="menuItems">
        <ui-button :icon="Settings" variant="ghost" aria-label="Options" title="Options" />
      </ui-dropdown-menu>
    </div>
    <settings-dialog v-model:open="settingsOpen" />
    <profile-dialog v-model:open="profileOpen" />
  </header>
</template>
