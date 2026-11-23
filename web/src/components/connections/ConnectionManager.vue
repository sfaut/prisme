<!--
  Connections manager: a wide window, the saved connections on the left
  (`ConnectionManagerList`), on the right the connection chosen, or the
  creation of a new one. Opened from the account menu ("Connexions"), from
  the "Nouveau" menu of the top bar ("Connexion", on a creation) and from the
  "⋮" menu of a source in the navigator ("Modifier", on that connection),
  through the connections store: it is mounted once.

  A saved connection shows its card at the top (icon of its kind, name,
  flavor badge, source and its version, last connection; the list gives its
  place), with "Tester", "Dupliquer" and "Supprimer" (after confirmation,
  `ConnectionRemoveDialog`), then its settings on a single page, those of
  its family: a server (`ConnectionFormServer`) or databases held in files
  (`ConnectionFormFiles`). "Tester" connects with the
  settings as typed; unchanged, the test is that of the saved connection,
  whose server and date of connection the server then remembers.

  Each connection keeps its input while the window is open: a connection
  changed and not saved is a draft, which the list marks, and which waits
  while another connection is shown. "Enregistrer" saves the connection
  shown, "Annuler les modifications" goes back to the saved settings.
  Closing the window drops the drafts and the creation in progress, without
  asking: the list and the footer said they were not saved. Saving a changed
  connection closes its sessions: while transactions are open on them, a
  warning says so, and the button says that they will be rolled back.

  A creation goes in three steps (`ConnectionManagerSteps`): the kind of
  source (`ConnectionManagerTypes`, MySQL / MariaDB or SQLite for now), the
  settings of its family, then the result of their test before saving: a
  failed test does not prevent saving (a server not reachable yet). It stays
  under the "Nouvelle connexion" button while another connection is shown.
  "Dupliquer" starts a creation at the settings, from the input of the
  connection shown under another name; its password, left empty, is that of
  the copied connection, which the server copies.

  The server checks the input; its message shows in the right pane. On
  opening, the focus goes to the window, not to the filter of the list.
-->
<script setup lang="ts">
import { ArrowLeft, ArrowRight, CopyPlus, PlugZap, Trash2 } from '@lucide/vue'
import { computed, ref, watch } from 'vue'

import { probeConnection, probeMessage, probeSettings } from '@/api/connections'
import { serverName } from '@/api/flavors'
import { errorMessage } from '@/api/errorMessage'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import { useConnectionsStore } from '@/stores/connections'
import { useEditorsStore } from '@/stores/editors'

import {
  type ConnectionForm as Form,
  connectedText,
  copyForm,
  connectionOf,
  ENGINE_FAMILIES,
  emptyForm,
  formOf,
  isChanged,
  placeOf,
  placeOfForm,
} from './connectionForm'
import type { Creation, ListEntry, ManagerSelection } from './connectionManager'
import { ENGINE_TYPES, SOURCE_FAMILIES } from './sourceTypes'
import { openTransactionsNotice } from './transactionNotice'

import ConnectionFlavorBadge from './ConnectionFlavorBadge.vue'
import ConnectionFormFiles from './ConnectionFormFiles.vue'
import ConnectionFormServer from './ConnectionFormServer.vue'
import ConnectionManagerList from './ConnectionManagerList.vue'
import ConnectionManagerSteps from './ConnectionManagerSteps.vue'
import ConnectionManagerTypes from './ConnectionManagerTypes.vue'
import ConnectionRemoveDialog from './ConnectionRemoveDialog.vue'
import UiAlert from '@/components/ui/UiAlert.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import { vTooltip } from '@/components/ui/tooltip'

const connections = useConnectionsStore()
const editors = useEditorsStore()

/**
 * Kinds of sources, by key.
 */
const TYPES = new Map(
  SOURCE_FAMILIES.flatMap((family) => family.types).map((type) => [type.key, type]),
)

/**
 * Engine of each kind of source supported, by key.
 */
const TYPE_ENGINES = new Map(Object.values(ENGINE_TYPES).map((type) => [type.key, type.engine]))

/**
 * Is the window open?
 */
const open = computed({
  get: () => connections.manager.open,
  set: (isOpen) => {
    if (!isOpen) {
      connections.closeManager()
    }
  },
})

/**
 * What the right pane shows.
 */
