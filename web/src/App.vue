<!--
  Layout of the screen: on top, the application bar (`AppTopBar`), across the
  whole width; below, on the left, the object explorer, in a panel whose
  width is set by dragging its right edge; in the middle, the SQL editors,
  one tab each (`EditorTabBar`), each with its cell (`QueryCell`): its
  connection, its session and its result tabs; on the right, the data
  explorer (`DataExplorer`), whose width is set by dragging its left edge.
  Both side panels go from the top bar down to the bottom of the window.
  Under the editors, between the side panels, the console
  (`ConsolePanel`), whose height is set by dragging its top edge. Each
  panel folds with the button of its header (`PanelHeader`) and leaves a
  rail on its edge, which opens it again (`PanelRail`): the object explorer
  and the console are open by default (`layout` and `console` stores), the
  data explorer closed. Layout in Tailwind classes.

  The explorer acts on the active editor: the chosen source becomes its
  connection, an opened table or a chosen database (`USE`) runs there. The
  cells of all the editors stay mounted: each one keeps its session (and its
  transactions) open, even when another editor is displayed.

  Transient messages (`notify`) show at the bottom right (`UiToaster`); the
  tooltips of every element (`v-tooltip`), in a single bubble (`UiTooltip`).
-->
<script setup lang="ts">
import { errorMessage } from '@/api/errorMessage'
import { Database } from '@lucide/vue'
import { TabsContent, TabsRoot } from 'reka-ui'
import { computed, nextTick, onMounted, ref, watch } from 'vue'

import { useConnectionsStore } from './stores/connections'
import { useConsoleStore } from './stores/console'
import { useDataExplorerStore } from './stores/dataExplorer'
import { useEditorsStore } from './stores/editors'
import { useLayoutStore } from './stores/layout'

import AppTopBar from './components/AppTopBar.vue'
import ConsolePanel from './components/ConsolePanel.vue'
import DataExplorer from './components/DataExplorer.vue'
import EditorTabBar from './components/EditorTabBar.vue'
import ObjectExplorer from './components/ObjectExplorer.vue'
import QueryCell from './components/QueryCell.vue'
import PanelRail from './components/PanelRail.vue'
import PanelResizer from './components/PanelResizer.vue'
import UiAlert from './components/ui/UiAlert.vue'
import UiToaster from './components/ui/UiToaster.vue'
import UiTooltip from './components/ui/UiTooltip.vue'
import { CONSOLE, DATA_PANEL, SIDER, clampPanelSize, loadPanelSize } from './components/panelSize'

const connections = useConnectionsStore()
const editors = useEditorsStore()
const explorer = useDataExplorerStore()
const layout = useLayoutStore()
const consoleStore = useConsoleStore()

/**
 * Width of the left panel, in pixels.
 */
const siderWidth = ref(clampPanelSize(SIDER, loadPanelSize(SIDER), window.innerWidth))

/**
 * Width of the data explorer, in pixels.
 */
const dataPanelWidth = ref(clampPanelSize(DATA_PANEL, loadPanelSize(DATA_PANEL), window.innerWidth))

/**
 * Height of the console, in pixels.
 */
const consoleHeight = ref(clampPanelSize(CONSOLE, loadPanelSize(CONSOLE), window.innerHeight))

/**
 * Error loading or saving the editors, `null` if none.
 */
const editorsError = ref<string | null>(null)

/**
 * Current database of each editor's session (it changes with `USE`); that
 * of the active editor is shown in bold in the explorer.
 */
const databases = ref<Record<number, string | null>>({})

/**
 * Number of reconnections requested, by editor: changing the key of its cell
 * reopens its session. Changing its connection reopens it too.
 */
const attempts = ref<Record<number, number>>({})

/**
 * Mounted cells, by editor, to start a query there from the explorer.
 */
const cells = new Map<number, InstanceType<typeof QueryCell>>()

/**
 * Active editor, in the form expected by Reka UI's tabs.
 */
const activeTab = computed({
  get: () => editors.activeId ?? 0,
  set: (id: string | number) => editors.activate(Number(id)),
})

/**
 * Connection of an editor, `null` if none (or deleted).
 *
 * @param connectionId Id of the editor's connection.
 */
function connectionOf(connectionId: number | null) {
  return connections.connections.find((connection) => connection.id === connectionId) ?? null
}

/**
 * Reports a failure loading or saving the editors.
 *
 * @param failure Received error.
 */
function report(failure: unknown): void {
  editorsError.value = errorMessage(failure)
}

onMounted(() => {
  editors.load().catch(report)
})

// The source highlighted in the explorer is the connection of the active editor.
watch(
  () => editors.active?.connectionId ?? null,
  (connectionId) => {
    connections.selectedId = connectionId
  },
)

// A created (or deleted) connection becomes (or stops being) that of the active editor.
watch(
  () => connections.selectedId,
  (connectionId) => {
    if (editors.active && editors.active.connectionId !== connectionId) {
      editors.setConnection(editors.active.id, connectionId).catch(report)
    }
  },
)

/**
 * Gives a connection to the active editor, then waits for its cell (reopened
 * on this connection) to be mounted.
 *
 * @param connectionId Id of the connection.
 * @returns The cell of the active editor, absent if there is none.
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
 * Shows the content of a table chosen in the explorer, in the active editor.
 * A table of another source first gives this connection to the editor: the
 * query starts as soon as its session is open.
 *
 * @param connectionId Id of the table's connection.
 * @param schema Name of the database.
 * @param table Name of the table.
 */
