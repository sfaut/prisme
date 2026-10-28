<!--
  Layout of the screen: on top, the application bar (`AppTopBar`), across the
  whole width; below, three columns: on the left, the object navigator; in
  the middle, the SQL editors, one tab each (`EditorTabBar`), each with its
  cell (`QueryCell`): its connection, its session and its result tabs; on
  the right, the data explorer (`DataExplorer`). Both side panels go from
  the top bar down to the bottom of the window. Under the editors, between
  the side panels, the console (`ConsolePanel`). The columns share the
  width, and the editors and the console the height, in splitters of Reka
  UI (`SplitterGroup`): dragging the line between two panels moves it
  within their bounds (the editors keep `--editors-min-width` and
  `--editors-min-height`); dragged further, it shrinks the next panel too,
  down to folding it, and the splitters remember the layout. Each side panel
  and the console fold with the button of their header (`PanelHeader`), by
  a drag below their smallest size or by a double click on their line, and
  leave a rail on their edge, which opens them again (`PanelRail`). The
  object navigator and the console are open by default (`layout` and
  `console` stores), the data explorer closed. Layout in Tailwind
  classes.

  The navigator acts on the active editor: the chosen source becomes its
  connection, an opened table or a chosen database (`USE`) runs there, and
  "Déconnecter" closes its session, which any of these actions on its source
  opens again. The cells of all the editors stay mounted: each one keeps its
  session (and its transactions) open, even when another editor is
  displayed. Closing a session rolls its open transaction back: while one
  is open, "Déconnecter", closing the editor and changing its source ask
  for a confirmation first (changing or deleting the connection is
  confirmed in its own window).

  Transient messages (`notify`) show at the bottom right (`UiToaster`); the
  tooltips of every element (`v-tooltip`), in a single bubble (`UiTooltip`).
-->
<script setup lang="ts">
import { errorMessage } from '@/api/errorMessage'
import { Database, PanelTopClose, PanelTopOpen } from '@lucide/vue'
import { SplitterGroup, SplitterPanel, TabsContent, TabsRoot } from 'reka-ui'
import { computed, nextTick, onMounted, ref, shallowReactive, useTemplateRef, watch } from 'vue'

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
import SplitHandle from './components/SplitHandle.vue'
import UiAlert from './components/ui/UiAlert.vue'
import UiButton from './components/ui/UiButton.vue'
import UiDialog from './components/ui/UiDialog.vue'
import UiToaster from './components/ui/UiToaster.vue'
import UiTooltip from './components/ui/UiTooltip.vue'
import { vTooltip } from './components/ui/tooltip'
import { cssPixels } from './cssLength'

const connections = useConnectionsStore()
const editors = useEditorsStore()
const explorer = useDataExplorerStore()
const layout = useLayoutStore()
const consoleStore = useConsoleStore()

/**
 * Sizes of the splitters, in pixels (`main.css`): default and smallest
 * width of the object navigator and of the data explorer; smallest width
 * and height of the area of the editors; default and smallest height of
 * the console; a folded panel keeps its rail.
 *
 * Fixed sizes: Reka UI's splitter only takes pixels or a percentage of the
 * group, so these theme lengths are read once, in pixels (`docs/DEVELOPER.md`, section "Lengths in pixels").
 */
const NAVIGATOR_WIDTH = cssPixels('--navigator-width', 300)
const NAVIGATOR_MIN_WIDTH = cssPixels('--navigator-min-width', 200)
const EXPLORER_WIDTH = cssPixels('--explorer-width', 420)
const EXPLORER_MIN_WIDTH = cssPixels('--explorer-min-width', 240)
const EDITORS_MIN_WIDTH = cssPixels('--editors-min-width', 640)
const EDITORS_MIN_HEIGHT = cssPixels('--editors-min-height', 312)
const CONSOLE_HEIGHT = cssPixels('--console-height', 220)
const CONSOLE_MIN_HEIGHT = cssPixels('--console-min-height', 120)
const RAIL_SIZE = cssPixels('--rail-size', 28)

