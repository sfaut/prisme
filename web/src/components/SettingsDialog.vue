<!--
  Window of the settings, saved on the server: interface language, then those
  of the queries, default row limit (shared with the choice in each SQL cell),
  read-only sessions and the writes they allow, by kind (greyed out while the
  sessions are read-only: the database would refuse them), the indentation of
  the SQL editor, in spaces, then the retention of the console's history and
  debug log, in days.
  The language offers French only for now: the interface stays in French
  until it is made multilingual (see `docs/QUESTIONS.md`), and the choice is
  not saved yet. The planned languages (English, Spanish, German, Italian)
  are listed, inactive.
  Window, fields and buttons of Prisme (`ui/`).
-->
<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'

import { errorMessage } from '@/api/errorMessage'
import type { WriteKind } from '@/api/generated/WriteKind'
import { WRITE_KIND_GROUPS, WRITE_KIND_STATEMENTS, writeKindTooltip } from '@/api/writeKinds'
import { useSettingsStore } from '@/stores/settings'

import { DEFAULT_INDENT_SIZE } from './editorIndent'
import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import UiDialog from './ui/UiDialog.vue'
import UiField from './ui/UiField.vue'
import UiInput from './ui/UiInput.vue'
import UiSelect from './ui/UiSelect.vue'
import { vTooltip } from './ui/tooltip'

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
 * Bounds of the indentation, in spaces: the server checks the same.
 */
const INDENT_MIN = 1
const INDENT_MAX = 8

/**
 * Why the writes cannot be chosen while the sessions are read-only.
 */
const READ_ONLY_WRITES =
  'Sans effet en lecture seule : décocher « Sessions en lecture seule » pour autoriser des écritures'

/**
 * Current input: the row limit, the read-only mode, the allowed writes, the
 * indentation and the retentions.
 */
const form = reactive({
  row_limit: DEFAULT_ROW_LIMIT,
  read_only_session: true,
  allowed_writes: [] as WriteKind[],
  indent_size: DEFAULT_INDENT_SIZE,
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
    error.value = errorMessage(failure)
    return
  }
  form.row_limit = settings.settings?.row_limit ?? DEFAULT_ROW_LIMIT
  form.read_only_session = settings.settings?.read_only_session ?? true
  form.allowed_writes = [...(settings.settings?.allowed_writes ?? [])]
  form.indent_size = settings.settings?.indent_size ?? DEFAULT_INDENT_SIZE
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
    await settings.update({ ...form, allowed_writes: [...form.allowed_writes] })
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
    title="Paramètres"
    description="Langue de l’interface, requêtes, éditeur et conservation de la console"
  >
    <form id="settings-form" class="flex flex-col gap-3" @submit.prevent="save">
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
          class="flex items-center gap-2 text-ui text-fg-strong"
          v-tooltip="'La base refuse les écritures des sessions, en plus du contrôle de Prisme'"
        >
          <input v-model="form.read_only_session" type="checkbox" class="size-icon accent-accent" />
          Sessions en lecture seule
        </label>
      </ui-field>
      <div
        role="group"
        aria-labelledby="settings-writes"
        class="grid grid-cols-[--spacing(28)_minmax(0,1fr)] items-start gap-2"
      >
        <span id="settings-writes" class="text-fg-muted">Écritures</span>
        <fieldset
          :disabled="form.read_only_session"
          class="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-2 text-fg-strong disabled:opacity-50"
          v-tooltip="form.read_only_session ? READ_ONLY_WRITES : null"
        >
          <template v-for="group in WRITE_KIND_GROUPS" :key="group.label">
            <span class="text-fg-muted">{{ group.label }}</span>
            <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
              <label
                v-for="{ kind, verb } in group.kinds"
                :key="kind"
                class="flex items-center gap-1 font-mono"
                v-tooltip="form.read_only_session ? null : writeKindTooltip(kind)"
              >
                <input
                  v-model="form.allowed_writes"
                  type="checkbox"
                  :value="kind"
                  :aria-label="WRITE_KIND_STATEMENTS[kind]"
                  class="size-icon accent-accent"
                />
                {{ verb }}
              </label>
            </div>
          </template>
        </fieldset>
      </div>
      <ui-field label="Indentation">
        <div class="flex items-center gap-2">
          <ui-input
            v-model="form.indent_size"
            type="number"
            :min="INDENT_MIN"
            :max="INDENT_MAX"
            required
            class="w-20"
            aria-label="Espaces par niveau d’indentation"
            v-tooltip="
              'Espaces d’un niveau d’indentation dans l’éditeur SQL (Tab, Maj-Tab) et pour Enjoliver'
            "
          />
          <span class="text-ui text-fg-muted">espaces</span>
        </div>
      </ui-field>
      <ui-field label="Rétention historique">
        <div class="flex items-center gap-2">
          <ui-input
            v-model="form.history_retention_days"
            type="number"
            :min="RETENTION_MIN"
            :max="RETENTION_MAX"
            required
            class="w-20"
            aria-label="Jours de conservation de l’historique"
            v-tooltip="'Durée de conservation des requêtes de la console'"
          />
          <span class="text-ui text-fg-muted">jours</span>
        </div>
      </ui-field>
      <ui-field label="Rétention debug">
        <div class="flex items-center gap-2">
          <ui-input
            v-model="form.debug_retention_days"
            type="number"
            :min="RETENTION_MIN"
            :max="RETENTION_MAX"
            required
            class="w-20"
            aria-label="Jours de conservation du debug"
            v-tooltip="'Durée de conservation des messages de débogage de la console'"
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
