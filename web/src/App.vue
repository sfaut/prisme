<!--
  Disposition de l'écran : à gauche, l'explorateur des sources de données,
  dans un panneau dont on règle la largeur en faisant glisser son bord droit,
  avec le choix du thème (clair, sombre ou celui du système) ; à droite, les
  éditeurs SQL, un onglet chacun (`EditorTabBar`), chacun avec sa cellule
  (`QueryCell`) : sa connexion, sa session et ses onglets de résultat.
  Disposition en classes Tailwind.

  L'explorateur agit sur l'éditeur actif : la source choisie devient sa
  connexion, une table ouverte ou une base choisie (`USE`) s'y exécute. Les
  cellules de tous les éditeurs restent montées : chacune garde sa session
  (et ses transactions) ouverte, même quand un autre éditeur est affiché.

  Les messages éphémères (`notify`) s'affichent en bas à droite (`UiToaster`).
-->
<script setup lang="ts">
import { Database, Monitor, Moon, Sun } from 'lucide-vue-next'
import { TabsContent, TabsRoot } from 'reka-ui'
import { computed, nextTick, onMounted, ref, watch } from 'vue'

import { type ColorMode, colorMode } from './colorMode'
import { useConnectionsStore } from './stores/connections'
import { useEditorsStore } from './stores/editors'

import EditorTabBar from './components/EditorTabBar.vue'
import ObjectExplorer from './components/ObjectExplorer.vue'
import QueryCell from './components/QueryCell.vue'
import SiderResizer from './components/SiderResizer.vue'
import UiAlert from './components/ui/UiAlert.vue'
import UiSelect from './components/ui/UiSelect.vue'
import UiToaster from './components/ui/UiToaster.vue'
import { clampSiderWidth, loadSiderWidth } from './components/siderWidth'

const connections = useConnectionsStore()
const editors = useEditorsStore()

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
 * Erreur de chargement ou d'enregistrement des éditeurs, `null` si aucune.
 */
const editorsError = ref<string | null>(null)

/**
 * Base en cours de la session de chaque éditeur (elle change avec `USE`) ;
 * celle de l'éditeur actif est mise en gras dans l'explorateur.
 */
const databases = ref<Record<number, string | null>>({})

/**
 * Nombre de reconnexions demandées, par éditeur : changer la clef de sa
 * cellule rouvre sa session. Modifier sa connexion la rouvre aussi.
 */
const attempts = ref<Record<number, number>>({})

/**
 * Cellules montées, par éditeur, pour y lancer une requête depuis l'explorateur.
 */
const cells = new Map<number, InstanceType<typeof QueryCell>>()

/**
 * Éditeur actif, sous la forme attendue par les onglets de Reka UI.
 */
const activeTab = computed({
  get: () => editors.activeId ?? 0,
  set: (id: string | number) => editors.activate(Number(id)),
})

/**
 * Connexion d'un éditeur, `null` si aucune (ou supprimée).
 *
 * @param connectionId Identifiant de la connexion de l'éditeur.
 */
function connectionOf(connectionId: number | null) {
  return connections.connections.find((connection) => connection.id === connectionId) ?? null
}

/**
 * Signale un échec de chargement ou d'enregistrement des éditeurs.
 *
 * @param failure Erreur reçue.
 */
function report(failure: unknown): void {
  editorsError.value = failure instanceof Error ? failure.message : String(failure)
}

onMounted(() => {
  editors.load().catch(report)
})

// La source surlignée dans l'explorateur est la connexion de l'éditeur actif.
watch(
  () => editors.active?.connectionId ?? null,
  (connectionId) => {
    connections.selectedId = connectionId
  },
)

// Une connexion créée (ou supprimée) devient (ou cesse d'être) celle de l'éditeur actif.
watch(
  () => connections.selectedId,
  (connectionId) => {
    if (editors.active && editors.active.connectionId !== connectionId) {
      editors.setConnection(editors.active.id, connectionId).catch(report)
    }
  },
)

