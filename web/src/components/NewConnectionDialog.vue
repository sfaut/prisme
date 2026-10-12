<!--
  Fenêtre de création d'une connexion MySQL / MariaDB.

  Les longueurs maximales reprennent celles du serveur
  (`crates/server/src/connections.rs`). Les autres contrôles sont faits par le
  serveur, dont le message s'affiche dans la fenêtre en cas de refus.
-->
<script setup lang="ts">
import { NAlert, NButton, NForm, NFormItem, NInput, NInputNumber, NModal } from 'naive-ui'
import { reactive, ref } from 'vue'

import type { NewConnection } from '@/api/generated/NewConnection'
import { useConnectionsStore } from '@/stores/connections'

/**
 * Fenêtre ouverte ou fermée.
 */
const open = defineModel<boolean>('open', { required: true })

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
 * Saisie vide, avec le port par défaut de MySQL.
 */
function emptyForm(): Required<NewConnection> {
  return { name: '', host: '127.0.0.1', port: 3306, username: '', password: '', database_name: '' }
}

/**
 * Enregistre la connexion, puis ferme et vide la fenêtre.
 */
async function save(): Promise<void> {
  saving.value = true
  error.value = null
  try {
    await store.create({ ...form })
    Object.assign(form, emptyForm())
    open.value = false
  } catch (failure) {
    error.value = failure instanceof Error ? failure.message : String(failure)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <n-modal v-model:show="open" preset="card" title="Nouvelle connexion" style="width: 420px">
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
        />
      </n-form-item>
      <n-form-item label="Base">
        <n-input v-model:value="form.database_name" :maxlength="64" placeholder="facultatif" />
      </n-form-item>
      <n-alert v-if="error" type="error" :show-icon="false" class="dialog-error">
        {{ error }}
      </n-alert>
      <n-button type="primary" attr-type="submit" block :loading="saving">Enregistrer</n-button>
    </n-form>
  </n-modal>
</template>

<style scoped>
.dialog-error {
  margin-bottom: 12px;
}
</style>