const selected = ref<ManagerSelection>(null)

/**
 * Input of each saved connection shown since the window opened, by id.
 */
const forms = ref<Record<number, Form>>({})

/**
 * Creation in progress, `null` if none.
 */
const creation = ref<Creation | null>(null)

/**
 * Message of the server after a refusal or a failed test, `null` if none.
 */
const error = ref<string | null>(null)

/**
 * Message of the last successful test (flavor and version of the server, and
 * the warning of an unsupported version), `null` if none.
 */
const probed = ref<ReturnType<typeof probeMessage> | null>(null)

/**
 * Test in progress.
 */
const probing = ref(false)

/**
 * Saving in progress.
 */
const saving = ref(false)

/**
 * Connection whose deletion waits for confirmation, `null` if none.
 */
const removing = ref<ConnectionSummary | null>(null)

/**
 * Saved connection shown on the right, `null` during a creation.
 */
const saved = computed(() =>
  typeof selected.value === 'number'
    ? (connections.connections.find((connection) => connection.id === selected.value) ?? null)
    : null,
)

/**
 * Input of the connection shown, or of the creation.
 */
const form = computed<Form | null>(() =>
  saved.value !== null
    ? (forms.value[saved.value.id] ?? null)
    : selected.value === 'new'
      ? (creation.value?.form ?? null)
      : null,
)

/**
 * Creation shown on the right, `null` when a saved connection is.
 */
const shownCreation = computed(() => (selected.value === 'new' ? creation.value : null))

/**
 * Is the input of a saved connection changed?
 *
 * @param connection Saved connection.
 */
function changed(connection: ConnectionSummary): boolean {
  const input = forms.value[connection.id]
  return input !== undefined && isChanged(input, connection)
}

/**
 * Is the connection shown changed?
 */
const shownChanged = computed(() => saved.value !== null && changed(saved.value))

/**
 * Lines of the list.
 */
const entries = computed<ListEntry[]>(() =>
  connections.connections.map((connection) => {
    const isChangedNow = changed(connection)
    return {
      id: connection.id,
      engine: connection.engine,
      name: isChangedNow ? forms.value[connection.id]!.name : connection.name,
      place: placeOf(connection.settings),
      changed: isChangedNow,
      connected: connection.probe !== null,
    }
  }),
)

/**
 * Number of the other connections changed and not saved.
 */
const othersChanged = computed(
  () =>
    connections.connections.filter(
      (connection) => connection.id !== selected.value && changed(connection),
    ).length,
)

/**
 * State of the drafts, in the footer, empty when nothing waits.
 */
const draftsText = computed(() => {
  const others = othersChanged.value
  const s = others > 1 ? 's' : ''
  if (shownChanged.value) {
    return others > 0
      ? `Modifications non enregistrées · ${others} autre${s} connexion${s} modifiée${s}`
      : 'Modifications non enregistrées'
  }
  return others > 0 ? `${others} connexion${s} modifiée${s}, non enregistrée${s}` : ''
})

/**
 * Editors of the connection shown with a transaction open, which saving
 * rolls back.
 */
const transactionCount = computed(() =>
  saved.value === null ? 0 : editors.inTransaction(saved.value.id).length,
)

/**
 * Title and subtitle of each step of a creation.
 */
const stepHeading = computed(() => {
  const step = shownCreation.value
  if (step === null) {
    return null
  }
  const type = TYPES.get(step.type)!
  return [
    {
      title: 'Quel type de source ?',
      subtitle: 'Un serveur de bases de données, ou un fichier lu par le serveur de Prisme.',
    },
    { title: `Réglages · ${type.label}`, subtitle: type.description },
    {
      title: 'Vérification',
      subtitle:
        error.value === null
          ? 'Prisme a testé la connexion avec ces réglages.'
          : 'Le test a échoué : la connexion peut être enregistrée quand même, et testée plus tard.',
    },
  ][step.step - 1]!
})

/**
 * Settings of a creation, at its last step, before saving.
 */