/**
 * Foldable panels of the splitters: object navigator, data explorer,
 * console.
 */
const navigatorSplit = useTemplateRef<InstanceType<typeof SplitterPanel>>('navigatorSplit')
const explorerSplit = useTemplateRef<InstanceType<typeof SplitterPanel>>('explorerSplit')
const consoleSplit = useTemplateRef<InstanceType<typeof SplitterPanel>>('consoleSplit')

/**
 * Folds or unfolds a panel of a splitter as its state says, unless it is
 * so already.
 *
 * Workaround: `collapse` on a panel already folded (a layout saved folded,
 * on loading) replaces the width it unfolds to by its folded width, and
 * the panel would unfold at its smallest width (`docs/DEVELOPER.md`,
 * section "Workarounds").
 *
 * @param panel Panel of the splitter.
 * @param open Is the panel open?
 */
function showPanel(panel: InstanceType<typeof SplitterPanel> | null, open: boolean): void {
  if (panel === null || panel.isCollapsed !== open) {
    return
  }
  if (open) {
    panel.expand()
  } else {
    panel.collapse()
  }
}

/**
 * Do the foldable panels follow their states yet? Once mounted, each panel
 * takes the state of its store; until then, the resizes of the default or
 * saved layout leave the stores as they are.
 */
const laidOut = ref(false)

/**
 * After a resize of a foldable panel, gives its state to its store: open
 * unless folded, from Reka UI's `isCollapsed`.
 *
 * Workaround: the `collapse` event of a panel in pixels misses a fold by a
 * drag (`docs/DEVELOPER.md`, section "Workarounds").
 *
 * @param panel Panel of the splitter.
 * @param setOpen Sets the state in the store.
 */
function follow(
  panel: InstanceType<typeof SplitterPanel> | null,
  setOpen: (open: boolean) => void,
): void {
  if (laidOut.value && panel !== null) {
    setOpen(!panel.isCollapsed)
  }
}

// The state of each foldable panel (header button, rail, double click)
// drives the panel; the panel, folded or unfolded by a drag, updates the
// state in return (`follow`).
watch(
  () => layout.objectsOpen,
  (open) => showPanel(navigatorSplit.value, open),
)
watch(
  () => explorer.open,
  (open) => showPanel(explorerSplit.value, open),
)
watch(
  () => consoleStore.open,
  (open) => showPanel(consoleSplit.value, open),
)
onMounted(() =>
  nextTick(() => {
    showPanel(navigatorSplit.value, layout.objectsOpen)
    showPanel(explorerSplit.value, explorer.open)
    showPanel(consoleSplit.value, consoleStore.open)
    laidOut.value = true
  }),
)

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
 * What ends a session: "Déconnecter", closing of the editor, change of its
 * source.
 */
type Ending = 'disconnect' | 'close' | 'change'

/**
 * Title of the confirmation window and label of its button, per ending.
 */
const ENDING_TEXTS: Record<Ending, { title: string; confirm: string }> = {
  disconnect: { title: 'Déconnecter la session', confirm: 'Déconnecter et annuler' },
  close: { title: 'Fermer l’éditeur', confirm: 'Fermer et annuler' },
  change: { title: 'Changer de source', confirm: 'Changer et annuler' },
}

/**
 * Ending of a session whose transaction is open, last asked for, with the
 * answer to give (`null` once given). Kept after the confirmation window
 * closes, which keeps its texts while it fades out.
 */
const ending = ref<{ action: Ending; answer: ((confirmed: boolean) => void) | null } | null>(null)

/**
 * Is the confirmation window of `ending` open?
 */
const endingOpen = ref(false)

/**
 * May the session of an editor end? At once without a transaction open;
 * otherwise once the user confirms, which rolls the transaction back.
 *
 * @param editorId Id of the editor.
 * @param action What ends the session.
 * @returns Whether the session may end.
 */
