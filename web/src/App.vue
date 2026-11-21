<!--
  Layout of the screen: on top, the application bar (`TopBar`), across the
  whole width; below, the frame of the editors: their tabs (`EditorTabBar`),
  across the whole width, then three columns: on the left, the object
  navigator; in the middle, the view of the active editor (`EditorView`):
  its connection, its session and its result tabs; on the right, the data
  explorer (`ExplorerPanel`). Under the three columns, as wide as them, the
  console of the editor (`ConsolePanel`). The columns share the width, and
  the columns and the console the height, in splitters of Prisme
  (`split/`): dragging the line between two panels moves it, to the pixel,
  within their bounds (the editors keep `--editors-min-width` and
  `--editors-min-height`, where the line stops), and the sizes are kept in
  the browser (`navigator`, `editorLayouts` and `explorer` stores). On the
  left of each editor's tab, down to its bottom, its activity bar (`PanelActivityBar`) opens and closes
  the panels: a closed panel takes no room, and its line goes with it; a
  drag of a line below the smallest size of its panel closes it too. The
  object navigator and the console are open by default (`navigator` and
  `editorLayouts` stores), the data explorer closed. Layout in Tailwind
  classes.

  Each editor has its own object navigator, data explorer and console, in
  its tab.
  The navigator acts on its editor: the chosen source becomes its
  connection, an opened table or a chosen database (`USE`) runs there, and
  "Déconnecter" closes its session, which any of these actions on its source
  opens again. The explorer shows the values selected in the grid of its
  editor. Both keep their layout (open or closed, their width) for their
  editor, and the navigator its tree (`navigator` and `explorer` stores).
  The tabs of all the editors stay mounted, navigator, view and explorer:
  each view keeps its session (and its transactions) open, even when
  another editor is displayed. The console of the active editor alone is
  mounted: the lists of the console are those of the editor on screen
  (`console` store). Closing a session rolls its open transaction
  back: while one is open, "Déconnecter", closing the editor and changing
  its source ask for a confirmation first (changing or deleting the
  connection is confirmed in its own window).

  Transient messages (`notify`) show at the bottom right (`UiToaster`); the
  tooltips of every element (`v-tooltip`), in a single bubble (`UiTooltip`).
-->
<script setup lang="ts">
import { errorMessage } from '@/api/errorMessage'
import { Database } from '@lucide/vue'
import { TabsContent, TabsRoot } from 'reka-ui'
import { computed, nextTick, onMounted, onUnmounted, ref, shallowReactive, watch } from 'vue'

import type { SessionStatus } from './api/querySession'

import { useConnectionsStore } from './stores/connections'
import { useConsoleStore } from './stores/console'
import { useEditorLayoutsStore } from './stores/editorLayouts'
import { useExplorerStore } from './stores/explorer'
import { type EditorEntry, useEditorsStore } from './stores/editors'
import { useNavigatorStore } from './stores/navigator'
import { showEditor } from './components/ui/toasts'

import TopBar from '@/components/top-bar/TopBar.vue'
import ConsolePanel from '@/components/console/ConsolePanel.vue'
import ExplorerPanel from '@/components/explorer/ExplorerPanel.vue'
import EditorTabBar from '@/components/editor/EditorTabBar.vue'
import NavigatorPanel from '@/components/navigator/NavigatorPanel.vue'
import EditorView from '@/components/editor/EditorView.vue'
import PanelActivityBar from '@/components/panel/PanelActivityBar.vue'
import SplitGroup from './components/split/SplitGroup.vue'
import SplitHandle from './components/split/SplitHandle.vue'
import SplitPanel from './components/split/SplitPanel.vue'
import UiAlert from './components/ui/UiAlert.vue'
import UiButton from './components/ui/UiButton.vue'
import UiDialog from './components/ui/UiDialog.vue'
import UiToaster from './components/ui/UiToaster.vue'
import UiTooltip from './components/ui/UiTooltip.vue'

const connections = useConnectionsStore()
const editors = useEditorsStore()
const explorer = useExplorerStore()
const navigatorStore = useNavigatorStore()
const layouts = useEditorLayoutsStore()
const consoleStore = useConsoleStore()

// The lists of the console are those of the active editor, followed while
// its console is open, the only one mounted.
watch(
  () => {
    const id = editors.activeId
    return id !== null && !layouts.layoutOf(id).consoleFolded ? id : null
  },
  (id) => consoleStore.follow(id),
  { immediate: true },
)
onUnmounted(() => consoleStore.follow(null))

// The notifications go to the debug log of the editor on screen.
watch(() => editors.activeId, showEditor, { immediate: true })

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
 * Stage of an editor's session, `null` without a session (an editor without
 * a connection).
 *
 * @param editor Editor.
 */
function statusOf(editor: EditorEntry): SessionStatus | null {
  if (!connectionOf(editor.connectionId)) {
    return null
  }
  return statuses.value[editor.id] ?? null
}

