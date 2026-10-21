<!--
  Window of the settings, each applied as soon as it is changed: interface
  language, then those of the queries, default row limit (shared with the
  choice in each SQL cell) and read-only sessions, then the retention of the
  console's history and debug log, in days.
  The language offers French only for now: the interface stays in French
  until it is made multilingual (see `docs/QUESTIONS.md`), and the choice is
  not saved yet. The planned languages (English, Spanish, German, Italian)
  are listed, inactive.
  Window, fields and buttons of Prisme (`ui/`).
-->
<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'

import type { SettingsUpdate } from '@/api/generated/SettingsUpdate'
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
 * Retention of the console by default, in days.
 */
const DEFAULT_RETENTION = 30

/**
 * Bounds of a retention, in days: the server checks the same.
 */
const RETENTION_MIN = 1
const RETENTION_MAX = 3650

/**
 * Retentions as typed, applied when the field is left (`change`).
 */
const retention = reactive({
  history_retention_days: DEFAULT_RETENTION,
  debug_retention_days: DEFAULT_RETENTION,
})

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
      void change({ row_limit: value })
    }
  },
})

/**
 * Read-only mode of the query sessions, changed as soon as it is ticked.
 */
const readOnlySession = computed({
  get: () => settings.settings?.read_only_session ?? true,
  set: (value: boolean) => void change({ read_only_session: value }),
})

/**
 * Changes some settings on the server.
 *
 * @param update Settings to change.
 */
async function change(update: Partial<SettingsUpdate>): Promise<void> {
  error.value = null
  try {
    await settings.update(update)
  } catch (failure) {
    error.value = failure instanceof ApiError ? failure.message : String(failure)
  }
}

/**
 * Applies a typed retention, if it is within its bounds.
 *
 * @param field Retention typed.
 */
function changeRetention(field: keyof typeof retention): void {
  const days = retention[field]
  if (!Number.isInteger(days) || days < RETENTION_MIN || days > RETENTION_MAX) {
    error.value = `Rétention : entre ${RETENTION_MIN} et ${RETENTION_MAX.toLocaleString('fr-FR')} jours`
    return
  }
  void change({ [field]: days })
}

/**
 * Loads the settings each time the window opens, so that it shows the saved values.
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
  retention.history_retention_days = settings.settings?.history_retention_days ?? DEFAULT_RETENTION
  retention.debug_retention_days = settings.settings?.debug_retention_days ?? DEFAULT_RETENTION
})
</script>

<template>
  <ui-dialog
    v-model:open="open"
    title="Paramètres"
    description="Langue de l’interface, requêtes et conservation de la console"
  >
    <div class="flex flex-col gap-gap">
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
          v-model="rowLimit"
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
          <input v-model="readOnlySession" type="checkbox" class="size-icon accent-accent" />
          Sessions en lecture seule
        </label>
      </ui-field>
      <ui-field label="Rétention historique">
        <div class="flex items-center gap-gap">
          <ui-input
            v-model="retention.history_retention_days"
            type="number"
            :min="RETENTION_MIN"
            :max="RETENTION_MAX"
            class="w-20"
            aria-label="Jours de conservation de l’historique"
            title="Durée de conservation des requêtes de la console"
            @change="changeRetention('history_retention_days')"
          />
          <span class="text-ui text-fg-muted">jours</span>
        </div>
      </ui-field>
      <ui-field label="Rétention debug">
        <div class="flex items-center gap-gap">
          <ui-input
            v-model="retention.debug_retention_days"
            type="number"
            :min="RETENTION_MIN"
            :max="RETENTION_MAX"
            class="w-20"
            aria-label="Jours de conservation du debug"
            title="Durée de conservation des messages de débogage de la console"
            @change="changeRetention('debug_retention_days')"
          />
          <span class="text-ui text-fg-muted">jours</span>
        </div>
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
