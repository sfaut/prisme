<!--
  Window of the user's profile: first and last names, the read-only mode of
  the query sessions and the preferred dialect (that of the editors which do
  not choose their own), saved on the server.
  The email address is shown but not editable: it identifies the user, and
  `prisme --setup` or `--user-email` change it.
  Window, fields and buttons of Prisme (`ui/`).
-->
<script setup lang="ts">
import { reactive, ref, watch } from 'vue'

import { ApiError } from '@/api/http'
import { useProfileStore } from '@/stores/profile'

import DialectSelect from './DialectSelect.vue'
import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import UiDialog from './ui/UiDialog.vue'
import UiField from './ui/UiField.vue'
import UiInput from './ui/UiInput.vue'

/**
 * Window open or closed.
 */
const open = defineModel<boolean>('open', { required: true })

const profile = useProfileStore()

/**
 * Current input: the names, as typed, and the settings.
 */
const form = reactive({
  first_name: '',
  last_name: '',
  read_only_session: true,
  preferred_dialect: 'source',
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
    error.value = failure instanceof ApiError ? failure.message : String(failure)
    return
  }
  form.first_name = profile.profile?.first_name ?? ''
  form.last_name = profile.profile?.last_name ?? ''
  form.read_only_session = profile.profile?.read_only_session ?? true
  form.preferred_dialect = profile.profile?.preferred_dialect ?? 'source'
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
    error.value = failure instanceof ApiError ? failure.message : String(failure)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <ui-dialog v-model:open="open" title="Profil" description="Nom, prénom et préférences">
    <form id="profile-form" class="flex flex-col gap-gap" @submit.prevent="save">
      <ui-field label="Nom">
        <ui-input v-model="form.last_name" :maxlength="100" autocomplete="family-name" />
      </ui-field>
      <ui-field label="Prénom">
        <ui-input v-model="form.first_name" :maxlength="100" autocomplete="given-name" />
      </ui-field>
      <ui-field label="Requêtes">
        <label
          class="flex items-center gap-gap text-ui text-fg-strong"
          title="La base refuse les écritures des sessions, en plus du contrôle de Prisme"
        >
          <input v-model="form.read_only_session" type="checkbox" class="size-icon accent-accent" />
          Sessions en lecture seule
        </label>
      </ui-field>
      <ui-field label="Dialecte préféré">
        <dialect-select v-model="form.preferred_dialect" label="Dialecte préféré" />
      </ui-field>
      <ui-field label="E-mail">
        <span class="truncate text-fg-muted" :title="email">{{ email }}</span>
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
