<!--
  Window of the preferences, saved on the server: the user's profile and the
  settings, in sections chosen in a list on the left (Reka UI's vertical
  `Tabs`: arrow keys, state read by screen readers).

  - Profil: first and last names; the email address is shown, not editable:
    it identifies the user, and `prisme --setup` or `--user-email` change it.
  - Langue: French only for now, the interface staying in French until it is
    made multilingual (see `docs/QUESTIONS.md`); the planned languages are
    listed, inactive, and the choice is not saved yet.
  - Requêtes: the default row limit (shared with the choice in each SQL
    cell), then the read-only sessions and the writes allowed by kind
    (greyed out while the sessions are read-only: the database would refuse
    them).
  - Éditeur SQL: the indentation, in spaces.
  - Pivoteur: the longest text of a `GROUP_CONCAT`, in bytes, and the
    shortest delay between two computations during the reception of the
    rows, in seconds (saved in milliseconds).
  - Console: the retention of the history and of the debug log, in days.

  Each setting is a row, its description in sight under its name
  (`UiSettingRow`). One save for the whole window: the profile, then the
  settings. Window, fields and buttons of Prisme (`ui/`).
-->
<script setup lang="ts">
import { Code, Database, Grid2x2, Languages, SquareTerminal, User } from '@lucide/vue'
import { TabsContent, TabsList, TabsRoot, TabsTrigger } from 'reka-ui'
import { type Component, computed, reactive, ref, watch } from 'vue'

import { errorMessage } from '@/api/errorMessage'
import type { WriteKind } from '@/api/generated/WriteKind'
import { initialsOf, useProfileStore } from '@/stores/profile'
import { useSettingsStore } from '@/stores/settings'

import PreferencesWrites from './PreferencesWrites.vue'
import { DEFAULT_INDENT_SIZE } from '@/components/editor/editorIndent'
import { DEFAULT_REFRESH_MS } from '@/components/pivot/pivotClient'
import { DEFAULT_CONCAT_MAX_BYTES } from '@/components/pivot/pivotEngine'
import UiAlert from '@/components/ui/UiAlert.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiSettingList from '@/components/ui/UiSettingList.vue'
import UiSettingRow from '@/components/ui/UiSettingRow.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'

/**
 * Window open or closed.
 */
const open = defineModel<boolean>('open', { required: true })

const profile = useProfileStore()
const settings = useSettingsStore()

/**
 * Sections of the window.
 */
type Section = 'profile' | 'language' | 'queries' | 'editor' | 'pivot' | 'console'

/**
 * Sections in the list, with their name, icon and description.
 */
const SECTIONS: { value: Section; label: string; icon: Component; description: string }[] = [
  {
    value: 'profile',
    label: 'Profil',
    icon: User,
    description: 'Le nom que Prisme affiche dans la barre du haut.',
  },
  {
    value: 'language',
    label: 'Langue',
    icon: Languages,
    description: 'La langue des menus, des boutons et des messages.',
  },
  {
    value: 'queries',
    label: 'Requêtes',
    icon: Database,
    description: 'Combien de lignes une requête affiche, et ce qu’elle a le droit de modifier.',
  },
  {
    value: 'editor',
    label: 'Éditeur SQL',
    icon: Code,
    description: 'La mise en forme du texte des requêtes.',
  },
  {
    value: 'pivot',
    label: 'Pivoteur',
    icon: Grid2x2,
    description: 'Le calcul des tableaux croisés.',
  },
  {
    value: 'console',
    label: 'Console',
    icon: SquareTerminal,
    description: 'Combien de temps la console garde ce qu’elle affiche.',
  },
]

/**
 * Section shown: the first at the first opening, then the last one chosen.
 */
const section = ref<Section>('profile')

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
 * Bounds of the longest text of a `GROUP_CONCAT`, in bytes: the server checks the same.
 */
const CONCAT_MIN = 100
const CONCAT_MAX = 1_048_576

/**
 * Bounds of the delay between two computations of the Pivoteur, in seconds,
 * and the step of its input: the server checks the same bounds, in
 * milliseconds.
 */
const REFRESH_MIN_SECONDS = 0.2
const REFRESH_MAX_SECONDS = 60
const REFRESH_STEP_SECONDS = 0.1

/**
 * Number of kinds of write, for the count of those allowed.
 */
const WRITE_KIND_COUNT = 25

/**
 * Current input of the profile: the names, as typed.
 */
const names = reactive({
  first_name: '',
  last_name: '',
})

/**
 * Email address of the user, shown in the profile.
 */
const email = ref('')

/**
 * Current input of the settings: the row limit, the read-only mode, the
 * allowed writes, the indentation, the longest text of a `GROUP_CONCAT` and
 * the retentions; the delay of the Pivoteur is typed in seconds
 * (`refreshSeconds`).
 */
