<!--
  Layout of the screen: on top, the application bar (`AppTopBar`), across the
  whole width; below, on the left, the object navigator, in a panel whose
  width is set by dragging its right edge; in the middle, the SQL editors,
  one tab each (`EditorTabBar`), each with its cell (`QueryCell`): its
  connection, its session and its result tabs; on the right, the data
  explorer (`DataExplorer`), whose width is set by dragging its left edge.
  Both side panels go from the top bar down to the bottom of the window.
  Under the editors, between the side panels, the console
  (`ConsolePanel`). The editors and the console share the height in a
  splitter of Reka UI (`SplitterGroup`): dragging the line between them
  sets the height of the console, within the bounds of both (the editors
  keep `--editors-min-height`), and the splitter remembers it. Each panel
  folds with the button of its header (`PanelHeader`) and leaves a rail on
  its edge, which opens it again (`PanelRail`); the console also folds by
  dragging it below its smallest height, or by a double click on the line.
  The object navigator and the console are open by default (`layout` and
  `console` stores), the data explorer closed. Layout in Tailwind
  classes.

  The navigator acts on the active editor: the chosen source becomes its
  connection, an opened table or a chosen database (`USE`) runs there, and
  "Déconnecter" closes its session. The cells of all the editors stay
  mounted: each one keeps its session (and its transactions) open, even when
  another editor is displayed.

  Transient messages (`notify`) show at the bottom right (`UiToaster`); the
  tooltips of every element (`v-tooltip`), in a single bubble (`UiTooltip`).
-->
<script setup lang="ts">
import { errorMessage } from '@/api/errorMessage'
import { Database } from '@lucide/vue'
import { SplitterGroup, SplitterPanel, TabsContent, TabsRoot } from 'reka-ui'
import { computed, nextTick, onMounted, ref, useTemplateRef, watch } from 'vue'

import type { SessionStatus } from './api/querySession'

import { useConnectionsStore } from './stores/connections'
import { useConsoleStore } from './stores/console'
import { useDataExplorerStore } from './stores/dataExplorer'
import { useEditorsStore } from './stores/editors'
import { useLayoutStore } from './stores/layout'

import AppTopBar from './components/AppTopBar.vue'
import ConsolePanel from './components/ConsolePanel.vue'
import DataExplorer from './components/DataExplorer.vue'
import EditorTabBar from './components/EditorTabBar.vue'
import ObjectNavigator from './components/ObjectNavigator.vue'
import QueryCell from './components/QueryCell.vue'
import PanelRail from './components/PanelRail.vue'
import PanelResizer from './components/PanelResizer.vue'
import SplitHandle from './components/SplitHandle.vue'
import UiAlert from './components/ui/UiAlert.vue'
import UiToaster from './components/ui/UiToaster.vue'
import UiTooltip from './components/ui/UiTooltip.vue'
import { DATA_PANEL, SIDER, clampPanelSize, loadPanelSize } from './components/panelSize'
import { cssPixels } from './cssLength'

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
 * Heights of the central splitter, in pixels (`main.css`): smallest area of
 * the editors; default and smallest console; folded console, its rail.
 */
const EDITORS_MIN_HEIGHT = cssPixels('--editors-min-height', 312)
const CONSOLE_HEIGHT = cssPixels('--console-height', 220)
const CONSOLE_MIN_HEIGHT = cssPixels('--console-min-height', 120)
const RAIL_SIZE = cssPixels('--rail-size', 28)

/**
 * Panel of the console in the central splitter.
 */
const consoleSplit = useTemplateRef<InstanceType<typeof SplitterPanel>>('consoleSplit')

/**
 * Folds or unfolds the console's panel as the console's state says.
 *
 * @param open Is the console open?
 */
function showConsole(open: boolean): void {
  if (open) {
    consoleSplit.value?.expand()
  } else {
    consoleSplit.value?.collapse()
  }
}

// The console's state (header button, rail, double click) drives its panel;
// the panel, folded or unfolded by a drag, updates the state in return. On
// each resize, from Reka UI's `isCollapsed`: its `collapse` event compares
// sizes in pixels to ten decimals and misses a fold by a drag.
watch(() => consoleStore.open, showConsole)
onMounted(() => nextTick(() => showConsole(consoleStore.open)))

