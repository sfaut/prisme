<!--
  Window "Enregistrer dans un fichier", asked by the "⋮" menu of a database
  in memory in the navigator: the location of the new file where the session
  copies the database (`backup` of the session). The file must not exist; a
  location refused or a copy that fails shows as an error of the session.

  Open while a database waits for its location (`v-model:schema`); confirmed,
  it emits `save` with the location typed, and closes.
-->
<script setup lang="ts">
import { computed, ref, watch } from 'vue'

import { LENGTHS } from '@/components/connections/connectionForm'
import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiField from '@/components/ui/UiField.vue'
import UiInput from '@/components/ui/UiInput.vue'

/**
 * Schema of the database to copy, `null` if none: the window is open while
 * there is one.
 */
const schema = defineModel<string | null>('schema', { required: true })

const emit = defineEmits<{
  /**
   * The user asks to copy the database into the file at `location`.
   */
  save: [schema: string, location: string]
}>()

/**
 * Location typed, an absolute path on the machine of Prisme's server.
 */
const location = ref('')

// Each opening starts from an empty field.
watch(schema, () => {
  location.value = ''
})

/**
 * Is the window open?
 */
const open = computed({
  get: () => schema.value !== null,
  set: (isOpen) => {
    if (!isOpen) {
      schema.value = null
    }
  },
})

/**
 * Asks for the copy, and closes the window.
 */
function save(): void {
  const copied = schema.value
  const typed = location.value.trim()
  if (copied === null || typed === '') {
    return
  }
  schema.value = null
  emit('save', copied, typed)
}
</script>

<template>
  <ui-dialog
    v-model:open="open"
    title="Enregistrer dans un fichier"
    :description="`Copie la base en mémoire « ${schema} » dans un nouveau fichier.`"
  >
    <form class="flex flex-col gap-3" @submit.prevent="save">
      <ui-field label="Fichier">
        <ui-input
          v-model="location"
          :maxlength="LENGTHS.location"
          placeholder="D:\data\brouillon.db"
          spellcheck="false"
        />
      </ui-field>
      <p class="text-sm text-fg-secondary">
        Chemin absolu sur la machine du serveur de Prisme. Un fichier qui existe déjà est refusé.
      </p>
    </form>
    <template #footer>
      <div class="flex w-full justify-end gap-3">
        <ui-button label="Annuler" @click="schema = null" />
        <ui-button
          label="Enregistrer"
          variant="solid"
          tone="primary"
          :disabled="location.trim() === ''"
          @click="save"
        />
      </div>
    </template>
  </ui-dialog>
</template>