/**
 * Current database of an editor's session, `null` if none or if the session
 * is closed.
 *
 * @param editor Editor.
 */
function databaseOf(editor: EditorEntry): string | null {
  if (statusOf(editor) === 'closed') {
    return null
  }
  return databases.value[editor.id] ?? null
}

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
 * Mounted views of the editors, by editor, to start a query there from its
 * navigator.
 */
const views = shallowReactive(new Map<number, InstanceType<typeof EditorView>>())

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
 * Gives a connection to an editor, then waits for its view (reopened on this
 * connection) to be mounted.
 *
 * @param editorId Id of the editor.
 * @param connectionId Id of the connection.
 * @returns The view of the editor, absent if there is none.
 */
async function cellOn(editorId: number, connectionId: number) {
  if ((await connectEditor(editorId, connectionId)) === 'cancelled') {
    return undefined
  }
  await nextTick()
  return views.get(editorId)
}

/**
 * Shows the content of a table chosen in the navigator of an editor, in that
 * editor. A table of another source first gives this connection to the
 * editor: the query starts as soon as its session is open.
 *
 * @param editorId Id of the editor.
 * @param connectionId Id of the table's connection.
 * @param schema Name of the database.
 * @param table Name of the table.
 */
async function openTable(
  editorId: number,
  connectionId: number,
  schema: string,
  table: string,
): Promise<void> {
  try {
    ;(await cellOn(editorId, connectionId))?.showTableWhenReady(schema, table)
  } catch (failure) {
    report(failure)
  }
}

/**
 * Writes the operations on an index chosen in the navigator of an editor, in
 * that editor, as `openTable` does for a table.
 *
 * @param editorId Id of the editor.
 * @param connectionId Id of the index's connection.
 * @param schema Name of the database.
 * @param table Name of the table.
 * @param index Name of the index.
 */
async function writeIndexOperations(
  editorId: number,
  connectionId: number,
  schema: string,
  table: string,
  index: string,
): Promise<void> {
  try {
    ;(await cellOn(editorId, connectionId))?.writeIndexOperationsWhenReady(schema, table, index)
  } catch (failure) {
    report(failure)
  }
}

/**
 * Makes a database chosen in the navigator of an editor the current database
 * of its session (like `USE`). A database of another source first gives this
 * connection to the editor. A session that opens for it (other source, or
 * closed session) opens on this database at once: the navigator never shows
 * the connection's database in between.
 *
 * @param editorId Id of the editor.
 * @param connectionId Id of the database's connection.
 * @param schema Name of the database.
 */
async function useDatabase(editorId: number, connectionId: number, schema: string): Promise<void> {
  try {
    if ((await connectEditor(editorId, connectionId, schema)) === 'keeps') {
      views.get(editorId)?.changeDatabaseWhenReady(schema)
    }
  } catch (failure) {
    report(failure)
  }
}

/**
 * Closes the session of an editor ("Déconnecter" of its navigator), after a
 * confirmation if its transaction is open.
 *
 * @param editorId Id of the editor.
 */
