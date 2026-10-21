<!--
  Window of the settings, saved on the server: interface language, then those
  of the queries, default row limit (shared with the choice in each SQL cell)
  and read-only sessions, then the retention of the console's history and
  debug log, in days.
  The language offers French only for now: the interface stays in French
  until it is made multilingual (see `docs/QUESTIONS.md`), and the choice is
  not saved yet. The planned languages (English, Spanish, German, Italian)
  are listed, inactive.
  Window, fields and buttons of Prisme (`ui/`).
-->
<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'

import { ApiError } from '@/api/http'
import { useSettingsStore } from '@/stores/settings'

import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import UiDialog from './ui/UiDialog.vue'
import UiField from './ui/UiField.vue'
import UiInput from './ui/UiInput.vue'
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
 * Default row limit and retention of the console, in days.
 */
const DEFAULT_ROW_LIMIT = 1000
const DEFAULT_RETENTION = 30

/**
 * Bounds of a retention, in days: the server checks the same.
 */
const RETENTION_MIN = 1
const RETENTION_MAX = 3650

/**
 * Current input: the row limit, the read-only mode and the retentions.
 */
const form = reactive({
  row_limit: DEFAULT_ROW_LIMIT,
  read_only_session: true,
  history_retention_days: DEFAULT_RETENTION,
  debug_retention_days: DEFAULT_RETENTION,
})

/**
 * Error of the last load or save, `null` if none.
 */
const error = ref<string | null>(null)

/**
 * A save is running.
 */
const saving = ref(false)

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
 * Loads the settings in the window each time it opens, so that it starts from the saved values.
 */
watch(open, async (isOpen) => {
  if (!isOpen) {
    return
  }
  error.value = null
  try {
    await settings.load()
  } catch (failure) {
    error.value = failure instanceof ApiError ? failure.message : String(failure)
    return
  }
  form.row_limit = settings.settings?.row_limit ?? DEFAULT_ROW_LIMIT
  form.read_only_session = settings.settings?.read_only_session ?? true
  form.history_retention_days = settings.settings?.history_retention_days ?? DEFAULT_RETENTION
  form.debug_retention_days = settings.settings?.debug_retention_days ?? DEFAULT_RETENTION
})

/**
 * Saves the settings, then closes the window.
 */
async function save(): Promise<void> {
  saving.value = true
  error.value = null
  try {
    await settings.update({ ...form })
    open.value = false
  } catch (failure) {
    error.value = failure instanceof ApiError ? failure.message : String(failure)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <ui-dialog
    v-model:open="open"
    title="Paramètres"
    description="Langue de l’interface, requêtes et conservation de la console"
  >
    <form id="settings-form" class="flex flex-col gap-field" @submit.prevent="save">
      <ui-field label="Langue">
        <ui-select
          v-model="language"
          :items="LANGUAGE_ITEMS"
          label="Langue de l’interface"
          class="w-40"
        />
      </ui-field>
      <ui-field label="Lignes">
        <ui-select
          v-model="form.row_limit"
          :items="rowLimitItems"
          label="Limite de lignes par défaut"
          class="w-40"
        />
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
      <ui-field label="Rétention historique">
        <div class="flex items-center gap-gap">
          <ui-input
            v-model="form.history_retention_days"
            type="number"
            :min="RETENTION_MIN"
            :max="RETENTION_MAX"
            required
            class="w-20"
            aria-label="Jours de conservation de l’historique"
            title="Durée de conservation des requêtes de la console"
          />
          <span class="text-ui text-fg-muted">jours</span>
        </div>
      </ui-field>
      <ui-field label="Rétention debug">
        <div class="flex items-center gap-gap">
          <ui-input
            v-model="form.debug_retention_days"
            type="number"
            :min="RETENTION_MIN"
            :max="RETENTION_MAX"
            required
            class="w-20"
            aria-label="Jours de conservation du debug"
            title="Durée de conservation des messages de débogage de la console"
          />
          <span class="text-ui text-fg-muted">jours</span>
        </div>
      </ui-field>
      <ui-alert v-if="error" tone="error" :title="error" />
    </form>
    <template #footer>
      <ui-button label="Annuler" @click="open = false" />
      <ui-button
        type="submit"
        form="settings-form"
        label="Enregistrer"
        variant="solid"
        tone="accent"
        :loading="saving"
        class="flex-1"
      />
    </template>
  </ui-dialog>
</template>