const summary = computed(() => {
  const step = shownCreation.value
  if (step === null) {
    return []
  }
  const input = step.form
  const settings =
    ENGINE_FAMILIES[input.engine] === 'files'
      ? input.databases.map((database) => ({
          label: database.schema || '(sans schéma)',
          value:
            database.origin === 'memory'
              ? 'en mémoire'
              : database.origin === 'new'
                ? `${database.location} (à créer)`
                : database.location,
        }))
      : [
          { label: 'Serveur', value: placeOfForm(input) },
          { label: 'Compte', value: input.username || '(aucun)' },
          { label: 'Base', value: input.database_name || '(aucune)' },
        ]
  return [
    { label: 'Type', value: TYPES.get(step.type)!.label },
    { label: 'Nom', value: input.name || '(sans nom)' },
    ...settings,
    { label: 'Session gardée', value: `${input.session_timeout_minutes} min` },
  ]
})

/**
 * Forgets the messages of the right pane.
 */
function clearMessages(): void {
  error.value = null
  probed.value = null
}

/**
 * Shows a saved connection, with its input kept since the window opened.
 *
 * @param id Id of the connection.
 */
function select(id: number): void {
  const connection = connections.connections.find((item) => item.id === id)
  if (connection === undefined) {
    return
  }
  forms.value[id] ??= formOf(connection)
  selected.value = id
  clearMessages()
}

/**
 * Shows the first connection, or starts a creation when there is none.
 */
function selectFirst(): void {
  const first = connections.connections[0]
  if (first === undefined) {
    startCreation()
  } else {
    select(first.id)
  }
}

/**
 * Shows the creation in progress, or starts one, at the choice of its kind.
 */
function startCreation(): void {
  creation.value ??= { step: 1, type: 'mysql', form: emptyForm(), copyOf: null }
  selected.value = 'new'
  clearMessages()
}

/**
 * Shows the list or the creation chosen in the list.
 *
 * @param target Connection, or the creation in progress (`new`).
 */
function onSelect(target: number | 'new'): void {
  if (target === 'new') {
    startCreation()
  } else {
    select(target)
  }
}

// Each opening shows what it asks for: a connection, a creation, or the
// connection shown last.
watch(
  () => connections.manager.opening,
  async () => {
    try {
      // Read again: the dates of connection move with the tests and the catalogs.
      await connections.load()
    } catch (failure) {
      error.value = errorMessage(failure)
      return
    }
    const target = connections.manager.target
    if (target === 'new') {
      startCreation()
    } else if (typeof target === 'number') {
      select(target)
    } else if (typeof selected.value === 'number' && saved.value !== null) {
      select(selected.value)
    } else {
      selectFirst()
    }
  },
)

// Closed, the window drops the drafts and the creation in progress.
watch(open, (isOpen) => {
  if (!isOpen) {
    forms.value = {}
    creation.value = null
    removing.value = null
    clearMessages()
  }
})

// A changed input makes the last test stale.
watch(form, () => (probed.value = null), { deep: true })

/**
 * Connects with the input of the connection shown. Unchanged, the test is
 * that of the saved connection, whose server and date of connection the
 * server remembers; changed, an empty password takes the saved one.
 */
async function probe(): Promise<void> {
  const connection = saved.value
  const input = form.value
  if (connection === null || input === null) {
    return
  }
  probing.value = true
  clearMessages()
  try {
    if (changed(connection)) {
      probed.value = probeMessage(
        await probeSettings({ connection: connectionOf(input), saved_id: connection.id }),
      )
    } else {
      probed.value = probeMessage(await probeConnection(connection.id))
      await connections.load()
    }
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    probing.value = false
  }
}

/**
 * Saves the connection shown.
 */
async function save(): Promise<void> {
  const connection = saved.value
  const input = form.value
  if (connection === null || input === null || !changed(connection)) {
    return
  }
  saving.value = true
  clearMessages()
  try {
    await connections.update(connection.id, connectionOf(input))
    const updated = connections.connections.find((item) => item.id === connection.id)
    if (updated !== undefined) {
      forms.value[connection.id] = formOf(updated)
    }
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    saving.value = false
  }
}

/**
 * Goes back to the saved settings of the connection shown.
 */
function revert(): void {
  if (saved.value !== null) {
    forms.value[saved.value.id] = formOf(saved.value)
    clearMessages()
  }
}

/**
 * Starts the creation of a copy of the connection shown, at its settings.
 */
