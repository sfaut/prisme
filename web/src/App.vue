<!--
  Disposition de l'écran : à gauche, l'explorateur des sources de données,
  dans un panneau dont on règle la largeur en faisant glisser son bord droit,
  avec le choix du thème (clair, sombre ou celui du système) ; à droite, la
  cellule SQL et ses résultats. Disposition en classes Tailwind.

  Les messages éphémères (`notify`) s'affichent en bas à droite (`UiToaster`).
-->
<script setup lang="ts">
import { Database, Monitor, Moon, Sun } from 'lucide-vue-next'
import { nextTick, ref, useTemplateRef, watch } from 'vue'

import { type ColorMode, colorMode } from './colorMode'
import { useConnectionsStore } from './stores/connections'

import ObjectExplorer from './components/ObjectExplorer.vue'
import QueryCell from './components/QueryCell.vue'
import SiderResizer from './components/SiderResizer.vue'
import UiSelect from './components/ui/UiSelect.vue'
import UiToaster from './components/ui/UiToaster.vue'
import { selectAllQuery } from './components/explorerNodes'
import { clampSiderWidth, loadSiderWidth } from './components/siderWidth'

const connections = useConnectionsStore()

/**
 * Choix du thème, dans la liste en tête du panneau de gauche.
 */
const COLOR_MODES: { label: string; value: ColorMode; icon: typeof Monitor }[] = [
  { label: 'Système', value: 'system', icon: Monitor },
  { label: 'Clair', value: 'light', icon: Sun },
  { label: 'Sombre', value: 'dark', icon: Moon },
]

/**
 * Largeur du panneau de gauche, en pixels.
 */
const siderWidth = ref(clampSiderWidth(loadSiderWidth(), window.innerWidth))

/**
 * Texte de la cellule SQL.
 */
const sql = ref('')

/**
 * Base en cours de la session de requêtes (elle change avec `USE`), mise en
 * gras dans l'explorateur.
 */
const currentDatabase = ref<string | null>(null)

// Nouvelle connexion : sa base en cours n'est connue qu'à l'ouverture de sa session.
watch(
  () => connections.selectedId,
  () => {
    currentDatabase.value = null
  },
)

/**
 * Nombre de reconnexions demandées : changer la clef de la cellule rouvre sa
 * session. Modifier la connexion la rouvre aussi.
 */
const attempt = ref(0)

const cell = useTemplateRef<InstanceType<typeof QueryCell>>('cell')

/**
 * Affiche le contenu d'une table choisie dans l'explorateur. Une table d'une
 * autre source choisit d'abord cette connexion : la requête part dès que la
 * session de la nouvelle cellule est ouverte.
 *
 * @param connectionId Identifiant de la connexion de la table.
 * @param schema Nom de la base.
 * @param table Nom de la table.
 */
async function openTable(connectionId: number, schema: string, table: string): Promise<void> {
  sql.value = selectAllQuery(schema, table)
  connections.selectedId = connectionId
  await nextTick()
  cell.value?.runWhenReady(0)
}

/**
 * Fait d'une base choisie dans l'explorateur la base en cours de la session
 * (comme `USE`). Une base d'une autre source choisit d'abord cette connexion :
 * le changement part dès que la session de la nouvelle cellule est ouverte.
 *
 * @param connectionId Identifiant de la connexion de la base.
 * @param schema Nom de la base.
 */
async function useDatabase(connectionId: number, schema: string): Promise<void> {
  connections.selectedId = connectionId
  await nextTick()
  cell.value?.changeDatabaseWhenReady(schema)
}
</script>

<template>
  <div class="relative flex h-full">
    <aside
      class="flex shrink-0 flex-col gap-panel border-e border-line p-panel"
      :style="{ width: `${siderWidth}px` }"
    >
      <header class="flex items-center justify-between gap-gap">
        <h1 class="text-heading font-semibold text-fg-strong">Prisme</h1>
        <ui-select v-model="colorMode" :items="COLOR_MODES" label="Thème" class="w-28" />
      </header>
      <object-explorer
        :selected-id="connections.selectedId"
        :current-database="currentDatabase"
        @open-table="openTable"
        @select="connections.selectedId = $event"
        @use-database="useDatabase"
      />
    </aside>
    <sider-resizer v-model="siderWidth" />
    <main class="flex h-full min-w-0 flex-1 flex-col gap-panel p-panel">
      <query-cell
        v-if="connections.selected"
        ref="cell"
        :key="`${connections.selected.id}-${connections.versions[connections.selected.id] ?? 0}-${attempt}`"
        v-model:sql="sql"
        :connection="connections.selected"
        @reconnect="attempt++"
        @database="currentDatabase = $event"
      />
      <div v-else class="m-auto flex max-w-xs flex-col items-center gap-gap text-center">
        <span class="grid size-9 place-items-center rounded-full bg-surface-elevated">
          <Database class="size-4 text-fg-muted" />
        </span>
        <p class="text-balance text-fg-muted">
          Double-cliquez sur une source de données pour la choisir, ou ajoutez une connexion par le
          menu « ⋮ »
        </p>
      </div>
    </main>
  </div>
  <ui-toaster />
</template>