async function openTable(connectionId: number, schema: string, table: string): Promise<void> {
  try {
    ;(await activeCellOn(connectionId))?.showTableWhenReady(schema, table)
  } catch (failure) {
    report(failure)
  }
}

/**
 * Writes the operations on an index chosen in the explorer, in the active
 * editor, as `openTable` does for a table.
 *
 * @param connectionId Id of the index's connection.
 * @param schema Name of the database.
 * @param table Name of the table.
 * @param index Name of the index.
 */
async function writeIndexOperations(
  connectionId: number,
  schema: string,
  table: string,
  index: string,
): Promise<void> {
  try {
    ;(await activeCellOn(connectionId))?.writeIndexOperationsWhenReady(schema, table, index)
  } catch (failure) {
    report(failure)
  }
}

/**
 * Makes a database chosen in the explorer the current database of the active
 * editor's session (like `USE`). A database of another source first gives
 * this connection to the editor.
 *
 * @param connectionId Id of the database's connection.
 * @param schema Name of the database.
 */
async function useDatabase(connectionId: number, schema: string): Promise<void> {
  try {
    ;(await activeCellOn(connectionId))?.changeDatabaseWhenReady(schema)
  } catch (failure) {
    report(failure)
  }
}

/**
 * Chooses a source as the connection of the active editor.
 *
 * @param connectionId Id of the connection.
 */
function selectSource(connectionId: number): void {
  if (editors.active) {
    editors.setConnection(editors.active.id, connectionId).catch(report)
  }
}
</script>

<template>
  <div class="flex h-full flex-col">
    <app-top-bar />
    <div class="relative flex min-h-0 flex-1">
      <!-- Folded, the object explorer stays mounted: its tree keeps its state. -->
      <aside
        v-show="layout.objectsOpen"
        class="flex shrink-0 flex-col gap-3 border-e border-line p-3"
        :style="{ width: `${siderWidth}px` }"
      >
        <object-explorer
          :selected-id="connections.selectedId"
          :current-database="
            editors.activeId === null ? null : (databases[editors.activeId] ?? null)
          "
          @open-table="openTable"
          @index-operations="writeIndexOperations"
          @select="selectSource"
          @use-database="useDatabase"
        />
      </aside>
      <panel-resizer v-if="layout.objectsOpen" v-model="siderWidth" :panel="SIDER" side="start" />
      <panel-rail
        v-else
        title="Explorateur d’objets"
        side="start"
        @expand="layout.objectsOpen = true"
      />
      <div class="relative flex min-w-0 flex-1 flex-col">
        <main class="flex min-h-0 min-w-0 flex-1 flex-col gap-2 p-3">
          <ui-alert
            v-if="editorsError"
            tone="error"
            :title="editorsError"
            @close="editorsError = null"
          />
          <tabs-root
            v-model="activeTab"
            :unmount-on-hide="false"
            class="flex min-h-0 flex-1 flex-col gap-2"
          >
            <editor-tab-bar />
            <tabs-content
              v-for="editor in editors.editors"
              :key="editor.id"
              :value="editor.id"
              class="flex min-h-0 flex-1 flex-col outline-none"
              :class="{ 'items-center justify-center': !connectionOf(editor.connectionId) }"
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
                :active="editor.id === editors.activeId"
                :connection="connectionOf(editor.connectionId)!"
                :sql="editor.sql"
                @update:sql="(sql) => editors.setSql(editor.id, sql)"
                @reconnect="attempts[editor.id] = (attempts[editor.id] ?? 0) + 1"
                @database="(database) => (databases[editor.id] = database)"
              />
              <div v-else class="flex max-w-xs flex-col items-center gap-2 text-center">
                <span class="grid size-9 place-items-center rounded-full bg-surface-elevated">
                  <Database class="size-4 text-fg-muted" />
                </span>
                <p class="text-balance text-fg-muted">
                  Faites Ctrl+clic (Cmd+clic sous macOS) sur une source de données pour la choisir
                  pour cet éditeur, ou ajoutez une connexion par le menu « ⋮ »
                </p>
              </div>
            </tabs-content>
          </tabs-root>
        </main>
        <template v-if="consoleStore.open">
          <panel-resizer v-model="consoleHeight" :panel="CONSOLE" side="bottom" />
          <section
            class="shrink-0 border-t border-line px-3 pt-1 pb-3"
            :style="{ height: `${consoleHeight}px` }"
          >
            <console-panel />
          </section>
        </template>
        <panel-rail v-else title="Console" side="bottom" @expand="consoleStore.open = true" />
      </div>
      <template v-if="explorer.open">
        <panel-resizer v-model="dataPanelWidth" :panel="DATA_PANEL" side="end" />
        <aside
          class="flex shrink-0 flex-col border-s border-line p-3"
          :style="{ width: `${dataPanelWidth}px` }"
        >
          <data-explorer />
        </aside>
      </template>
      <panel-rail v-else title="Explorateur de données" side="end" @expand="explorer.open = true" />
    </div>
  </div>
  <ui-toaster />
  <ui-tooltip />
</template>