function duplicate(): void {
  const connection = saved.value
  const input = form.value
  if (connection === null || input === null) {
    return
  }
  creation.value = {
    step: 2,
    type: ENGINE_TYPES[connection.engine].key,
    form: copyForm(input),
    copyOf: connection.id,
  }
  selected.value = 'new'
  clearMessages()
}

/**
 * Goes to the next step of the creation. From the settings, tests them, and
 * shows the result at the last step, success or failure.
 */
async function next(): Promise<void> {
  const current = creation.value
  if (current === null) {
    return
  }
  clearMessages()
  if (current.step === 1) {
    current.form.engine = TYPE_ENGINES.get(current.type) ?? current.form.engine
    current.step = 2
    return
  }
  probing.value = true
  try {
    const info = await probeSettings({
      connection: connectionOf(current.form),
      saved_id: current.copyOf ?? undefined,
    })
    probed.value = probeMessage(info)
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    probing.value = false
  }
  current.step = 3
}

/**
 * Goes back to the previous step of the creation.
 */
function back(): void {
  if (creation.value !== null && creation.value.step > 1) {
    creation.value.step = creation.value.step === 3 ? 2 : 1
    clearMessages()
  }
}

/**
 * Drops the creation in progress and shows a saved connection.
 */
function abandon(): void {
  creation.value = null
  selectFirst()
}

/**
 * Saves the new connection, or the copy, and shows it.
 */
async function create(): Promise<void> {
  const current = creation.value
  if (current === null) {
    return
  }
  saving.value = true
  clearMessages()
  try {
    const input = { ...current.form }
    const id =
      current.copyOf === null
        ? await connections.create(connectionOf(input))
        : await connections.copy(current.copyOf, connectionOf(input))
    creation.value = null
    select(id)
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    saving.value = false
  }
}

/**
 * Sends the form (Enter in a field, or the main button of the footer): saves
 * the connection shown, or moves the creation on.
 */
function submit(): void {
  const current = shownCreation.value
  if (current === null) {
    void save()
  } else if (current.step === 3) {
    void create()
  } else {
    void next()
  }
}

/**
 * Forgets the input of a deleted connection and shows another one.
 *
 * @param id Id of the deleted connection.
 */
function onRemoved(id: number): void {
  delete forms.value[id]
  if (selected.value === id) {
    selectFirst()
  }
}
</script>

