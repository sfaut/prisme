<!--
  Window of the user's profile, saved on the server: the avatar of the
  initials with the name, as typed, then the first and last names. The email
  address is shown, not editable: it identifies the user, and
  `prisme --setup` or `--user-email` change it.

  Each field is a row, as in the preferences (`UiSettingRow`). Window,
  fields and buttons of Prisme (`ui/`).
-->
<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'

import { errorMessage } from '@/api/errorMessage'
import { initialsOf, useProfileStore } from '@/stores/profile'

import UiAlert from '@/components/ui/UiAlert.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiSettingList from '@/components/ui/UiSettingList.vue'
import UiSettingRow from '@/components/ui/UiSettingRow.vue'

/**
 * Window open or closed.
 */
const open = defineModel<boolean>('open', { required: true })

const profile = useProfileStore()

/**
 * Current input: the names, as typed.
 */
const names = reactive({
  first_name: '',
  last_name: '',
})

/**
 * Email address of the user.
 */
const email = ref('')

/**
 * Error of the last load or save, `null` if none.
 */
const error = ref<string | null>(null)

/**
 * A save is running.
 */
const saving = ref(false)

/**
 * Initials and full name, as typed: the avatar of the top bar once saved.
 */
const initials = computed(() => initialsOf({ email: email.value, ...names }))
const fullName = computed(() => `${names.first_name} ${names.last_name}`.trim())

/**
 * Loads the profile in the window each time it opens, so that it starts
 * from the saved names.
 */
watch(open, async (isOpen) => {
  if (!isOpen) {
    return
  }
  error.value = null
  try {
    await profile.load()
  } catch (failure) {
    error.value = errorMessage(failure)
    return
  }
  names.first_name = profile.profile?.first_name ?? ''
  names.last_name = profile.profile?.last_name ?? ''
  email.value = profile.profile?.email ?? ''
})

/**
 * Saves the names, then closes the window.
 */
async function save(): Promise<void> {
  saving.value = true
  error.value = null
  try {
    await profile.save({ ...names })
    open.value = false
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <ui-dialog
    v-model:open="open"
    title="Profil"
    description="Le nom que Prisme affiche dans la barre du haut."
  >
    <form id="profile-form" class="flex flex-col gap-4" @submit.prevent="save">
      <div class="flex items-center gap-3">
        <span
          class="grid size-12 shrink-0 place-items-center rounded-full bg-primary/15 text-lg font-semibold text-primary"
          aria-hidden="true"
        >
          {{ initials }}
        </span>
        <div class="flex min-w-0 flex-col">
          <span class="truncate font-medium text-fg-strong">{{ fullName || email }}</span>
          <span v-if="fullName" class="truncate text-sm text-fg-secondary">{{ email }}</span>
        </div>
      </div>
      <ui-setting-list>
        <ui-setting-row label="Prénom" for="profile-first-name">
          <ui-input
            id="profile-first-name"
            v-model="names.first_name"
            :maxlength="100"
            autocomplete="given-name"
            class="w-72"
          />
        </ui-setting-row>
        <ui-setting-row label="Nom" for="profile-last-name">
          <ui-input
            id="profile-last-name"
            v-model="names.last_name"
            :maxlength="100"
            autocomplete="family-name"
            class="w-72"
          />
        </ui-setting-row>
        <ui-setting-row label="Adresse e-mail">
          <template #description>
            Elle identifie votre compte, et se change au lancement de Prisme :
            <span class="font-mono">prisme --setup</span>.
          </template>
          <span class="text-fg-strong">{{ email }}</span>
        </ui-setting-row>
      </ui-setting-list>
      <ui-alert v-if="error" tone="error" :title="error" />
    </form>
    <template #footer>
      <div class="flex w-full justify-end gap-3">
        <ui-button label="Annuler" @click="open = false" />
        <ui-button
          type="submit"
          form="profile-form"
          label="Enregistrer"
          variant="solid"
          tone="primary"
          :loading="saving"
        />
      </div>
    </template>
  </ui-dialog>
</template>