function mayEnd(editorId: number, action: Ending): Promise<boolean> {
  if (!editors.transactions[editorId]) {
    return Promise.resolve(true)
  }
  answerEnding(false)
  return new Promise((answer) => {
    ending.value = { action, answer }
    endingOpen.value = true
  })
}

/**
 * Gives the answer of the confirmation window, and closes it.
 *
 * @param confirmed Does the user confirm the end of the session?
 */
function answerEnding(confirmed: boolean): void {
  endingOpen.value = false
  const answer = ending.value?.answer
  if (ending.value) {
    ending.value.answer = null
  }
  answer?.(confirmed)
}

// Escape or a click outside the window: no.
watch(endingOpen, (open) => {
  if (!open) {
    answerEnding(false)
  }
})

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
 * Database chosen in the navigator for the session that opens, by editor:
 * the session opens on it at once, in place of the connection's database.
 * Forgotten once that session is open (or has failed).
 */
const openingDatabases = ref<Record<number, string>>({})

/**
 * Mounted cells, by editor, to start a query there from the navigator, and
 * to fold the active editor from the editors' bar.
 */
const cells = shallowReactive(new Map<number, InstanceType<typeof QueryCell>>())

/**
 * Cell of the active editor, absent without a connection.
 */
const activeCell = computed(() =>
  editors.activeId === null ? undefined : cells.get(editors.activeId),
)

/**
 * Label of the fold button of the active editor.
 */
const editorFoldLabel = computed(() =>
  activeCell.value?.editorFolded ? 'Déplier l’éditeur' : 'Replier l’éditeur',
)

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

// A created (or deleted) connection becomes (or stops being) that of the
// active editor; a created one after a confirmation if its transaction is
// open, else the editor keeps its source. A deleted one has closed the
// session already.
watch(
  () => connections.selectedId,
  async (connectionId) => {
    const editor = editors.active
    if (!editor || editor.connectionId === connectionId) {
      return
    }
    if (connectionId !== null && !(await mayEnd(editor.id, 'change'))) {
      connections.selectedId = editor.connectionId
      return
    }
    editors.setConnection(editor.id, connectionId).catch(report)
  },
)

/**
 * Gives a connection to an editor. Its closed session reopens when the
 * connection stays the same: choosing its source again in the navigator
 * reconnects it, like "Reconnecter".
 *
 * Another connection closes the session: after a confirmation if its
 * transaction is open.
 *
 * @param editorId Id of the editor.
 * @param connectionId Id of the connection.
 * @param database
 *     Database to open the session on, when a session opens; absent: that of
 *     the connection.
 * @returns
 *     `opens` if a session opens (other connection, or closed session),
 *     `keeps` if the open session stays, `cancelled` if the user keeps the
 *     transaction and the connection.
 */
async function connectEditor(
  editorId: number,
  connectionId: number,
  database?: string,
): Promise<'opens' | 'keeps' | 'cancelled'> {
  const editor = editors.editors.find((item) => item.id === editorId)
  if (!editor) {
    return 'keeps'
  }
  if (editor.connectionId !== connectionId && !(await mayEnd(editorId, 'change'))) {
    return 'cancelled'
  }
  const reopens = editor.connectionId === connectionId && statuses.value[editorId] === 'closed'
  const opens = reopens || editor.connectionId !== connectionId
  if (opens && database !== undefined) {
    openingDatabases.value[editorId] = database
  }
  if (reopens) {
    attempts.value[editorId] = (attempts.value[editorId] ?? 0) + 1
  }
  await editors.setConnection(editorId, connectionId)
  return opens ? 'opens' : 'keeps'
}

/**
 * Follows the stage of an editor's session: the database chosen for its
 * opening is forgotten once it is open, so that "Reconnecter" opens on the
 * connection's database again.
 *
 * @param editorId Id of the editor.
 * @param status Stage of its session.
 */
