<!--
  Fenêtre de création ou de modification d'une connexion MySQL / MariaDB.

  En modification, la saisie part de la connexion enregistrée, sauf le mot de
  passe, que l'interface ne connaît pas : laissé vide, il reste inchangé.
  « Tester » se connecte avec la saisie en cours, sans l'enregistrer, et
  affiche la variante et la version du serveur.

  « À la connexion » reçoit des requêtes exécutées à l'ouverture de chaque
  connexion vers la base, séparées par des « ; » : seuls les réglages de la
  session (`SET NAMES`, `SET SESSION sql_mode`...) sont permis, le serveur
  refuse le reste à l'enregistrement. « Tester » les exécute aussi.

  Les longueurs maximales reprennent celles du serveur
  (`crates/server/src/connections.rs`). Les autres contrôles sont faits par le
  serveur, dont le message s'affiche dans la fenêtre en cas de refus.

  Fenêtre, champs et boutons de Prisme (`ui/`).
-->
<script setup lang="ts">
import { Eye, EyeOff } from 'lucide-vue-next'
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

/**
 * Fenêtre ouverte ou fermée.
 */
const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  /**
   * Connexion à modifier, `null` pour en créer une.
   */
  connection: ConnectionSummary | null
}>()

const emit = defineEmits<{
  /**
   * La connexion modifiée est enregistrée.
   */
  updated: [connectionId: number]
}>()

const store = useConnectionsStore()

/**
 * Saisie en cours.
 */
const form = reactive<Required<NewConnection>>(emptyForm())

/**
 * Message du serveur si l'enregistrement a échoué.
 */
const error = ref<string | null>(null)

/**
 * Enregistrement en cours.
 */
const saving = ref(false)

/**
 * Test de la saisie en cours.
 */
const probing = ref(false)

/**
 * Mot de passe affiché en clair ?
 */
const showPassword = ref(false)

/**
 * Résultat du dernier test réussi (variante et version du serveur), `null` si aucun.
 */
const probed = ref<string | null>(null)

/**
 * Titre de la fenêtre.
 */
const title = computed(() =>
  props.connection === null ? 'Nouvelle connexion' : 'Modifier la connexion',
)

/**
 * Saisie vide, avec le port par défaut de MySQL.
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
  }
}

/**
 * Saisie de départ : la connexion à modifier, mot de passe vide, ou une saisie vide.
 *
 * @param connection Connexion à modifier, `null` pour en créer une.
 */
function initialForm(connection: ConnectionSummary | null): Required<NewConnection> {
  if (connection === null) {
    return emptyForm()
  }
  const { name, host, port, username, database_name, init_sql } = connection
  return { name, host, port, username, password: '', database_name, init_sql }
}

// Chaque ouverture repart de la connexion choisie, sans l'erreur précédente.
watch(open, (isOpen) => {
  if (isOpen) {
    Object.assign(form, initialForm(props.connection))
    error.value = null
    probed.value = null
  }
})

/**
 * Se connecte avec la saisie en cours, sans l'enregistrer. En modification,
 * un mot de passe vide reprend celui de la connexion enregistrée.
 */
async function probe(): Promise<void> {
  probing.value = true
  error.value = null
  probed.value = null
  try {
    const info = await probeSettings({ connection: { ...form }, saved_id: props.connection?.id })
    probed.value = serverLabel(info)
  } catch (failure) {
    error.value = failure instanceof Error ? failure.message : String(failure)
  } finally {
    probing.value = false
  }
}

/**
 * Enregistre la connexion, puis ferme la fenêtre.
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
    error.value = failure instanceof Error ? failure.message : String(failure)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <ui-dialog v-model:open="open" :title="title">
    <form id="connection-form" class="flex flex-col gap-gap" @submit.prevent="save">
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
          title="Réglages de la session exécutés à l'ouverture de chaque connexion, séparés par des « ; »"
        />
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
