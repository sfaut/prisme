<!--
  Cellule SQL : éditeur, exécution, limite de lignes, état et résultats.

  La cellule ouvre une session de requêtes sur la connexion choisie et la
  garde ouverte tant qu'elle est affichée. Ctrl-Entrée ou « Exécuter » lance
  la requête sous le curseur ; Ctrl-Maj-Entrée ou « Tout exécuter » lance
  toutes les requêtes en parallèle. Chaque requête a son onglet de résultat.
  La limite de lignes choisie ici devient la limite par défaut de l'utilisateur.
-->
<script setup lang="ts">
import { NAlert, NButton, NSelect, NTabPane, NTabs } from 'naive-ui'
import { computed, h, onMounted, ref, useTemplateRef, watch } from 'vue'

import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import { type QueryTab, tabProgress, useQuerySession } from '@/api/querySession'
import { useSettingsStore } from '@/stores/settings'

import ProgressDot from './ProgressDot.vue'
import QueryTabView from './QueryTabView.vue'
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

const editor = useTemplateRef<InstanceType<typeof SqlEditor>>('editor')

/**
 * Onglet affiché : numéro de sa requête.
 */
const selectedTab = ref(0)

// Nouveau lot : le premier onglet s'affiche.
watch(
  () => state.tabs,
  () => {
    selectedTab.value = 0
  },
)

/**
 * Libellé d'un onglet : numéro et début de la requête.
 *
 * @param query Numéro de la requête.
 * @param text Texte de la requête.
 */
function tabLabel(query: number, text: string): string {
  const short = text.replace(/\s+/g, ' ')
  return `${query + 1} · ${short.length > 40 ? `${short.slice(0, 40)}…` : short}`
}

/**
 * Titre d'un onglet : pastille d'avancement, puis libellé.
 *
 * @param tab Onglet de la requête.
 */
function tabTitle(tab: QueryTab) {
  return () => [h(ProgressDot, { progress: tabProgress(tab) }), tabLabel(tab.query, tab.sql)]
}

/**
 * Exécute la requête sous le curseur.
 *
 * @param cursor Position du curseur ; absente, celle de l'éditeur.
 */
function run(cursor?: number): void {
  if (sql.value.trim() !== '') {
    session.run(sql.value, cursor ?? editor.value?.cursor() ?? 0)
  }
}

/**
 * Exécute toutes les requêtes de l'éditeur, en parallèle.
 */
function runAll(): void {
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
    <sql-editor
      ref="editor"
      v-model="sql"
      :flavor="state.server?.flavor ?? 'mysql'"
      @run="run"
      @run-all="runAll"
    />

    <div class="cell-toolbar">
      <n-button
        type="primary"
        size="small"
        :disabled="state.status !== 'ready' && state.status !== 'running'"
        :loading="state.status === 'running'"
        @click="run()"
      >
        Exécuter
      </n-button>
      <n-button
        size="small"
        :disabled="state.status !== 'ready'"
        title="Toutes les requêtes, en parallèle (Ctrl-Maj-Entrée)"
        @click="runAll"
      >
        Tout exécuter
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

    <n-tabs
      v-if="state.tabs.length > 0"
      v-model:value="selectedTab"
      type="line"
      size="small"
      class="cell-tabs"
    >
      <n-tab-pane
        v-for="tab in state.tabs"
        :key="tab.query"
        :name="tab.query"
        :tab="tabTitle(tab)"
        display-directive="show"
        class="cell-tab-pane"
      >
        <query-tab-view :tab="tab" />
      </n-tab-pane>
    </n-tabs>
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

.cell-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
}

.cell-limit {
  width: 150px;
}

.cell-server {
  color: #666;
  font-size: 0.8rem;
}

.cell-tabs {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
}

.cell-tabs :deep(.n-tabs-pane-wrapper) {
  display: flex;
  flex: 1;
  min-height: 0;
}

.cell-tab-pane {
  display: flex;
  flex: 1;
  min-height: 0;
}
</style>
