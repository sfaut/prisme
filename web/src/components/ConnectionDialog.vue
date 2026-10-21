<!--
  Window to create or change a MySQL / MariaDB connection.

  When changing, the input starts from the saved connection, except the
  password, which the interface does not know: left empty, it is unchanged.
  "Tester" connects with the current input, without saving it, and shows the
  flavor and the version of the server.

  "À la connexion" receives queries run when each connection to the database
  opens, separated by ";": only session settings (`SET NAMES`,
  `SET SESSION sql_mode`...) are allowed, the server refuses the rest on
  saving. "Tester" runs them too.

  The maximum lengths repeat the server's
  (`crates/server/src/connections.rs`). The other checks are made by the
  server, whose message shows in the window on refusal.

  Window, fields and buttons of Prisme (`ui/`).
-->
<script setup lang="ts">
import { errorMessage } from '@/api/errorMessage'
import { Eye, EyeOff } from '@lucide/vue'
import { computed, reactive, ref, watch } from 'vue'

import { probeSettings, serverLabel } from '@/api/connections'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import type { NewConnection } from '@/api/generated/NewConnection'
import { useConnectionsStore } from '@/stores/connections'

import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import UiDialog from './ui/UiDialog.vue'
import UiField from './ui/UiField.vue'
import UiInput from './ui/UiInput.vue'
import UiTextarea from './ui/UiTextarea.vue'
import { vTooltip } from './ui/tooltip'

/**
 * Window open or closed.
 */
const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  /**
   * Connection to change, `null` to create one.
   */
  connection: ConnectionSummary | null
}>()

const emit = defineEmits<{
  /**
   * The changed connection is saved.
   */
  updated: [connectionId: number]
}>()

const store = useConnectionsStore()

/**
 * Current input.
 */
const form = reactive<Required<NewConnection>>(emptyForm())

/**
 * Message of the server if saving failed.
 */
const error = ref<string | null>(null)

/**
 * Saving in progress.
 */
const saving = ref(false)

/**
 * Test of the current input in progress.
 */
const probing = ref(false)

/**
 * Is the password shown in clear?
 */
const showPassword = ref(false)

/**
 * Result of the last successful test (flavor and version of the server), `null` if none.
 */
const probed = ref<string | null>(null)

/**
 * Title of the window.
 */
const title = computed(() =>
  props.connection === null ? 'Nouvelle connexion' : 'Modifier la connexion',
)

/**
 * Empty input, with MySQL's default port.
 */
function emptyForm(): Required<NewConnection> {
  return {
    name: '',
    host: '127.0.0.1',
    port: 3306,
    username: '',
    password: '',
    database_name: '',
    init_sql: '',
    session_timeout_minutes: 5,
    show_system_schemas: true,
  }
}

/**
 * Initial input: the connection to change, password empty, or an empty input.
 *
 * @param connection Connection to change, `null` to create one.
 */
function initialForm(connection: ConnectionSummary | null): Required<NewConnection> {
  if (connection === null) {
    return emptyForm()
  }
  const {
    name,
    host,
    port,
    username,
    database_name,
    init_sql,
    session_timeout_minutes,
    show_system_schemas,
  } = connection
  return {
    name,
    host,
    port,
    username,
    password: '',
    database_name,
    init_sql,
    session_timeout_minutes,
    show_system_schemas,
  }
}

// Each opening starts again from the chosen connection, without the previous error.
watch(open, (isOpen) => {
  if (isOpen) {
    Object.assign(form, initialForm(props.connection))
    error.value = null
    probed.value = null
  }
})

/**
 * Connects with the current input, without saving it. When changing, an
 * empty password takes that of the saved connection.
 */
async function probe(): Promise<void> {
  probing.value = true
  error.value = null
  probed.value = null
  try {
    const info = await probeSettings({ connection: { ...form }, saved_id: props.connection?.id })
    probed.value = serverLabel(info)
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    probing.value = false
  }
}

/**
 * Saves the connection, then closes the window.
 */
async function save(): Promise<void> {
  saving.value = true
  error.value = null
  try {
    if (props.connection === null) {
      await store.create({ ...form })
    } else {
      await store.update(props.connection.id, { ...form })
      emit('updated', props.connection.id)
    }
    open.value = false
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <ui-dialog v-model:open="open" :title="title">
    <form id="connection-form" class="flex flex-col gap-2" @submit.prevent="save">
      <ui-field label="Nom">
        <ui-input v-model="form.name" :maxlength="100" placeholder="prod, local..." />
      </ui-field>
      <ui-field label="Hôte">
        <ui-input v-model="form.host" :maxlength="253" />
      </ui-field>
      <ui-field label="Port">
        <ui-input v-model="form.port" type="number" :min="1" :max="65535" />
      </ui-field>
      <ui-field label="Compte">
        <ui-input v-model="form.username" :maxlength="80" />
      </ui-field>
      <ui-field label="Mot de passe">
        <ui-input
          v-model="form.password"
          :type="showPassword ? 'text' : 'password'"
          autocomplete="new-password"
          :placeholder="connection === null ? '' : 'inchangé'"
        >
          <template #trailing>
            <ui-button
              :icon="showPassword ? EyeOff : Eye"
              variant="ghost"
              small
              :aria-label="showPassword ? 'Masquer le mot de passe' : 'Afficher le mot de passe'"
              @click="showPassword = !showPassword"
            />
          </template>
        </ui-input>
      </ui-field>
      <ui-field label="Base">
        <ui-input v-model="form.database_name" :maxlength="64" placeholder="facultatif" />
      </ui-field>
      <ui-field label="À la connexion" top>
        <ui-textarea
          v-model="form.init_sql"
          :maxlength="4000"
          rows="3"
          placeholder="SET NAMES utf8mb4;&#10;SET SESSION sql_mode = 'ANSI_QUOTES';"
          aria-label="Requêtes à exécuter à la connexion"
          v-tooltip="
            'Réglages de la session exécutés à l’ouverture de chaque connexion, séparés par des « ; »'
          "
        />
      </ui-field>
      <ui-field label="Session gardée">
        <div class="flex items-center gap-2">
          <ui-input
            v-model="form.session_timeout_minutes"
            type="number"
            :min="0"
            :max="1440"
            class="w-20"
            aria-label="Minutes de garde de la session"
            v-tooltip="
              'La session (connexion, transactions, requêtes en cours) reste ouverte ce nombre de minutes après la fermeture de la page, qui s’y raccroche à son retour ; 0 la ferme aussitôt'
            "
          />
          <span class="text-ui text-fg-muted">min après la fermeture de la page</span>
        </div>
      </ui-field>
      <ui-field label="Explorateur">
        <label
          class="flex items-center gap-2 text-ui text-fg-strong"
          v-tooltip="
            'Bases fournies par le serveur : information_schema, mysql, performance_schema, sys'
          "
        >
          <input
            v-model="form.show_system_schemas"
            type="checkbox"
            class="size-icon accent-accent"
          />
          Afficher les bases système
        </label>
      </ui-field>
      <ui-alert v-if="error" tone="error" :title="error" />
      <ui-alert v-if="probed" tone="success" :title="probed" />
    </form>
    <template #footer>
      <ui-button label="Tester" :loading="probing" @click="probe" />
      <ui-button
        type="submit"
        form="connection-form"
        label="Enregistrer"
        variant="solid"
        tone="accent"
        :loading="saving"
        class="flex-1"
      />
    </template>
  </ui-dialog>
</template>
