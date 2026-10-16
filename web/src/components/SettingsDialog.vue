<!--
  Window of the settings: default row limit (shared with the choice in each
  SQL cell) and interface language. The language offers French only for now:
  the interface stays in French until it is made multilingual (see
  `docs/QUESTIONS.md`), and the choice is not saved yet. The planned languages
  (English, Spanish, German, Italian) are listed, inactive.
  Window, fields and buttons of Prisme (`ui/`).
-->
<script setup lang="ts">
import { computed, ref, watch } from 'vue'

import { ApiError } from '@/api/http'
import { useSettingsStore } from '@/stores/settings'

import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import UiDialog from './ui/UiDialog.vue'
import UiField from './ui/UiField.vue'
import UiSelect from './ui/UiSelect.vue'

/**
 * Window open or closed.
 */
const open = defineModel<boolean>('open', { required: true })

const settings = useSettingsStore()

/**
 * Language codes of the interface.
 */
type Language = 'fr' | 'en' | 'es' | 'de' | 'it'

/**
 * Language of the interface: French only for now. The other languages are
 * listed as planned, and cannot be chosen until they are translated.
 */
const language = ref<Language>('fr')

/**
 * Choices of the language, the planned ones inactive.
 */
const LANGUAGE_ITEMS: { label: string; value: Language; disabled?: boolean }[] = [
  { label: 'Français', value: 'fr' },
  { label: 'English', value: 'en', disabled: true },
  { label: 'Español', value: 'es', disabled: true },
  { label: 'Deutsch', value: 'de', disabled: true },
  { label: 'Italiano', value: 'it', disabled: true },
]

/**
 * Error of the last change, `null` if none.
 */
const error = ref<string | null>(null)

/**
 * Choices of the row limit, with their labels.
 */
const rowLimitItems = computed(() =>
  (settings.settings?.row_limit_choices ?? []).map((value) => ({
    label: `${value.toLocaleString('fr-FR')} lignes`,
    value,
  })),
)

/**
 * Default row limit, changed as soon as the user chooses it.
 */
const rowLimit = computed({
  get: () => settings.settings?.row_limit,
  set: (value: number | undefined) => {
    if (value !== undefined) {
      void changeRowLimit(value)
    }
  },
})

/**
 * Changes the default row limit on the server.
 *
 * @param value New limit, among the choices.
 */
async function changeRowLimit(value: number): Promise<void> {
  error.value = null
  try {
    await settings.setRowLimit(value)
  } catch (failure) {
    error.value = failure instanceof ApiError ? failure.message : String(failure)
  }
}

/**
 * Loads the settings each time the window opens, so that it shows the saved limit.
 */
watch(open, (isOpen) => {
  if (!isOpen) {
    return
  }
  error.value = null
  settings.load().catch((failure: unknown) => {
    error.value = failure instanceof ApiError ? failure.message : String(failure)
  })
})
</script>

<template>
  <ui-dialog v-model:open="open" title="Paramètres">
    <div class="flex flex-col gap-gap">
      <ui-field label="Lignes">
        <ui-select
          v-model="rowLimit"
          :items="rowLimitItems"
          label="Limite de lignes par défaut"
          class="w-40"
        />
      </ui-field>
      <ui-field label="Langue">
        <ui-select
          v-model="language"
          :items="LANGUAGE_ITEMS"
          label="Langue de l’interface"
          class="w-40"
        />
      </ui-field>
      <ui-alert v-if="error" tone="error" :title="error" />
    </div>
    <template #footer>
      <ui-button
        label="Fermer"
        variant="solid"
        tone="accent"
        class="flex-1"
        @click="open = false"
      />
    </template>
  </ui-dialog>
</template>