<template>
  <ui-dialog v-model:open="open" title="Connexions" wide focus-window>
    <div class="flex min-h-0 flex-1">
      <connection-manager-list
        :entries="entries"
        :creation="creation"
        :selected="selected"
        @select="onSelect"
        @create="startCreation"
      />

      <form
        id="connection-manager-form"
        class="flex min-h-0 min-w-0 flex-1 flex-col"
        @submit.prevent="submit"
      >
        <!-- Card of the saved connection shown. -->
        <div
          v-if="saved && form"
          class="flex items-center gap-4 border-b border-line px-8 pt-6 pb-4"
        >
          <span
            class="grid size-12 shrink-0 place-items-center rounded-control bg-primary/15 text-primary"
            aria-hidden="true"
          >
            <component :is="ENGINE_TYPES[saved.engine].icon" class="size-5" />
          </span>
          <div class="flex min-w-0 flex-1 flex-col gap-1">
            <h3 class="flex min-w-0 items-center gap-2 text-lg font-semibold text-fg-strong">
              <span class="truncate">{{ form.name || '(sans nom)' }}</span>
              <connection-flavor-badge
                v-if="saved.probe"
                :flavor="saved.probe.flavor"
                :version="saved.probe.version"
              />
            </h3>
            <div class="flex min-w-0 items-center gap-4 text-sm text-fg-secondary">
              <span v-if="saved.probe" class="truncate font-mono">
                {{ serverName(saved.probe.flavor, saved.probe.version) }}
              </span>
              <span class="flex shrink-0 items-center gap-2">
                <span
                  class="size-2 rounded-full"
                  :class="
                    shownChanged
                      ? 'bg-warning'
                      : saved.probe !== null
                        ? 'bg-success'
                        : 'bg-fg-faint'
                  "
                  aria-hidden="true"
                />
                {{ shownChanged ? 'Modifiée, non enregistrée' : connectedText(saved) }}
              </span>
            </div>
          </div>
          <div class="flex shrink-0 items-center gap-3">
            <ui-button :icon="PlugZap" label="Tester" :loading="probing" @click="probe" />
            <ui-button
              :icon="CopyPlus"
              aria-label="Dupliquer"
              v-tooltip="{
                title: 'Dupliquer',
                description: 'Nouvelle connexion avec ces réglages',
              }"
              @click="duplicate"
            />
            <ui-button
              :icon="Trash2"
              tone="error"
              aria-label="Supprimer"
              v-tooltip="{
                title: 'Supprimer',
                description: 'Supprime la connexion, après confirmation',
              }"
              @click="removing = saved"
            />
          </div>
        </div>

        <div class="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto px-8 py-6">
          <template v-if="shownCreation && stepHeading">
            <connection-manager-steps :step="shownCreation.step" />
            <div class="flex flex-col gap-1">
              <h3 class="text-lg font-semibold text-fg-strong">{{ stepHeading.title }}</h3>
              <p class="text-sm text-fg-secondary">{{ stepHeading.subtitle }}</p>
            </div>
          </template>

          <ui-alert v-if="error" tone="error" :title="error" />
          <ui-alert v-if="probed" :tone="probed.tone" :title="probed.title" />
          <ui-alert
            v-if="shownChanged && transactionCount > 0"
            tone="warning"
            :title="openTransactionsNotice(transactionCount, 'enregistrer')"
          />

          <connection-manager-types
            v-if="shownCreation?.step === 1"
            v-model:type="shownCreation.type"
          />
          <!-- The input is changed in place: the form gets it, without replacing it. -->
          <template v-else-if="form && (saved || shownCreation?.step === 2)">
            <connection-form-files v-if="ENGINE_FAMILIES[form.engine] === 'files'" :form="form" />
            <connection-form-server
              v-else
              :form="form"
              :password="saved ? 'saved' : shownCreation?.copyOf ? 'copy' : 'new'"
            />
          </template>
          <template v-else-if="shownCreation?.step === 3">
            <dl
              class="grid grid-cols-[--spacing(40)_minmax(0,1fr)] gap-x-6 gap-y-2 rounded-control border border-line px-4 py-3"
            >
              <template v-for="row in summary" :key="row.label">
                <dt class="text-fg-secondary">{{ row.label }}</dt>
                <dd class="truncate font-mono text-fg-strong">{{ row.value }}</dd>
              </template>
            </dl>
            <p class="text-sm text-fg-secondary">
              Une fois enregistrée, la connexion apparaît dans le navigateur d’objets et dans le
              choix de connexion des éditeurs.
            </p>
          </template>
        </div>
      </form>
    </div>

    <template #footer>
      <div class="flex min-w-0 flex-1 items-center gap-2">
        <span v-if="shownCreation" class="text-fg-secondary">
          Étape {{ shownCreation.step }} sur 3
        </span>
        <span v-else-if="draftsText" class="flex items-center gap-2 text-sm text-warning">
          <span class="size-2 shrink-0 rounded-full bg-warning" aria-hidden="true" />
          {{ draftsText }}
        </span>
      </div>
      <template v-if="shownCreation">
        <ui-button label="Abandonner" @click="abandon" />
        <ui-button v-if="shownCreation.step > 1" :icon="ArrowLeft" label="Retour" @click="back" />
        <ui-button
          type="submit"
          form="connection-manager-form"
          :label="
            shownCreation.step === 3
              ? 'Enregistrer'
              : shownCreation.step === 2
                ? 'Tester'
                : 'Suivant'
          "
          :trailing-icon="shownCreation.step === 3 ? undefined : ArrowRight"
          variant="solid"
          tone="primary"
          :loading="probing || saving"
        />
      </template>
      <template v-else-if="saved">
        <ui-button label="Annuler les modifications" :disabled="!shownChanged" @click="revert" />
        <ui-button
          type="submit"
          form="connection-manager-form"
          :label="transactionCount > 0 && shownChanged ? 'Enregistrer et annuler' : 'Enregistrer'"
          variant="solid"
          :tone="transactionCount > 0 && shownChanged ? 'error' : 'primary'"
          :disabled="!shownChanged"
          :loading="saving"
        />
      </template>
    </template>
  </ui-dialog>

  <connection-remove-dialog
    v-model:connection="removing"
    @removed="onRemoved"
    @failed="(_, message) => (error = message)"
  />
</template>
