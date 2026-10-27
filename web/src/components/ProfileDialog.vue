<!--
  Window of the user's profile: first and last names, saved on the server.
  The email address is shown but not editable: it identifies the user, and
  `prisme --setup` or `--user-email` change it.
  Window, fields and buttons of Prisme (`ui/`).
-->
<script setup lang="ts">
import { reactive, ref, watch } from 'vue'

import { errorMessage } from '@/api/errorMessage'
import { useProfileStore } from '@/stores/profile'

import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import UiDialog from './ui/UiDialog.vue'
import UiField from './ui/UiField.vue'
import UiInput from './ui/UiInput.vue'
import { vTooltip } from './ui/tooltip'

/**
 * Window open or closed.
 */
const open = defineModel<boolean>('open', { required: true })

const profile = useProfileStore()

/**
 * Current input: the names, as typed.
 */
const form = reactive({
  first_name: '',
  last_name: '',
})

/**
 * Email address of the user, shown above the names.
 */
const email = ref('')

/**
 * Error of the last save, `null` if none.
 */
const error = ref<string | null>(null)

/**
 * A save is running.
 */
const saving = ref(false)

/**
 * Loads the profile in the window each time it opens, so that it starts from the saved names.
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
  form.first_name = profile.profile?.first_name ?? ''
  form.last_name = profile.profile?.last_name ?? ''
  email.value = profile.profile?.email ?? ''
})

/**
 * Saves the names, then closes the window.
 */
async function save(): Promise<void> {
  saving.value = true
  error.value = null
  try {
    await profile.save({ ...form })
    open.value = false
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <ui-dialog v-model:open="open" title="Profil" description="Nom et prénom de l’utilisateur">
    <form id="profile-form" class="flex flex-col gap-4" @submit.prevent="save">
      <ui-field label="Nom">
        <ui-input v-model="form.last_name" :maxlength="100" autocomplete="family-name" />
      </ui-field>
      <ui-field label="Prénom">
        <ui-input v-model="form.first_name" :maxlength="100" autocomplete="given-name" />
      </ui-field>
      <ui-field label="E-mail">
        <span class="truncate text-fg-muted" v-tooltip="email">{{ email }}</span>
      </ui-field>
      <ui-alert v-if="error" tone="error" :title="error" />
    </form>
    <template #footer>
      <ui-button label="Annuler" @click="open = false" />
      <ui-button
        type="submit"
        form="profile-form"
        label="Enregistrer"
        variant="solid"
        tone="accent"
        :loading="saving"
        class="flex-1"
      />
    </template>
  </ui-dialog>
</template>