const form = reactive({
  row_limit: DEFAULT_ROW_LIMIT,
  read_only_session: true,
  allowed_writes: [] as WriteKind[],
  indent_size: DEFAULT_INDENT_SIZE,
  concat_max_bytes: DEFAULT_CONCAT_MAX_BYTES,
  history_retention_days: DEFAULT_RETENTION,
  debug_retention_days: DEFAULT_RETENTION,
})

/**
 * Shortest delay between two computations of the Pivoteur during the
 * reception, as typed, in seconds.
 */
const refreshSeconds = ref(DEFAULT_REFRESH_MS / 1000)

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
 * Initials and full name of the profile, as typed: the avatar of the top bar
 * once saved.
 */
const initials = computed(() => initialsOf({ email: email.value, ...names }))
const fullName = computed(() => `${names.first_name} ${names.last_name}`.trim())

/**
 * What the writes allowed do, under their name: nothing while the sessions
 * are read-only, else how many there are.
 */
const writesDescription = computed(() => {
  if (form.read_only_session) {
    return 'Sans effet tant que les sessions sont en lecture seule : la base les refuserait.'
  }
  const count = form.allowed_writes.length
  return count === 0
    ? 'Aucune : Prisme refuse toute écriture.'
    : `${count} sur ${WRITE_KIND_COUNT}. Prisme refuse celles qui ne sont pas cochées.`
})

/**
 * Loads the profile and the settings in the window each time it opens, so
 * that it starts from the saved values.
 */
watch(open, async (isOpen) => {
  if (!isOpen) {
    return
  }
  error.value = null
  try {
    await Promise.all([profile.load(), settings.load()])
  } catch (failure) {
    error.value = errorMessage(failure)
    return
  }
  names.first_name = profile.profile?.first_name ?? ''
  names.last_name = profile.profile?.last_name ?? ''
  email.value = profile.profile?.email ?? ''
  form.row_limit = settings.settings?.row_limit ?? DEFAULT_ROW_LIMIT
  form.read_only_session = settings.settings?.read_only_session ?? true
  form.allowed_writes = [...(settings.settings?.allowed_writes ?? [])]
  form.indent_size = settings.settings?.indent_size ?? DEFAULT_INDENT_SIZE
  form.concat_max_bytes = settings.settings?.concat_max_bytes ?? DEFAULT_CONCAT_MAX_BYTES
  form.history_retention_days = settings.settings?.history_retention_days ?? DEFAULT_RETENTION
  form.debug_retention_days = settings.settings?.debug_retention_days ?? DEFAULT_RETENTION
  refreshSeconds.value = (settings.settings?.pivot_refresh_ms ?? DEFAULT_REFRESH_MS) / 1000
})

/**
 * Saves the profile, then the settings, then closes the window.
 */
