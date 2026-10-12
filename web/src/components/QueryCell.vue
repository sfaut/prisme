<!--
  Cellule SQL : éditeur, exécution, limite de lignes, état et résultats.

  La cellule ouvre une session de requêtes sur la connexion choisie et la
  garde ouverte tant qu'elle est affichée. Ctrl-Entrée ou le bouton
  « Exécuter » envoie tout le texte de l'éditeur. La limite de lignes choisie
  ici devient la limite par défaut de l'utilisateur.
-->
<script setup lang="ts">
import { NAlert, NButton, NSelect, NTag } from 'naive-ui'
import { computed, onMounted, watch } from 'vue'

import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import { useQuerySession } from '@/api/querySession'
import { useSettingsStore } from '@/stores/settings'

import ResultGrid from './ResultGrid.vue'
import SqlEditor from './SqlEditor.vue'

const props = defineProps<{
  /**
   * Connexion sur laquelle s'exécutent les requêtes.
   */
  connection: ConnectionSummary
}>()

const emit = defineEmits<{
  /**
   * La session est fermée et l'utilisateur demande à la rouvrir.
   */
  reconnect: []
  /**
   * La base en cours a changé (connexion, `USE`) ; `null` si aucune.
   */
  database: [database: string | null]
}>()

/**
 * Texte SQL de la cellule.
 */
const sql = defineModel<string>('sql', { required: true })

const session = useQuerySession(props.connection.id)
const state = session.state
const settings = useSettingsStore()

/**
 * Choix de la limite de lignes, formatés pour la liste.
 */
const limitOptions = computed(() =>
  (settings.settings?.row_limit_choices ?? []).map((choice) => ({
    label: `${choice.toLocaleString('fr-FR')} lignes`,
    value: choice,
  })),
)

/**
 * Variante et version du serveur, pour l'en-tête de la cellule.
 */
const serverLabel = computed(() => {
  const server = state.server
  if (!server) {
    return 'Connexion…'
  }
  return `${server.flavor === 'mariadb' ? 'MariaDB' : 'MySQL'} ${server.version}`
})

/**
 * Exécute le texte de l'éditeur.
 */
function run(): void {
  if (sql.value.trim() !== '') {
    session.run(sql.value)
  }
}

/**
 * Change la limite de lignes par défaut de l'utilisateur.
 *
 * @param rowLimit Nouvelle limite.
 */
async function changeLimit(rowLimit: number): Promise<void> {
  try {
    await settings.setRowLimit(rowLimit)
  } catch (failure) {
    state.error = failure instanceof Error ? failure.message : String(failure)
  }
}

onMounted(async () => {
  if (settings.settings === null) {
    try {
      await settings.load()
    } catch (failure) {
      state.error = failure instanceof Error ? failure.message : String(failure)
    }
  }
})

watch(
  () => state.database,
  (database) => emit('database', database),
)

defineExpose({ run })
</script>

<template>
  <div class="query-cell">
    <sql-editor v-model="sql" :flavor="state.server?.flavor ?? 'mysql'" @run="run" />

    <div class="cell-toolbar">
      <n-button
        type="primary"
        size="small"
        :disabled="state.status !== 'ready' && state.status !== 'running'"
        :loading="state.status === 'running'"
        @click="run"
      >
        Exécuter
      </n-button>
      <n-select
        :value="settings.settings?.row_limit"
        :options="limitOptions"
        size="small"
        class="cell-limit"
        @update:value="changeLimit"
      />
      <span class="cell-server">{{ connection.name }} · {{ serverLabel }}</span>
    </div>

    <n-alert v-if="state.status === 'closed'" type="warning" :show-icon="false">
      Session fermée.
      <n-button size="tiny" @click="emit('reconnect')">Reconnecter</n-button>
    </n-alert>
    <n-alert v-if="state.error" type="error" :show-icon="false">{{ state.error }}</n-alert>

    <template v-if="state.result">
      <div class="cell-status">
        <span v-if="state.result.done">
          {{ state.result.rows.length.toLocaleString('fr-FR') }} lignes ·
          {{ state.result.elapsedMs }} ms
        </span>
        <span v-else>{{ state.result.rows.length.toLocaleString('fr-FR') }} lignes reçues…</span>
        <n-tag v-if="state.result.truncated" type="warning" size="small">
          Résultat tronqué à {{ state.result.rows.length.toLocaleString('fr-FR') }} lignes
        </n-tag>
        <code v-if="state.result.limit === 'added'" class="cell-sql" :title="state.result.sql">
          {{ state.result.sql }}
        </code>
      </div>
      <result-grid
        v-if="state.result.columns.length > 0"
        :columns="state.result.columns"
        :rows="state.result.rows"
      />
    </template>
  </div>
</template>

<style scoped>
.query-cell {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 8px;
  min-height: 0;
}

.cell-toolbar,
.cell-status {
  display: flex;
  align-items: center;
  gap: 8px;
}

.cell-limit {
  width: 150px;
}

.cell-server,
.cell-status {
  color: #666;
  font-size: 0.8rem;
}

.cell-sql {
  overflow: hidden;
  font-size: 0.75rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
