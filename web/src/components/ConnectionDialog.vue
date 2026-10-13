<!--
  Fenêtre de création ou de modification d'une connexion MySQL / MariaDB.

  En modification, la saisie part de la connexion enregistrée, sauf le mot de
  passe, que l'interface ne connaît pas : laissé vide, il reste inchangé.
  « Tester » se connecte avec la saisie en cours, sans l'enregistrer, et
  affiche la variante et la version du serveur.

  Les longueurs maximales reprennent celles du serveur
  (`crates/server/src/connections.rs`). Les autres contrôles sont faits par le
  serveur, dont le message s'affiche dans la fenêtre en cas de refus.
-->
<script setup lang="ts">
import {
  NAlert,
  NButton,
  NConfigProvider,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NModal,
} from 'naive-ui'
import { computed, reactive, ref, watch } from 'vue'

import { probeSettings, serverLabel } from '@/api/connections'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import type { NewConnection } from '@/api/generated/NewConnection'
import { useConnectionsStore } from '@/stores/connections'
import { POPUP_CONTENT_THEME } from '@/theme'

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
  return { name: '', host: '127.0.0.1', port: 3306, username: '', password: '', database_name: '' }
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
  const { name, host, port, username, database_name } = connection
  return { name, host, port, username, password: '', database_name }
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
  <n-modal v-model:show="open" preset="card" :title="title" style="width: 420px">
    <n-config-provider :theme-overrides="POPUP_CONTENT_THEME">
      <n-form label-placement="left" label-width="110" @submit.prevent="save">
        <n-form-item label="Nom">
          <n-input v-model:value="form.name" :maxlength="100" placeholder="prod, local..." />
        </n-form-item>
        <n-form-item label="Hôte">
          <n-input v-model:value="form.host" :maxlength="253" />
        </n-form-item>
        <n-form-item label="Port">
          <n-input-number v-model:value="form.port" :min="1" :max="65535" :show-button="false" />
        </n-form-item>
        <n-form-item label="Compte">
          <n-input v-model:value="form.username" :maxlength="80" />
        </n-form-item>
        <n-form-item label="Mot de passe">
          <n-input
            v-model:value="form.password"
            type="password"
            show-password-on="click"
            :input-props="{ autocomplete: 'new-password' }"
            :placeholder="connection === null ? '' : 'inchangé'"
          />
        </n-form-item>
        <n-form-item label="Base">
          <n-input v-model:value="form.database_name" :maxlength="64" placeholder="facultatif" />
        </n-form-item>
        <n-alert v-if="error" type="error" :show-icon="false" class="dialog-error">
          {{ error }}
        </n-alert>
        <n-alert v-if="probed" type="success" :show-icon="false" class="dialog-error">
          {{ probed }}
        </n-alert>
        <div class="dialog-actions">
          <n-button :loading="probing" @click="probe">Tester</n-button>
          <n-button type="primary" attr-type="submit" :loading="saving" class="dialog-save">
            Enregistrer
          </n-button>
        </div>
      </n-form>
    </n-config-provider>
  </n-modal>
</template>

<style scoped>
.dialog-error {
  margin-bottom: 12px;
}

.dialog-actions {
  display: flex;
  gap: 8px;
}

.dialog-save {
  flex: 1;
}
</style>