/**
 * Donne une connexion à l'éditeur actif, puis attend que sa cellule (rouverte
 * sur cette connexion) soit montée.
 *
 * @param connectionId Identifiant de la connexion.
 * @returns La cellule de l'éditeur actif, absente s'il n'y en a pas.
 */
async function activeCellOn(connectionId: number) {
  const editor = editors.active
  if (!editor) {
    return undefined
  }
  await editors.setConnection(editor.id, connectionId)
  await nextTick()
  return cells.get(editor.id)
}

/**
 * Affiche le contenu d'une table choisie dans l'explorateur, dans l'éditeur
 * actif. Une table d'une autre source donne d'abord cette connexion à
 * l'éditeur : la requête part dès que sa session est ouverte.
 *
 * @param connectionId Identifiant de la connexion de la table.
 * @param schema Nom de la base.
 * @param table Nom de la table.
 */
async function openTable(connectionId: number, schema: string, table: string): Promise<void> {
  try {
    ;(await activeCellOn(connectionId))?.showTableWhenReady(schema, table)
  } catch (failure) {
    report(failure)
  }
}

/**
 * Fait d'une base choisie dans l'explorateur la base en cours de la session
 * de l'éditeur actif (comme `USE`). Une base d'une autre source donne d'abord
 * cette connexion à l'éditeur.
 *
 * @param connectionId Identifiant de la connexion de la base.
 * @param schema Nom de la base.
 */
async function useDatabase(connectionId: number, schema: string): Promise<void> {
  try {
    ;(await activeCellOn(connectionId))?.changeDatabaseWhenReady(schema)
  } catch (failure) {
    report(failure)
  }
}

/**
 * Choisit une source comme connexion de l'éditeur actif.
 *
 * @param connectionId Identifiant de la connexion.
 */
function selectSource(connectionId: number): void {
  if (editors.active) {
    editors.setConnection(editors.active.id, connectionId).catch(report)
  }
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
        :current-database="editors.activeId === null ? null : (databases[editors.activeId] ?? null)"
        @open-table="openTable"
        @select="selectSource"
        @use-database="useDatabase"
      />
    </aside>
    <sider-resizer v-model="siderWidth" />
    <main class="flex h-full min-w-0 flex-1 flex-col gap-gap p-panel">
      <ui-alert
        v-if="editorsError"
        tone="error"
        :title="editorsError"
        closable
        @close="editorsError = null"
      />
      <tabs-root
        v-model="activeTab"
        :unmount-on-hide="false"
        class="flex min-h-0 flex-1 flex-col gap-gap"
      >
        <editor-tab-bar />
        <tabs-content
          v-for="editor in editors.editors"
          :key="editor.id"
          :value="editor.id"
          class="flex min-h-0 flex-1 flex-col outline-none"
        >
          <query-cell
            v-if="connectionOf(editor.connectionId)"
            :ref="
              (cell) =>
                cell
                  ? cells.set(editor.id, cell as InstanceType<typeof QueryCell>)
                  : cells.delete(editor.id)
            "
            :key="`${editor.connectionId}-${connections.versions[editor.connectionId!] ?? 0}-${attempts[editor.id] ?? 0}`"
            :editor-id="editor.id"
            :connection="connectionOf(editor.connectionId)!"
            :sql="editor.sql"
            @update:sql="(sql) => editors.setSql(editor.id, sql)"
            @reconnect="attempts[editor.id] = (attempts[editor.id] ?? 0) + 1"
            @database="(database) => (databases[editor.id] = database)"
          />
          <div v-else class="m-auto flex max-w-xs flex-col items-center gap-gap text-center">
            <span class="grid size-9 place-items-center rounded-full bg-surface-elevated">
              <Database class="size-4 text-fg-muted" />
            </span>
            <p class="text-balance text-fg-muted">
              Double-cliquez sur une source de données pour la choisir pour cet éditeur, ou ajoutez
              une connexion par le menu « ⋮ »
            </p>
          </div>
        </tabs-content>
      </tabs-root>
    </main>
  </div>
  <ui-toaster />
</template>