async function save(): Promise<void> {
  saving.value = true
  error.value = null
  try {
    await profile.save({ ...names })
    await settings.update({
      ...form,
      allowed_writes: [...form.allowed_writes],
      pivot_refresh_ms: Math.round(refreshSeconds.value * 1000),
    })
    open.value = false
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <ui-dialog v-model:open="open" title="Préférences" wide>
    <form id="preferences-form" class="flex min-h-0 flex-1 flex-col gap-4" @submit.prevent="save">
      <tabs-root v-model="section" orientation="vertical" class="flex min-h-0 flex-1 gap-6">
        <tabs-list
          class="flex w-45 shrink-0 flex-col gap-1 border-e border-line pe-3"
          aria-label="Sections des préférences"
        >
          <tabs-trigger
            v-for="item in SECTIONS"
            :key="item.value"
            :value="item.value"
            class="group flex h-9 cursor-pointer items-center gap-3 rounded-lg px-3 text-fg hover:bg-surface-raised data-[state=active]:bg-primary/10 data-[state=active]:font-medium data-[state=active]:text-fg-strong"
          >
            <component
              :is="item.icon"
              class="size-4 text-fg-secondary group-data-[state=active]:text-primary"
              aria-hidden="true"
            />
            {{ item.label }}
          </tabs-trigger>
        </tabs-list>

        <tabs-content
          v-for="item in SECTIONS"
          :key="item.value"
          :value="item.value"
          class="flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-y-auto pe-1"
        >
          <div class="flex flex-col gap-1">
            <h3 class="text-lg font-semibold text-fg-strong">{{ item.label }}</h3>
            <p class="text-sm text-fg-secondary">{{ item.description }}</p>
          </div>

          <template v-if="item.value === 'profile'">
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
              <ui-setting-row label="Prénom" for="preferences-first-name">
                <ui-input
                  id="preferences-first-name"
                  v-model="names.first_name"
                  :maxlength="100"
                  autocomplete="given-name"
                  class="w-72"
                />
              </ui-setting-row>
              <ui-setting-row label="Nom" for="preferences-last-name">
                <ui-input
                  id="preferences-last-name"
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
          </template>

          <ui-setting-list v-else-if="item.value === 'language'">
            <ui-setting-row
              label="Langue de l’interface"
              description="Les autres langues arriveront avec la traduction de l’interface."
              for="preferences-language"
            >
              <ui-select
                id="preferences-language"
                v-model="language"
                :items="LANGUAGE_ITEMS"
                label="Langue de l’interface"
                class="w-53"
              />
            </ui-setting-row>
          </ui-setting-list>

          <template v-else-if="item.value === 'queries'">
            <ui-setting-list title="Résultats">
              <ui-setting-row
                label="Limite de lignes par défaut"
                description="Une requête sans LIMIT s’arrête là, et le résultat se signale comme tronqué. Chaque cellule SQL peut changer cette limite."
                for="preferences-row-limit"
              >
                <ui-select
                  id="preferences-row-limit"
                  v-model="form.row_limit"
                  :items="rowLimitItems"
                  label="Limite de lignes par défaut"
                  class="w-53"
                />
              </ui-setting-row>
            </ui-setting-list>
            <ui-setting-list title="Écritures">
              <ui-setting-row
                label="Sessions en lecture seule"
                description="La base refuse toute écriture, en plus du contrôle de Prisme. Désactivez-la pour autoriser les écritures cochées ci-dessous."
                for="preferences-read-only"
              >
                <ui-switch id="preferences-read-only" v-model="form.read_only_session" />
              </ui-setting-row>
              <ui-setting-row label="Écritures autorisées" :description="writesDescription" stacked>
                <preferences-writes
                  v-model="form.allowed_writes"
                  :disabled="form.read_only_session"
                />
              </ui-setting-row>
            </ui-setting-list>
            <p class="text-sm text-fg-secondary">
              REPLACE demande INSERT et DELETE ; CREATE INDEX et DROP INDEX demandent ALTER des
              tables ; ALTER VIEW demande CREATE des vues.
            </p>
          </template>

          <ui-setting-list v-else-if="item.value === 'editor'">
            <ui-setting-row
              label="Indentation"
              description="Un niveau d’indentation, avec Tab et Maj-Tab dans l’éditeur et pour Enjoliver. De 1 à 8 espaces."
              for="preferences-indent"
            >
              <ui-input
                id="preferences-indent"
                v-model="form.indent_size"
                type="number"
                :min="INDENT_MIN"
                :max="INDENT_MAX"
                required
                class="w-27"
              />
              <span class="w-18 text-fg-secondary">espaces</span>
            </ui-setting-row>
          </ui-setting-list>

          <ui-setting-list v-else-if="item.value === 'pivot'">
            <ui-setting-row
              label="Longueur d’un GROUP_CONCAT"
              description="Au-delà, le Pivoteur garde les valeurs entières qui tiennent, suivies de « … » et du nombre de valeurs omises. De 100 octets à 1 Mio."
              for="preferences-concat"
            >
              <ui-input
                id="preferences-concat"
                v-model="form.concat_max_bytes"
                type="number"
                :min="CONCAT_MIN"
                :max="CONCAT_MAX"
                required
                class="w-27"
              />
              <span class="w-18 text-fg-secondary">octets</span>
            </ui-setting-row>
            <ui-setting-row
              label="Mise à jour pendant la réception"
              description="Pendant que les lignes arrivent, délai le plus court entre deux calculs du tableau. De 0,2 à 60 secondes."
              for="preferences-refresh"
            >
              <ui-input
                id="preferences-refresh"
                v-model="refreshSeconds"
                type="number"
                :min="REFRESH_MIN_SECONDS"
                :max="REFRESH_MAX_SECONDS"
                :step="REFRESH_STEP_SECONDS"
                required
                class="w-27"
              />
              <span class="w-18 text-fg-secondary">secondes</span>
            </ui-setting-row>
          </ui-setting-list>

          <ui-setting-list v-else-if="item.value === 'console'">
            <ui-setting-row
              label="Historique des requêtes"
              description="Les requêtes exécutées restent dans la console pendant ce temps. De 1 à 3 650 jours."
              for="preferences-history"
            >
              <ui-input
                id="preferences-history"
                v-model="form.history_retention_days"
                type="number"
                :min="RETENTION_MIN"
                :max="RETENTION_MAX"
                required
                class="w-27"
              />
              <span class="w-18 text-fg-secondary">jours</span>
            </ui-setting-row>
            <ui-setting-row
              label="Messages de débogage"
              description="Les messages de débogage restent dans la console pendant ce temps. De 1 à 3 650 jours."
              for="preferences-debug"
            >
              <ui-input
                id="preferences-debug"
                v-model="form.debug_retention_days"
                type="number"
                :min="RETENTION_MIN"
                :max="RETENTION_MAX"
                required
                class="w-27"
              />
              <span class="w-18 text-fg-secondary">jours</span>
            </ui-setting-row>
          </ui-setting-list>
        </tabs-content>
      </tabs-root>
      <ui-alert v-if="error" tone="error" :title="error" />
    </form>
    <template #footer>
      <ui-button label="Annuler" @click="open = false" />
      <ui-button
        type="submit"
        form="preferences-form"
        label="Enregistrer"
        variant="solid"
        tone="primary"
        :loading="saving"
        class="flex-1"
      />
    </template>
  </ui-dialog>
</template>
