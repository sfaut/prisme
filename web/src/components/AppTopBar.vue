<!--
  Top bar of the application, across the whole width of the screen: name of
  the application with its version next to it (that of the executable, read
  from the server), synchronisation state
  (`SyncStatus`), theme choice, and the "Options" menu, which opens the
  settings (row limit, language) and the user's profile.

  The windows it opens are its own: the menu only chooses which one shows.
-->
<script setup lang="ts">
import { Monitor, Moon, Settings, Sun, User } from 'lucide-vue-next'
import { computed, onMounted, ref } from 'vue'

import { getVersion } from '@/api/version'
import { type ColorMode, colorMode } from '@/colorMode'
import { useProfileStore } from '@/stores/profile'

import ProfileDialog from './ProfileDialog.vue'
import SyncStatus from './SyncStatus.vue'
import SettingsDialog from './SettingsDialog.vue'
import UiDropdownMenu from './ui/UiDropdownMenu.vue'
import UiSelect from './ui/UiSelect.vue'
import type { MenuItem } from './ui/menuItem'

/**
 * Version of Prisme shown next to its name ("v0.2.0"), `null` as long as the
 * server has not given it.
 */
const version = ref<string | null>(null)

const profile = useProfileStore()

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
 * Tooltip of the avatar: the user's name, once the profile is read.
 */
const accountTitle = computed(() =>
  profile.displayName === '' ? 'Compte' : `Compte : ${profile.displayName}`,
)

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
 * Choices of the account menu, opened from the avatar.
 */
const menuItems: MenuItem[] = [
  { label: 'Profil', icon: User, onSelect: () => (profileOpen.value = true) },
  { label: 'Paramètres', icon: Settings, onSelect: () => (settingsOpen.value = true) },
]
</script>

<template>
  <header class="flex items-center gap-2 border-b border-line px-3 py-1">
    <!-- Name, then version in smaller type, on the same baseline. -->
    <div class="flex items-baseline gap-2">
      <h1 class="text-heading leading-none font-semibold text-fg-strong">Prisme</h1>
      <span v-if="version" class="text-tiny leading-none text-fg-dimmed" title="Version de Prisme">
        {{ version }}
      </span>
    </div>
    <div class="ms-auto flex items-center gap-2">
      <sync-status />
      <ui-select v-model="colorMode" :items="COLOR_MODES" label="Thème" class="w-28" />
      <ui-dropdown-menu :items="menuItems">
        <!-- Round avatar with the user's initials, the user icon before they are known. -->
        <button
          type="button"
          class="grid size-control shrink-0 cursor-pointer place-items-center rounded-full bg-accent-soft text-small leading-none font-semibold text-accent select-none hover:bg-accent hover:text-on-accent data-[state=open]:bg-accent data-[state=open]:text-on-accent"
          aria-label="Compte"
          :title="accountTitle"
        >
          <template v-if="profile.initials">{{ profile.initials }}</template>
          <user v-else class="size-icon-sm" />
        </button>
      </ui-dropdown-menu>
    </div>
    <settings-dialog v-model:open="settingsOpen" />
    <profile-dialog v-model:open="profileOpen" />
  </header>
</template>