function followStatus(editorId: number, status: SessionStatus): void {
  statuses.value[editorId] = status
  if (status !== 'connecting') {
    delete openingDatabases.value[editorId]
  }
}

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
  if ((await connectEditor(editor.id, connectionId)) === 'cancelled') {
    return undefined
  }
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
 * this connection to the editor. A session that opens for it (other source,
 * or closed session) opens on this database at once: the navigator never
 * shows the connection's database in between.
 *
 * @param connectionId Id of the database's connection.
 * @param schema Name of the database.
 */
async function useDatabase(connectionId: number, schema: string): Promise<void> {
  const editor = editors.active
  if (!editor) {
    return
  }
  try {
    if ((await connectEditor(editor.id, connectionId, schema)) === 'keeps') {
      cells.get(editor.id)?.changeDatabaseWhenReady(schema)
    }
  } catch (failure) {
    report(failure)
  }
}

/**
 * Closes the session of the active editor ("Déconnecter" of the navigator),
 * after a confirmation if its transaction is open.
 */
async function disconnect(): Promise<void> {
  const editorId = editors.activeId
  if (editorId !== null && (await mayEnd(editorId, 'disconnect'))) {
    await cells.get(editorId)?.disconnect()
  }
}

/**
 * Closes an editor, after a confirmation if its transaction is open.
 *
 * @param editorId Id of the editor.
 */
async function closeEditor(editorId: number): Promise<void> {
  if (await mayEnd(editorId, 'close')) {
    await editors.close(editorId).catch(report)
  }
}

/**
 * Chooses a source as the connection of the active editor.
 *
 * @param connectionId Id of the connection.
 */
function selectSource(connectionId: number): void {
  if (editors.active) {
    connectEditor(editors.active.id, connectionId).catch(report)
  }
}
</script>

