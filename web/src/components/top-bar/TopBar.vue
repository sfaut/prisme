<!--
  Top bar of the application, across the whole width of the screen: name of
  the application with its version next to it (that of the executable, read
  from the server), on the left; on the right, the "Nouveau" button, the
  main action, filled in the primary color, which opens an editor on the
  connection of the active one (later, a menu: editor or notebook), exports
  manager (`ExportManager`), synchronisation state
  (`SyncStatus`), theme choice, and the "Options" menu, which opens the
  settings (row limit, language) and the user's profile.

  The windows it opens are its own: the menu only chooses which one shows.
-->
<script setup lang="ts">
import { Monitor, Moon, Plus, Settings, Sun, User } from '@lucide/vue'
import { onMounted, ref } from 'vue'

import { getVersion } from '@/api/version'
import { type ColorMode, colorMode } from '@/utils/colorMode'
import { useEditorsStore } from '@/stores/editors'
import { useProfileStore } from '@/stores/profile'

import ExportManager from './ExportManager.vue'
import ProfileDialog from './ProfileDialog.vue'
import SyncStatus from './SyncStatus.vue'
import SettingsDialog from './SettingsDialog.vue'
import UiDropdownMenu from '@/components/ui/UiDropdownMenu.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiChoiceSwitch from '@/components/ui/UiChoiceSwitch.vue'
import type { MenuItem } from '@/components/ui/menuItem'

/**
 * Version of Prisme shown next to its name ("v0.2.0"), `null` as long as the
 * server has not given it.
 */
const version = ref<string | null>(null)

const profile = useProfileStore()
const editors = useEditorsStore()

onMounted(async () => {
  try {
    version.value = `v${(await getVersion()).version}`
  } catch {
    // Server unreachable: the version stays hidden, nothing else depends on it.
  }
})

onMounted(async () => {
  try {
    await profile.load()
  } catch {
    // Server unreachable: the avatar shows its icon instead of the initials.
  }
})

/**
 * Choice of the theme, "system" between the two forced ones.
 */
const COLOR_MODES: { label: string; value: ColorMode; icon: typeof Monitor }[] = [
  { label: 'Clair', value: 'light', icon: Sun },
  { label: 'Système', value: 'system', icon: Monitor },
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
 * Choices of the account menu, opened from the avatar.
 */
const menuItems: MenuItem[] = [
  { label: 'Profil', icon: User, onSelect: () => (profileOpen.value = true) },
  { label: 'Paramètres', icon: Settings, onSelect: () => (settingsOpen.value = true) },
]
</script>

<template>
  <header class="flex items-center justify-between gap-3 border-b border-line px-4 py-3">
    <!-- Name, then version in smaller type, on the same baseline. -->
    <div class="flex items-baseline gap-3">
      <h1 class="text-2xl leading-none font-semibold text-fg-strong">Prisme</h1>
      <span v-if="version" class="text-xs leading-none text-fg-faint">{{ version }}</span>
    </div>
    <div class="flex items-center gap-3">
      <ui-button
        :icon="Plus"
        label="Nouveau"
        variant="solid"
        tone="primary"
        wide
        @click="editors.create(editors.active?.connectionId ?? null)"
      />
      <export-manager />
      <sync-status />
      <ui-choice-switch v-model="colorMode" :items="COLOR_MODES" label="Thème" />
      <ui-dropdown-menu :items="menuItems">
        <!-- Round avatar with the user's initials, the user icon before they are known. -->
        <button
          type="button"
          class="grid size-8 shrink-0 cursor-pointer place-items-center rounded-full bg-primary/15 text-sm leading-none font-semibold text-primary select-none hover:bg-primary hover:text-on-primary data-[state=open]:bg-primary data-[state=open]:text-on-primary"
          aria-label="Compte"
        >
          <template v-if="profile.initials">{{ profile.initials }}</template>
          <user v-else class="size-4" />
        </button>
      </ui-dropdown-menu>
    </div>
    <settings-dialog v-model:open="settingsOpen" />
    <profile-dialog v-model:open="profileOpen" />
  </header>
</template>