async function disconnect(editorId: number): Promise<void> {
  if (await mayEnd(editorId, 'disconnect')) {
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
 * Chooses a source, in the navigator of an editor, as its connection.
 *
 * @param editorId Id of the editor.
 * @param connectionId Id of the connection.
 */
function selectSource(editorId: number, connectionId: number): void {
  connectEditor(editorId, connectionId).catch(report)
}
</script>

<template>
  <div class="flex h-full flex-col">
    <top-bar />
    <!--
      Each panel of the splitters has an id of its own: the line that
      resizes it names it (`controls`, `aria-controls`).
    -->
    <tabs-root
      v-model="activeTab"
      :unmount-on-hide="false"
      as="main"
      class="flex min-h-0 min-w-0 flex-1 flex-col"
    >
      <div v-if="editorsError" class="flex flex-col px-4 pt-3">
        <ui-alert tone="error" :title="editorsError" @close="editorsError = null" />
      </div>
      <editor-tab-bar @close="closeEditor" />
      <tabs-content
        v-for="editor in editors.editors"
        :key="editor.id"
        :value="editor.id"
        class="flex min-h-0 min-w-0 flex-1"
      >
        <panel-activity-bar :editor-id="editor.id" :active="editor.id === editors.activeId" />
        <split-group direction="vertical" class="min-w-0 flex-1">
          <split-panel
            :id="`columns-panel-${editor.id}`"
            fill
            min-size="var(--editors-min-height)"
            class="flex"
          >
            <split-group direction="horizontal" class="min-w-0 flex-1">
              <split-panel
                :id="`navigator-panel-${editor.id}`"
                :size="navigatorStore.layoutOf(editor.id).width"
                :folded="!navigatorStore.layoutOf(editor.id).open"
                default-size="var(--navigator-width)"
                min-size="var(--navigator-min-width)"
                folded-size="0px"
                class="flex"
                @update:size="
                  (size: number | null) => (navigatorStore.layoutOf(editor.id).width = size)
                "
                @update:folded="
                  (folded: boolean) => (navigatorStore.layoutOf(editor.id).open = !folded)
                "
              >
                <!-- Closed, the object navigator stays mounted: its tree keeps its state. -->
                <aside
                  v-show="navigatorStore.layoutOf(editor.id).open"
                  class="flex min-w-0 flex-1 flex-col gap-4 px-4 pt-3 pb-4"
                >
                  <navigator-panel
                    :editor-id="editor.id"
                    :active="editor.id === editors.activeId"
                    :selected-id="editor.connectionId"
                    :current-database="databaseOf(editor)"
                    :session-status="statusOf(editor)"
                    @open-table="
                      (connectionId, schema, table) =>
                        openTable(editor.id, connectionId, schema, table)
                    "
                    @index-operations="
                      (connectionId, schema, table, index) =>
                        writeIndexOperations(editor.id, connectionId, schema, table, index)
                    "
                    @select="(connectionId) => selectSource(editor.id, connectionId)"
                    @use-database="
                      (connectionId, schema) => useDatabase(editor.id, connectionId, schema)
                    "
                    @disconnect="disconnect(editor.id)"
                  />
                </aside>
              </split-panel>
              <split-handle
                v-if="navigatorStore.layoutOf(editor.id).open"
                :controls="`navigator-panel-${editor.id}`"
                :tooltip="{
                  title: 'Redimensionner le navigateur d’objets',
                  description: 'Glisser ; en deçà de sa largeur minimale, il se ferme',
                }"
              />
              <!--
                    The padding of the sides is inside the panels of the views
                    (`EditorView`), which would cut a border set against their
                    edge, as is the gap under the editors' tabs. The padding of
                    the bottom is that of the last open panel of a view.
                  -->
              <split-panel
                :id="`view-panel-${editor.id}`"
                fill
                min-size="var(--editors-min-width)"
                class="flex flex-col"
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
                  @progress="(progress) => (editors.progress[editor.id] = progress)"
                />
                <div v-else class="flex max-w-xs flex-col items-center gap-3 text-center">
                  <span class="flex rounded-full bg-surface-raised p-4">
                    <Database class="size-5 text-fg-secondary" />
                  </span>
                  <p class="text-balance text-fg-secondary">
                    Faites Ctrl+clic (Cmd+clic sous macOS) sur une source de données pour la choisir
                    pour cet éditeur, ou ajoutez une connexion par le menu « ⋮ »
                  </p>
                </div>
              </split-panel>
              <split-handle
                v-if="explorer.layoutOf(editor.id).open"
                :controls="`explorer-panel-${editor.id}`"
                :tooltip="{
                  title: 'Redimensionner l’explorateur de données',
                  description: 'Glisser ; en deçà de sa largeur minimale, il se ferme',
                }"
              />
              <split-panel
                :id="`explorer-panel-${editor.id}`"
                :size="explorer.layoutOf(editor.id).width"
                :folded="!explorer.layoutOf(editor.id).open"
                default-size="var(--explorer-width)"
                min-size="var(--explorer-min-width)"
                folded-size="0px"
                class="flex"
                @update:size="(size: number | null) => (explorer.layoutOf(editor.id).width = size)"
                @update:folded="(folded: boolean) => (explorer.layoutOf(editor.id).open = !folded)"
              >
                <aside
                  v-if="explorer.layoutOf(editor.id).open"
                  class="flex min-w-0 flex-1 flex-col px-4 pt-3 pb-4"
                >
                  <explorer-panel :editor-id="editor.id" />
                </aside>
              </split-panel>
            </split-group>
          </split-panel>
          <split-handle
            v-if="!layouts.layoutOf(editor.id).consoleFolded"
            :controls="`console-panel-${editor.id}`"
            :grip="false"
            :tooltip="{
              title: 'Redimensionner la console',
              description: 'Glisser ; en deçà de sa hauteur minimale, elle se ferme',
            }"
          />
          <split-panel
            :id="`console-panel-${editor.id}`"
            :size="layouts.layoutOf(editor.id).console"
            :folded="layouts.layoutOf(editor.id).consoleFolded"
            default-size="var(--console-height)"
            min-size="var(--console-min-height)"
            folded-size="0px"
            class="flex"
            @update:size="(size: number | null) => (layouts.layoutOf(editor.id).console = size)"
            @update:folded="
              (folded: boolean) => (layouts.layoutOf(editor.id).consoleFolded = folded)
            "
          >
            <section
              v-if="!layouts.layoutOf(editor.id).consoleFolded && editor.id === editors.activeId"
              class="flex min-h-0 min-w-0 flex-1 flex-col px-4 pt-1 pb-4"
            >
              <console-panel />
            </section>
          </split-panel>
        </split-group>
      </tabs-content>
    </tabs-root>
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
