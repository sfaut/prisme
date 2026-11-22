<!--
  Top bar of the application, across the whole width of the screen: name of
  the application with its version next to it (that of the executable, read
  from the server), on the left; on the right, the "Nouveau" button, the
  main action, filled in the primary color, which opens an editor on the
  connection of the active one (later, a menu: editor or notebook), exports
  manager (`ExportManager`), synchronisation state
  (`SyncStatus`), theme choice, and the account menu, headed by the user
  (initials, name, email address), which opens the user's profile
  (`ProfileDialog`), the connections manager (`ConnectionManager`) and the
  preferences, the settings (`PreferencesDialog`).

  The windows it opens are its own: the menu only chooses which one shows.
  The connections manager opens through its store, as the navigator opens
  it too.
-->
<script setup lang="ts">
import { Monitor, Moon, Plus, Server, Settings, Sun, User } from '@lucide/vue'
import { onMounted, ref } from 'vue'

import { getVersion } from '@/api/version'
import { type ColorMode, colorMode } from '@/utils/colorMode'
import { useConnectionsStore } from '@/stores/connections'
import { useEditorsStore } from '@/stores/editors'
import { useProfileStore } from '@/stores/profile'

import ExportManager from './ExportManager.vue'
import PreferencesDialog from './PreferencesDialog.vue'
import ProfileDialog from './ProfileDialog.vue'
import SyncStatus from './SyncStatus.vue'
import ConnectionManager from '@/components/connections/ConnectionManager.vue'
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
const connections = useConnectionsStore()

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
 * Profile window open or closed.
 */
const profileOpen = ref(false)

/**
 * Preferences window open or closed.
 */
const preferencesOpen = ref(false)

/**
 * Choices of the account menu, opened from the avatar.
 */
const menuItems: MenuItem[] = [
  { label: 'Profil', icon: User, onSelect: () => (profileOpen.value = true) },
  { label: 'Connexions', icon: Server, onSelect: () => connections.openManager() },
  { label: 'Préférences', icon: Settings, onSelect: () => (preferencesOpen.value = true) },
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
        <template v-if="profile.profile" #header>
          <!-- The user: avatar, name, then the email address unless it stands for the name. -->
          <div class="flex items-center gap-3">
            <span
              class="grid size-10 shrink-0 place-items-center rounded-full bg-primary/15 font-semibold text-primary"
              aria-hidden="true"
            >
              {{ profile.initials }}
            </span>
            <div class="flex min-w-0 flex-col">
              <span class="truncate font-medium text-fg-strong">{{ profile.displayName }}</span>
              <span
                v-if="profile.displayName !== profile.profile.email"
                class="truncate text-sm text-fg-secondary"
              >
                {{ profile.profile.email }}
              </span>
            </div>
          </div>
        </template>
      </ui-dropdown-menu>
    </div>
    <profile-dialog v-model:open="profileOpen" />
    <connection-manager />
    <preferences-dialog v-model:open="preferencesOpen" />
  </header>
</template>