<template>
  <div class="flex h-full flex-col">
    <app-top-bar />
    <!--
      Each panel of the splitters has an id of its own: Reka UI keys the
      size a folded panel unfolds to by its id, and the id it generates
      otherwise follows the order of the components, which changes with
      what is shown (a folded console renders less).
    -->
    <splitter-group direction="horizontal" auto-save-id="prisme.layout" class="min-h-0 flex-1">
      <splitter-panel
        id="navigator-panel"
        ref="navigatorSplit"
        :order="1"
        size-unit="px"
        :default-size="NAVIGATOR_WIDTH"
        :min-size="NAVIGATOR_MIN_WIDTH"
        collapsible
        :collapsed-size="RAIL_SIZE"
        class="flex"
        @resize="follow(navigatorSplit, (open) => (layout.objectsOpen = open))"
      >
        <!-- Folded, the object navigator stays mounted: its tree keeps its state. -->
        <aside v-show="layout.objectsOpen" class="flex min-w-0 flex-1 flex-col gap-4 p-4">
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
        <panel-rail
          v-if="!layout.objectsOpen"
          title="Navigateur d’objets"
          side="start"
          class="flex-1"
          @expand="layout.objectsOpen = true"
        />
      </splitter-panel>
      <split-handle
        tooltip="Glisser pour redimensionner le navigateur d’objets, double clic pour le replier ou le déplier"
        @toggle="layout.objectsOpen = !layout.objectsOpen"
      />
      <splitter-panel
        id="center-panel"
        :order="2"
        size-unit="px"
        :min-size="EDITORS_MIN_WIDTH"
        class="flex"
      >
        <splitter-group
          direction="vertical"
          auto-save-id="prisme.centerLayout"
          class="min-w-0 flex-1"
        >
          <splitter-panel
            id="editors-panel"
            :order="1"
            size-unit="px"
            :min-size="EDITORS_MIN_HEIGHT"
            class="flex"
          >
            <!--
              The padding of the sides is inside the panels of the cells
              (`QueryCell`), which would cut a border set against their edge;
              the other elements take it here.
            -->
            <main class="flex min-h-0 min-w-0 flex-1 flex-col gap-3 py-4">
              <div v-if="editorsError" class="flex px-4">
                <ui-alert
                  tone="error"
                  :title="editorsError"
                  class="flex-1"
                  @close="editorsError = null"
                />
              </div>
              <tabs-root
                v-model="activeTab"
                :unmount-on-hide="false"
                class="flex min-h-0 flex-1 flex-col"
              >
                <div class="flex px-4">
                  <editor-tab-bar class="flex-1" @close="closeEditor">
                    <!-- Fold button of the active editor, visible when it is folded. -->
                    <template #actions>
                      <ui-button
                        v-if="activeCell"
                        :icon="activeCell.editorFolded ? PanelTopOpen : PanelTopClose"
                        variant="ghost"
                        small
                        :aria-label="editorFoldLabel"
                        v-tooltip="editorFoldLabel"
                        @click="activeCell.toggleEditor()"
                      />
                    </template>
                  </editor-tab-bar>
                </div>
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
                    :opening-database="openingDatabases[editor.id]"
                    :sql="editor.sql"
                    @update:sql="(sql) => editors.setSql(editor.id, sql)"
                    @reconnect="attempts[editor.id] = (attempts[editor.id] ?? 0) + 1"
                    @database="(database) => (databases[editor.id] = database)"
                    @status="(status) => followStatus(editor.id, status)"
                    @transaction="(open) => (editors.transactions[editor.id] = open)"
                  />
                  <div v-else class="flex max-w-xs flex-col items-center gap-3 text-center">
                    <span class="flex rounded-full bg-surface-raised p-4">
                      <Database class="size-5 text-fg-secondary" />
                    </span>
                    <p class="text-balance text-fg-secondary">
                      Faites Ctrl+clic (Cmd+clic sous macOS) sur une source de données pour la
                      choisir pour cet éditeur, ou ajoutez une connexion par le menu « ⋮ »
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
            id="console-panel"
            ref="consoleSplit"
            :order="2"
            size-unit="px"
            :default-size="CONSOLE_HEIGHT"
            :min-size="CONSOLE_MIN_HEIGHT"
            collapsible
            :collapsed-size="RAIL_SIZE"
            class="flex"
            @resize="follow(consoleSplit, (open) => (consoleStore.open = open))"
          >
            <section
              v-if="consoleStore.open"
              class="flex min-h-0 min-w-0 flex-1 flex-col px-4 pt-1 pb-4"
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
      </splitter-panel>
      <split-handle
        tooltip="Glisser pour redimensionner l’explorateur de données, double clic pour le replier ou le déplier"
        @toggle="explorer.open = !explorer.open"
      />
      <splitter-panel
        id="explorer-panel"
        ref="explorerSplit"
        :order="3"
        size-unit="px"
        :default-size="EXPLORER_WIDTH"
        :min-size="EXPLORER_MIN_WIDTH"
        collapsible
        :collapsed-size="RAIL_SIZE"
        class="flex"
        @resize="follow(explorerSplit, (open) => (explorer.open = open))"
      >
        <aside v-if="explorer.open" class="flex min-w-0 flex-1 flex-col p-4">
          <data-explorer />
        </aside>
        <panel-rail
          v-else
          title="Explorateur de données"
          side="end"
          class="flex-1"
          @expand="explorer.open = true"
        />
      </splitter-panel>
    </splitter-group>
  </div>
  <ui-toaster />
  <ui-dialog
    v-model:open="endingOpen"
    :title="ENDING_TEXTS[ending?.action ?? 'disconnect'].title"
    description="Une transaction est ouverte sur la session."
  >
    <p class="text-fg-secondary">
      La session se ferme, ce qui annule la transaction (ROLLBACK) : ses modifications non validées
      sont perdues. Pour les garder, exécuter d’abord COMMIT.
    </p>
    <template #footer>
      <div class="flex w-full justify-end gap-3">
        <ui-button label="Annuler" @click="answerEnding(false)" />
        <ui-button
          :label="ENDING_TEXTS[ending?.action ?? 'disconnect'].confirm"
          variant="solid"
          tone="error"
          @click="answerEnding(true)"
        />
      </div>
    </template>
  </ui-dialog>
  <ui-tooltip />
</template>
