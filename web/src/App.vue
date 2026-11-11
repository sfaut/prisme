<!--
  Layout of the screen: on top, the application bar (`TopBar`), across the
  whole width; below, three columns: on the left, the object navigator; in
  the middle, the SQL editors, one tab each (`EditorTabBar`), each with its
  view (`EditorView`): its connection, its session and its result tabs; on
  the right, the data explorer (`ExplorerPanel`). Both side panels go from
  the top bar down to the bottom of the window. Under the editors, between
  the side panels, the console (`ConsolePanel`). The columns share the
  width, and the editors and the console the height, in splitters of Prisme
  (`split/`): dragging the line between two panels moves it, to the pixel,
  within their bounds (the editors keep `--editors-min-width` and
  `--editors-min-height`, where the line stops), and the sizes are kept in
  the browser (`prisme.panelSizes`). Each side panel
  and the console fold with the button of their header (`PanelHeader`), by
  a drag below their smallest size or by a double click on their line, and
  leave a rail on their edge, which opens them again (`PanelRail`). The
  object navigator and the console are open by default (`layout` and
  `console` stores), the data explorer closed. Layout in Tailwind
  classes.

  The navigator acts on the active editor: the chosen source becomes its
  connection, an opened table or a chosen database (`USE`) runs there, and
  "Déconnecter" closes its session, which any of these actions on its source
  opens again. The views of all the editors stay mounted: each one keeps its
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
import { TabsContent, TabsRoot } from 'reka-ui'
import { computed, nextTick, onMounted, ref, shallowReactive, watch } from 'vue'

import type { SessionStatus } from './api/querySession'

import { useConnectionsStore } from './stores/connections'
import { useConsoleStore } from './stores/console'
import { useDataExplorerStore } from './stores/dataExplorer'
import { useEditorsStore } from './stores/editors'
import { useLayoutStore } from './stores/layout'

import TopBar from '@/components/top-bar/TopBar.vue'
import ConsolePanel from '@/components/console/ConsolePanel.vue'
import ExplorerPanel from '@/components/explorer/ExplorerPanel.vue'
import EditorTabBar from '@/components/editor/EditorTabBar.vue'
import NavigatorPanel from '@/components/navigator/NavigatorPanel.vue'
import EditorView from '@/components/editor/EditorView.vue'
import PanelRail from '@/components/panel/PanelRail.vue'
import SplitGroup from './components/split/SplitGroup.vue'
import SplitHandle from './components/split/SplitHandle.vue'
import SplitPanel from './components/split/SplitPanel.vue'
import { useStoredLayout } from './components/split/storedLayout'
import UiAlert from './components/ui/UiAlert.vue'
import UiButton from './components/ui/UiButton.vue'
import UiDialog from './components/ui/UiDialog.vue'
import UiToaster from './components/ui/UiToaster.vue'
import UiTooltip from './components/ui/UiTooltip.vue'
import { vTooltip } from './components/ui/tooltip'

const connections = useConnectionsStore()
const editors = useEditorsStore()
const explorer = useDataExplorerStore()
const layout = useLayoutStore()
const consoleStore = useConsoleStore()

/**
 * Sizes of the panels of the layout given by the user, in CSS pixels
 * (`split/storedLayout.ts`); `null`: the default size of the theme.
 */
const sizes = useStoredLayout<{
  navigator: number | null
  explorer: number | null
  console: number | null
}>('prisme.panelSizes', { navigator: null, explorer: null, console: null })

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
 * Number of reconnections requested, by editor: changing the key of its view
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
 * Mounted views of the editors, by editor, to start a query there from the navigator, and
 * to fold the active editor from the editors' bar.
 */
const views = shallowReactive(new Map<number, InstanceType<typeof EditorView>>())

/**
 * Cell of the active editor, absent without a connection.
 */
const activeCell = computed(() =>
  editors.activeId === null ? undefined : views.get(editors.activeId),
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
 * Gives a connection to the active editor, then waits for its view (reopened
 * on this connection) to be mounted.
 *
 * @param connectionId Id of the connection.
 * @returns The view of the active editor, absent if there is none.
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
  return views.get(editor.id)
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
      views.get(editor.id)?.changeDatabaseWhenReady(schema)
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
    await views.get(editorId)?.disconnect()
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
    <top-bar />
    <!--
      Each panel of the splitters has an id of its own: the line that
      resizes it names it (`controls`, `aria-controls`).
    -->
    <split-group direction="horizontal" class="min-h-0 flex-1">
      <split-panel
        id="navigator-panel"
        v-model:size="sizes.navigator"
        :folded="!layout.objectsOpen"
        default-size="var(--navigator-width)"
        min-size="var(--navigator-min-width)"
        folded-size="var(--rail-size)"
        class="flex"
        @update:folded="(folded: boolean) => (layout.objectsOpen = !folded)"
      >
        <!-- Folded, the object navigator stays mounted: its tree keeps its state. -->
        <aside v-show="layout.objectsOpen" class="flex min-w-0 flex-1 flex-col gap-4 p-4">
          <navigator-panel
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
      </split-panel>
      <split-handle
        controls="navigator-panel"
        tooltip="Glisser pour redimensionner le navigateur d’objets, double clic pour le replier ou le déplier"
        @activate="layout.objectsOpen = !layout.objectsOpen"
      />
      <split-panel id="center-panel" fill min-size="var(--editors-min-width)" class="flex">
        <split-group direction="vertical" class="min-w-0 flex-1">
          <split-panel id="editors-panel" fill min-size="var(--editors-min-height)" class="flex">
            <!--
              The padding of the sides is inside the panels of the views
              (`EditorView`), which would cut a border set against their edge;
              the other elements take it here. The padding of the bottom is
              that of the last panel of a view, the Pivoteur: open, it ends
              with its own; folded, its rail closes the area, as the rail of
              the console closes the window.
            -->
            <main class="flex min-h-0 min-w-0 flex-1 flex-col gap-3 pt-4">
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
                  <editor-view
                    v-if="connectionOf(editor.connectionId)"
                    :ref="
                      (view) =>
                        view
                          ? views.set(editor.id, view as InstanceType<typeof EditorView>)
                          : views.delete(editor.id)
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
          </split-panel>
          <split-handle
            controls="console-panel"
            tooltip="Glisser pour redimensionner la console, double clic pour la replier ou la déplier"
            @activate="consoleStore.open = !consoleStore.open"
          />
          <split-panel
            id="console-panel"
            v-model:size="sizes.console"
            :folded="!consoleStore.open"
            default-size="var(--console-height)"
            min-size="var(--console-min-height)"
            folded-size="var(--rail-size)"
            class="flex"
            @update:folded="(folded: boolean) => (consoleStore.open = !folded)"
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
          </split-panel>
        </split-group>
      </split-panel>
      <split-handle
        controls="explorer-panel"
        tooltip="Glisser pour redimensionner l’explorateur de données, double clic pour le replier ou le déplier"
        @activate="explorer.open = !explorer.open"
      />
      <split-panel
        id="explorer-panel"
        v-model:size="sizes.explorer"
        :folded="!explorer.open"
        default-size="var(--explorer-width)"
        min-size="var(--explorer-min-width)"
        folded-size="var(--rail-size)"
        class="flex"
        @update:folded="(folded: boolean) => (explorer.open = !folded)"
      >
        <aside v-if="explorer.open" class="flex min-w-0 flex-1 flex-col p-4">
          <explorer-panel />
        </aside>
        <panel-rail
          v-else
          title="Explorateur de données"
          side="end"
          class="flex-1"
          @expand="explorer.open = true"
        />
      </split-panel>
    </split-group>
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