/**
 * Error loading or saving the editors, `null` if none.
 */
const editorsError = ref<string | null>(null)

/**
 * Current database of each editor's session (it changes with `USE`); that
 * of the active editor is shown in bold in the navigator.
 */
const databases = ref<Record<number, string | null>>({})

/**
 * Stage of each editor's session; that of the active editor enables
 * "Déconnecter" in the navigator.
 */
const statuses = ref<Record<number, SessionStatus>>({})

/**
 * Stage of the active editor's session, `null` without a session (no editor,
 * or an editor without a connection).
 */
const activeStatus = computed(() => {
  const editor = editors.active
  if (!editor || !connectionOf(editor.connectionId)) {
    return null
  }
  return statuses.value[editor.id] ?? null
})

/**
 * Current database of the active editor's session, `null` if none or if the
 * session is closed.
 */
const activeDatabase = computed(() => {
  if (editors.activeId === null || activeStatus.value === 'closed') {
    return null
  }
  return databases.value[editors.activeId] ?? null
})

/**
 * Number of reconnections requested, by editor: changing the key of its cell
 * reopens its session. Changing its connection reopens it too.
 */
const attempts = ref<Record<number, number>>({})

/**
 * Mounted cells, by editor, to start a query there from the navigator.
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

// The source highlighted in the navigator is the connection of the active editor.
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
 * Shows the content of a table chosen in the navigator, in the active editor.
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
 * Writes the operations on an index chosen in the navigator, in the active
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
 * Makes a database chosen in the navigator the current database of the active
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
 * Closes the session of the active editor ("Déconnecter" of the navigator).
 */
function disconnect(): void {
  if (editors.activeId !== null) {
    void cells.get(editors.activeId)?.disconnect()
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
      <!-- Folded, the object navigator stays mounted: its tree keeps its state. -->
      <aside
        v-show="layout.objectsOpen"
        class="flex shrink-0 flex-col gap-3 border-e border-line p-3"
        :style="{ width: `${siderWidth}px` }"
      >
        <object-navigator
          :selected-id="connections.selectedId"
          :current-database="activeDatabase"
          :session-status="activeStatus"
          @open-table="openTable"
          @index-operations="writeIndexOperations"
          @select="selectSource"
          @use-database="useDatabase"
          @disconnect="disconnect"
        />
      </aside>
      <panel-resizer v-if="layout.objectsOpen" v-model="siderWidth" :panel="SIDER" side="start" />
      <panel-rail
        v-else
        title="Navigateur d’objets"
        side="start"
        @expand="layout.objectsOpen = true"
      />
      <splitter-group
        direction="vertical"
        auto-save-id="prisme.centerLayout"
        class="min-w-0 flex-1"
      >
        <splitter-panel :order="1" size-unit="px" :min-size="EDITORS_MIN_HEIGHT" class="flex">
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
                class="flex min-h-0 flex-1 flex-col"
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
                  @status="(status) => (statuses[editor.id] = status)"
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
        </splitter-panel>
        <split-handle
          tooltip="Glisser pour redimensionner la console, double clic pour la replier ou la déplier"
          @toggle="consoleStore.open = !consoleStore.open"
        />
        <splitter-panel
          ref="consoleSplit"
          :order="2"
          size-unit="px"
          :default-size="CONSOLE_HEIGHT"
          :min-size="CONSOLE_MIN_HEIGHT"
          collapsible
          :collapsed-size="RAIL_SIZE"
          class="flex"
          @resize="consoleStore.open = !consoleSplit?.isCollapsed"
        >
          <section
            v-if="consoleStore.open"
            class="flex min-h-0 min-w-0 flex-1 flex-col px-3 pt-1 pb-3"
          >
            <console-panel />
          </section>
          <panel-rail
            v-else
            title="Console"
            side="bottom"
            class="flex-1"
            @expand="consoleStore.open = true"
          />
        </splitter-panel>
      </splitter-group>
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
