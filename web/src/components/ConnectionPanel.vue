<!--
  Choix de la connexion de travail : liste des connexions enregistrées,
  création d'une connexion, test et suppression de la connexion choisie.
-->
<script setup lang="ts">
import { NButton, NPopconfirm, NSelect, useMessage } from 'naive-ui'
import { computed, onMounted, ref } from 'vue'

import { probeConnection } from '@/api/connections'
import { useConnectionsStore } from '@/stores/connections'

import NewConnectionDialog from './NewConnectionDialog.vue'

const store = useConnectionsStore()
const message = useMessage()

/**
 * Fenêtre de création ouverte ?
 */
const creating = ref(false)

/**
 * Test de la connexion en cours ?
 */
const probing = ref(false)

/**
 * Choix proposés par la liste.
 */
const options = computed(() =>
  store.connections.map((connection) => ({
    label: `${connection.name} (${connection.username}@${connection.host}:${connection.port})`,
    value: connection.id,
  })),
)

/**
 * Se connecte au serveur choisi et affiche sa variante et sa version.
 */
async function probe(): Promise<void> {
  if (store.selectedId === null) {
    return
  }
  probing.value = true
  try {
    const info = await probeConnection(store.selectedId)
    message.success(`${info.flavor === 'mariadb' ? 'MariaDB' : 'MySQL'} ${info.version}`)
  } catch (failure) {
    message.error(failure instanceof Error ? failure.message : String(failure))
  } finally {
    probing.value = false
  }
}

/**
 * Supprime la connexion choisie, après confirmation.
 */
async function remove(): Promise<void> {
  const connection = store.selected
  if (connection === null) {
    return
  }
  try {
    await store.remove(connection.id)
    message.success(`Connexion « ${connection.name} » supprimée`)
  } catch (failure) {
    message.error(failure instanceof Error ? failure.message : String(failure))
  }
}

onMounted(async () => {
  try {
    await store.load()
  } catch (failure) {
    message.error(
      `Connexions illisibles : ${failure instanceof Error ? failure.message : String(failure)}`,
    )
  }
})
</script>

<template>
  <div class="connection-panel">
    <n-select
      v-model:value="store.selectedId"
      :options="options"
      placeholder="Choisir une connexion"
      size="small"
    />
    <div class="connection-actions">
      <n-button size="small" @click="creating = true">Nouvelle</n-button>
      <n-button
        size="small"
        :disabled="store.selectedId === null"
        :loading="probing"
        @click="probe"
      >
        Tester
      </n-button>
      <n-popconfirm positive-text="Supprimer" negative-text="Annuler" @positive-click="remove">
        <template #trigger>
          <n-button size="small" :disabled="store.selectedId === null">Supprimer</n-button>
        </template>
        Supprimer la connexion « {{ store.selected?.name }} » ?
      </n-popconfirm>
    </div>
    <new-connection-dialog v-model:open="creating" />
  </div>
</template>

<style scoped>
.connection-panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.connection-actions {
  display: flex;
  gap: 8px;
}
</style>
