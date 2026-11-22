<!--
  Confirmation of the deletion of a connection, asked by the "⋮" menu of a
  source in the navigator and by the connections manager.

  Open while a connection waits for confirmation (`v-model:connection`). It
  says that no data is lost, and warns of the transactions open on the
  connection, which the deletion rolls back: its button then says so.
  Confirmed, the connection is deleted, and every navigator forgets its
  source; the parent learns the outcome (`removed`, `failed`).
-->
<script setup lang="ts">
import { computed } from 'vue'

import { errorMessage } from '@/api/errorMessage'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import { useConnectionsStore } from '@/stores/connections'
import { useEditorsStore } from '@/stores/editors'
import { useNavigatorStore } from '@/stores/navigator'

import { openTransactionsNotice } from './transactionNotice'

import UiAlert from '@/components/ui/UiAlert.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'

/**
 * Connection whose deletion waits for confirmation, `null` if none: the
 * window is open while there is one.
 */
const connection = defineModel<ConnectionSummary | null>('connection', { required: true })

const emit = defineEmits<{
  /**
   * The connection is deleted.
   */
  removed: [connectionId: number]
  /**
   * The deletion failed, with the message of the error.
   */
  failed: [connectionId: number, message: string]
}>()

const connections = useConnectionsStore()
const editors = useEditorsStore()
const navigatorStore = useNavigatorStore()

/**
 * Is the window open?
 */
const open = computed({
  get: () => connection.value !== null,
  set: (isOpen) => {
    if (!isOpen) {
      connection.value = null
    }
  },
})

/**
 * Editors of the connection with a transaction open, which the deletion
 * rolls back.
 */
const transactionCount = computed(() =>
  connection.value === null ? 0 : editors.inTransaction(connection.value.id).length,
)

/**
 * Deletes the connection that was waiting for confirmation.
 */
async function confirm(): Promise<void> {
  const removed = connection.value
  connection.value = null
  if (removed === null) {
    return
  }
  try {
    await connections.remove(removed.id)
    navigatorStore.forgetSource(removed.id)
    emit('removed', removed.id)
  } catch (failure) {
    emit('failed', removed.id, errorMessage(failure))
  }
}
</script>

<template>
  <ui-dialog
    v-model:open="open"
    title="Supprimer la connexion"
    :description="`Supprimer la connexion « ${connection?.name} » ?`"
  >
    <p class="text-fg-secondary">
      Aucune donnée n'est perdue : seuls les paramètres de connexion enregistrés dans Prisme sont
      retirés, le serveur et ses bases ne sont pas touchés. La connexion pourra être recréée
      ultérieurement.
    </p>
    <ui-alert
      v-if="transactionCount > 0"
      tone="warning"
      :title="openTransactionsNotice(transactionCount, 'supprimer la connexion')"
    />
    <template #footer>
      <div class="flex w-full justify-end gap-3">
        <ui-button label="Annuler" @click="connection = null" />
        <ui-button
          :label="transactionCount > 0 ? 'Supprimer et annuler' : 'Supprimer'"
          variant="solid"
          tone="error"
          @click="confirm"
        />
      </div>
    </template>
  </ui-dialog>
</template>
