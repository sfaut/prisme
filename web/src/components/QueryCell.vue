<!--
  SQL cell: editor, run, row limit, state and results.

  The cell is that of an editor: it opens the editor's query session, on its
  connection, and keeps it open as long as it exists. The results are saved
  by the server; the cell starts again from the saved tabs, and also saves
  their name, their pinning, their order and their closing. Ctrl+Enter or
  "Exécuter" runs the query under the cursor; Ctrl+Shift+Enter or "Exécuter
  tout" runs in parallel the queries the selected text touches, all of them
  without a selection; "Exécuter précisément" runs only the selected text
  (the part of a query outside the selection does not run), and waits,
  disabled, for a selection. While queries run, the button that started them
  turns into a stop button (a click stops them all), and the others wait,
  disabled.
  "Enjoliver" rewrites the text as "beautiful SQL", once the connection's
  catalog is loaded (it gives the case of the user's functions).
  Each query has its result tab (`ResultTab.vue`), with its progress bar. A
  tab can be renamed, pinned (it stays when the query is run again), closed
  (its query is stopped if it is running) or moved by dragging it, like a
  column of the grid. Its result can be refreshed (button under the grid):
  the query runs again in the tab, after the running batch if any.
  The row limit chosen here becomes the user's default limit.
  The session can be closed from the object navigator ("Déconnecter",
  `disconnect`): the cell then shows it closed, with "Reconnecter".
  The buttons are above the editor. A horizontal bar, under the editor,
  splits the height between the editor (with its buttons) and the results:
  the top third by default, a remembered setting
  (`EditorResizer`).
-->
<script setup lang="ts">
import { errorMessage } from '@/api/errorMessage'
import { FastForward, LoaderCircle, Play, ScanText, Square, WandSparkles } from '@lucide/vue'
import { TabsContent, TabsList, TabsRoot } from 'reka-ui'
import { computed, onMounted, ref, useTemplateRef, watch } from 'vue'

import { analyzeSql } from '@/api/analyze'
import { beautify } from '@/api/beautify'
import {
  closeSession,
  deleteResult,
  listResults,
  reorderResults,
  updateResult,
} from '@/api/editors'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import type { SelectionRange } from '@/api/generated/SelectionRange'
import type { TextEnd } from '@/api/generated/TextEnd'
import {
  type QueryTab,
  type SessionStatus,
  refreshState,
  useQuerySession,
} from '@/api/querySession'
import { useCatalogStore } from '@/stores/catalog'
import { useSettingsStore } from '@/stores/settings'

import QueryTabView from './QueryTabView.vue'
import EditorResizer from './EditorResizer.vue'
import ResultTab from './ResultTab.vue'
import { DEFAULT_INDENT_SIZE } from './editorIndent'
import SqlEditor from './SqlEditor.vue'
import { movedItems, useTabDrag } from './tabDrag'
import { loadEditorSplit } from './editorSplit'
import { selectAllQuery } from './navigatorNodes'
import { indexOperations } from './indexOperations'
import { type Appending, appendBlock, appendQuery } from './queryAppend'
import { completionSchema } from './sqlCompletion'
import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import UiSelect from './ui/UiSelect.vue'
import { notify } from './ui/toasts'
import { vTooltip } from './ui/tooltip'

const props = defineProps<{
  /**
   * Editor of the cell.
   */
  editorId: number
  /**
   * Is the editor the active one, on screen? The grid of its active tab
   * then feeds the data explorer.
   */
  active: boolean
  /**
   * Connection on which the queries run.
   */
  connection: ConnectionSummary
}>()

const emit = defineEmits<{
  /**
   * The session is closed and the user asks to reopen it.
   */
  reconnect: []
  /**
   * The current database has changed (connection, `USE`); `null` if none.
   */
  database: [database: string | null]
  /**
   * The stage of the session has changed (opening, ready, running, closed).
   */
  status: [status: SessionStatus]
}>()

/**
 * SQL text of the cell.
 */
const sql = defineModel<string>('sql', { required: true })

const session = useQuerySession(props.editorId, () => listResults(props.editorId))
const catalog = useCatalogStore()
const state = session.state
const settings = useSettingsStore()

/**
 * Width of an indentation level, in spaces: that of the settings, for the
 * editor and the beautifier.
 */
const indentSize = computed(() => settings.settings?.indent_size ?? DEFAULT_INDENT_SIZE)

/**
 * Choices of the row limit, formatted for the list.
 */
const limitOptions = computed(() =>
  (settings.settings?.row_limit_choices ?? []).map((choice) => ({
    label: `${choice.toLocaleString('fr-FR')} lignes`,
    value: choice,
  })),
)

const editor = useTemplateRef<InstanceType<typeof SqlEditor>>('editor')

const cellRoot = useTemplateRef<HTMLDivElement>('cellRoot')

/**
 * Share of the cell's height given to the editor, from 0 to 1.
 */
const editorSplit = ref(loadEditorSplit())

/**
 * Displayed tab: its id.
 */
const selectedTab = ref<string | number>(0)

// The tabs of a new batch arrive: the first one is displayed. Otherwise,
// the displayed tab has disappeared: the first remaining one takes its place.
watch(
  () => state.tabs,
  (tabs, previous) => {
    const fresh = tabs.find((tab) => !previous?.some((old) => old.id === tab.id))
    if (fresh) {
      selectedTab.value = fresh.id
    } else if (!tabs.some((tab) => tab.id === selectedTab.value)) {
      selectedTab.value = tabs[0]?.id ?? 0
    }
  },
)

/**
 * Default label of a tab: number and start of the query.
 *
 * @param tab Tab of the query.
 */
function tabLabel(tab: QueryTab): string {
  const short = tab.sql.replace(/\s+/g, ' ')
  return `${tab.number + 1} · ${short.length > 40 ? `${short.slice(0, 40)}…` : short}`
}

/**
 * Shows a failure to save on the server side, as an error toast. Changing the
 * screen back is up to the caller: only it knows what it changed.
 *
 * @param failure Received error.
 */
function reportFailure(failure: unknown): void {
  notify(
    `Enregistrement refusé.

${errorMessage(failure)}`,
    'error',
  )
}

/**
 * Moving a result tab by dragging; the new order is saved.
 */
const tabDrag = useTabDrag((from, to) => {
  state.tabs = movedItems(state.tabs, from, to)
  reorderResults(
    props.editorId,
    state.tabs.map((tab) => tab.id),
  ).catch(reportFailure)
})

/**
 * Pins or unpins a result tab, and saves it. If the save fails, the tab goes
 * back to its previous state.
 *
 * @param tab Targeted tab.
 */
function togglePin(tab: QueryTab): void {
  const previous = tab.pinned
  tab.pinned = !previous
  updateResult(tab.id, { name: tab.name ?? '', pinned: tab.pinned }).catch((failure: unknown) => {
    tab.pinned = previous
    reportFailure(failure)
  })
}

/**
 * Renames a result tab, and saves it. If the save fails, the tab goes back to
 * its previous name.
 *
 * @param tab Targeted tab.
 * @param name New name, `null` for the default label.
 */
function renameTab(tab: QueryTab, name: string | null): void {
  const previous = tab.name
  tab.name = name
  updateResult(tab.id, { name: name ?? '', pinned: tab.pinned }).catch((failure: unknown) => {
    tab.name = previous
    reportFailure(failure)
  })
}

/**
 * Closes a tab; if it was displayed, its right (or left) neighbour takes its
 * place. Its query is stopped if it is running.
 *
 * @param tab Tab to close.
 */
function closeTab(tab: QueryTab): void {
  if (selectedTab.value === tab.id) {
    const index = state.tabs.indexOf(tab)
    const neighbour = state.tabs[index + 1] ?? state.tabs[index - 1]
    selectedTab.value = neighbour?.id ?? 0
  }
  session.closeTab(tab.id)
  deleteResult(tab.id).catch(reportFailure)
}

/**
 * Tooltip of the run buttons while queries run: a click stops them.
 */
const STOP_TIP = 'Arrête toutes les requêtes et tous les chargements en cours'

/**
 * Do queries run?
 */
const running = computed(() => state.status === 'running')

/**
 * Run button that started the running queries: "Exécuter" (`one`),
 * "Exécuter précisément" (`exact`) or "Exécuter tout" (`all`). It changes
 * only when a run really starts (a request while queries run is ignored). A
 * run started elsewhere (a table opened from the navigator, a refreshed tab, a
 * batch found again after a reload) counts as "Exécuter".
 */
const launcher = ref<'one' | 'exact' | 'all'>('one')

/**
 * Is "Exécuter" a stop button, the queries it started running?
 */
const stopsOne = computed(() => running.value && launcher.value === 'one')

/**
 * Is "Exécuter précisément" a stop button, the queries it started running?
 */
const stopsExact = computed(() => running.value && launcher.value === 'exact')

/**
 * Is "Exécuter tout" a stop button, the queries it started running?
 */
const stopsAll = computed(() => running.value && launcher.value === 'all')

/**
 * Is text selected in the editor? "Exécuter précisément" waits for it.
 */
const textSelected = ref(false)

/**
 * Tooltip of "Exécuter précisément".
 */
const exactTip = computed(() => {
  if (stopsExact.value) {
    return STOP_TIP
  }
  if (!textSelected.value) {
    return 'Sélectionnez le texte à exécuter'
  }
  return 'Le texte sélectionné seulement, tel quel : en parallèle si les requêtes ne font que lire, l’une après l’autre dès qu’une écrit (INSERT, UPDATE, DELETE…)'
})

/**
 * Runs the query under the cursor.
 *
 * @param cursor Position of the cursor; when absent, that of the editor.
 */
function run(cursor?: number): void {
  if (sql.value.trim() !== '' && state.status === 'ready') {
    launcher.value = 'one'
    session.run(sql.value, { cursor: cursor ?? editor.value?.cursor() ?? 0 })
  }
}

/**
 * Refreshes a tab. While a batch runs, the refresh waits for its end, and
 * the batch keeps its stop button.
 *
 * @param tab Tab to refresh.
 */
function refresh(tab: QueryTab): void {
  if (!running.value) {
    launcher.value = 'one'
  }
  session.refresh(tab)
}

/**
 * Starts an action as soon as the session is ready: right away if it is,
 * otherwise at the end of its opening. Nothing if it closes before.
 *
 * @param action Action to start.
 */
function whenReady(action: () => void): void {
  if (state.status === 'ready') {
    action()
    return
  }
  const stop = watch(
    () => state.status,
    (status) => {
      if (status === 'ready') {
        action()
      }
      if (status !== 'connecting') {
        stop()
      }
    },
  )
}

/**
 * Runs the query under the cursor as soon as the session is ready.
 *
 * @param cursor Position of the cursor; when absent, that of the editor.
 */
function runWhenReady(cursor?: number): void {
  whenReady(() => run(cursor))
}

/**
 * Shows the content of a table as soon as the session is ready. The names
 * are quoted according to the session's `ANSI_QUOTES` mode, known only once
 * it is ready.
 *
 * @param schema Name of the database.
 * @param table Name of the table.
 */
function showTableWhenReady(schema: string, table: string): void {
  whenReady(() => void showTable(schema, table))
}

/**
 * Appends text after the last query of the editor, without overwriting
 * anything (`queryAppend.ts`), the cursor on the next line. The end of the
 * text is analysed by the server again if the text changes meanwhile.
 *
 * @param append
 *     Change of the text, from the current text and its end as analysed by
 *     the server.
 * @returns The change made.
 * @throws {ApiError} If the server cannot analyse the text.
 */
async function appendToEditor(
  append: (text: string, end: TextEnd) => Appending,
): Promise<Appending> {
  let text: string
  let end: TextEnd
  do {
    text = sql.value
    end = (await analyzeSql(text, state.server?.flavor ?? 'mysql')).text_end
  } while (text !== sql.value)
  const appending = append(text, end)
  if (editor.value) {
    editor.value.changeText(appending.changes, appending.text.length)
  } else {
    sql.value = appending.text
  }
  return appending
}

/**
 * Shows the content of a table: its query is appended after the last query
 * of the editor, ready to edit; the query starts right away.
 *
 * @param schema Name of the database.
 * @param table Name of the table.
 */
async function showTable(schema: string, table: string): Promise<void> {
  const query = selectAllQuery(schema, table, state.server?.ansi_quotes ?? false)
  try {
    const appending = await appendToEditor((text, end) => appendQuery(text, query, end))
    if (state.status === 'ready') {
      launcher.value = 'one'
      session.run(appending.text, { cursor: appending.start })
    }
  } catch (failure) {
    state.error = errorMessage(failure)
  }
}

/**
 * Writes the operations on an index as soon as the session is ready: its
 * server gives their syntax.
 *
 * @param schema Name of the database.
 * @param table Name of the table.
 * @param index Name of the index.
 */
function writeIndexOperationsWhenReady(schema: string, table: string, index: string): void {
  whenReady(() => void writeIndexOperations(schema, table, index))
}

/**
 * Writes the operations on an index (`indexOperations.ts`) after the last
 * query of the editor, commented out: nothing runs.
 *
 * @param schema Name of the database.
 * @param table Name of the table.
 * @param index Name of the index.
 */
async function writeIndexOperations(schema: string, table: string, index: string): Promise<void> {
  const server = state.server
  if (server === null) {
    return
  }
  const target = { schema, table, name: index }
  const options = { flavor: server.flavor, version: server.version, ansiQuotes: server.ansi_quotes }
  try {
    await appendToEditor((text, end) =>
      appendBlock(text, indexOperations(target, options, end.delimiter), end),
    )
  } catch (failure) {
    state.error = errorMessage(failure)
  }
}

/**
 * Changes the session's current database (like `USE`) as soon as it is ready.
 *
 * @param database Name of the database.
 */
function changeDatabaseWhenReady(database: string): void {
  whenReady(() => session.changeDatabase(database))
}

/**
 * Databases, tables and columns offered by the editor's completion, after
 * the connection's catalog (loaded by the navigator).
 */
const completion = computed(() =>
  completionSchema(
    catalog.catalogOf(props.connection.id)?.schemas ?? [],
    state.server?.ansi_quotes ?? false,
  ),
)

/**
 * Is the connection's catalog fully loaded? Without it, the beautifier
 * knows neither the spelling of the tables and columns, nor the user's
 * functions, which it would put in capitals: "Enjoliver" therefore waits
 * for the end of the loading.
 */
const catalogReady = computed(() => catalog.catalogOf(props.connection.id)?.done === true)

/**
 * Beautifies the editor's text, after the analysis of the query by the
 * server: keywords in capitals, names (tables, columns, functions...) in the
 * catalog's spelling, according to the flavor and the version of the server,
 * indented like the editor. Ctrl+Z undoes it.
 */
async function prettify(): Promise<void> {
  try {
    sql.value = await beautify(sql.value, {
      catalog: catalog.catalogNames(props.connection.id),
      flavor: state.server?.flavor,
      version: state.server?.version,
      indent_size: indentSize.value,
    })
  } catch (failure) {
    state.error = errorMessage(failure)
  }
}

/**
 * Runs the editor's queries in parallel: those the selected passages touch,
 * or all of them.
 *
 * @param selection Passages selected in the editor; empty or absent: all the queries.
 */
function runAll(selection: SelectionRange[] = []): void {
  if (sql.value.trim() !== '' && state.status === 'ready') {
    launcher.value = 'all'
    session.run(sql.value, selection.length > 0 ? { selection } : {})
  }
}

/**
 * Runs only the selected text: of each query the passages touch, the part
 * inside them. Nothing without a selection.
 *
 * @param selection Passages selected in the editor.
 */
function runExact(selection: SelectionRange[] = []): void {
  if (selection.length > 0 && state.status === 'ready') {
    launcher.value = 'exact'
    session.run(sql.value, { selection, exact: true })
  }
}

/**
 * Changes the user's default row limit.
 *
 * @param rowLimit New limit.
 */
async function changeLimit(rowLimit: number): Promise<void> {
  try {
    await settings.update({ row_limit: rowLimit })
  } catch (failure) {
    state.error = errorMessage(failure)
  }
}

// The settings (row limit choices, default limit) are loaded once, when the
// first cell opens. The saved result tabs are read by the query session.
onMounted(async () => {
  if (settings.settings === null) {
    try {
      await settings.load()
    } catch (failure) {
      state.error = errorMessage(failure)
    }
  }
})

// Immediate: a reopened session starts without the database of the previous one.
watch(
  () => state.database,
  (database) => emit('database', database),
  { immediate: true },
)

watch(
  () => state.status,
  (status) => emit('status', status),
  { immediate: true },
)

/**
 * Closes the session on the server at once ("Déconnecter" of the object
 * navigator): its connection to the target server closes, and the WebSocket
 * with it; the cell then shows the session closed, with "Reconnecter".
 */
async function disconnect(): Promise<void> {
  try {
    await closeSession(props.editorId)
  } catch (failure) {
    state.error = errorMessage(failure)
  }
}

defineExpose({
  run,
  runWhenReady,
  showTableWhenReady,
  writeIndexOperationsWhenReady,
  changeDatabaseWhenReady,
  disconnect,
})
</script>

<template>
  <div ref="cellRoot" class="flex min-h-0 flex-1 flex-col gap-2">
    <!-- Top block: the buttons, then the editor, which takes the remaining room. -->
    <div class="editor-pane" :style="{ '--editor-split': `${editorSplit * 100}%` }">
      <div class="flex items-center gap-2">
        <ui-button
          v-tooltip="stopsOne ? STOP_TIP : 'La requête sous le curseur (Ctrl-Entrée)'"
          label="Exécuter"
          :icon="stopsOne ? Square : Play"
          variant="solid"
          :tone="stopsOne ? 'error' : 'accent'"
          :disabled="!stopsOne && state.status !== 'ready'"
          @click="stopsOne ? session.cancel() : run()"
        />
        <ui-button
          v-tooltip="exactTip"
          label="Exécuter précisément"
          :icon="stopsExact ? Square : ScanText"
          :tone="stopsExact ? 'error' : 'neutral'"
          :disabled="!stopsExact && (state.status !== 'ready' || !textSelected)"
          @click="stopsExact ? session.cancel() : runExact(editor?.selection())"
        />
        <ui-button
          v-tooltip="
            stopsAll
              ? STOP_TIP
              : 'Toutes les requêtes, ou celles que touche le texte sélectionné (Ctrl-Maj-Entrée) : en parallèle si elles ne font que lire, l’une après l’autre dès qu’une écrit (INSERT, UPDATE, DELETE…)'
          "
          label="Exécuter tout"
          :icon="stopsAll ? Square : FastForward"
          :tone="stopsAll ? 'error' : 'neutral'"
          :disabled="!stopsAll && state.status !== 'ready'"
          @click="stopsAll ? session.cancel() : runAll(editor?.selection())"
        />
        <ui-button
          label="Enjoliver"
          :icon="WandSparkles"
          :disabled="sql.trim() === '' || !catalogReady"
          v-tooltip="
            catalogReady
              ? 'Réécrit les requêtes en « beau SQL » : mots-clefs en capitales (Ctrl-Z annule)'
              : 'Disponible une fois le catalogue de la connexion chargé'
          "
          @click="prettify"
        />
        <ui-select
          :model-value="settings.settings?.row_limit"
          :items="limitOptions"
          label="Limite de lignes"
          class="w-32"
          @update:model-value="(limit) => limit !== undefined && changeLimit(limit)"
        />
      </div>

      <sql-editor
        ref="editor"
        v-model="sql"
        :flavor="state.server?.flavor ?? 'mysql'"
        :version="state.server?.version ?? null"
        :ansi-quotes="state.server?.ansi_quotes ?? false"
        :schema="completion"
        :default-schema="state.database"
        :indent-size="indentSize"
        @run="run"
        @run-all="runAll"
        @selection="(selected) => (textSelected = selected)"
        class="min-h-0 flex-1"
      />
    </div>
    <editor-resizer v-model="editorSplit" :container="cellRoot" />

    <ui-alert v-if="state.status === 'closed'" tone="warning" title="Session fermée.">
      <ui-button label="Reconnecter" @click="emit('reconnect')" />
    </ui-alert>
    <ui-alert v-if="state.error" tone="error" :title="state.error" />

    <div
      v-if="state.loading"
      role="status"
      class="flex flex-1 items-center justify-center gap-2 text-ui text-fg-muted"
    >
      <LoaderCircle class="size-icon animate-spin" />
      Chargement des résultats…
    </div>

    <!--
      The result grid must have a bounded height: otherwise its scroll area grows
      with the result, all the rows are drawn and the interface freezes. Each
      level, down to the grid, therefore takes the remaining room without being
      able to exceed it (flex-1 and min-h-0). The hidden tabs stay mounted
      (`unmount-on-hide` set to false): the grid keeps its scroll position.
    -->
    <tabs-root
      v-if="state.tabs.length > 0"
      v-model="selectedTab"
      :unmount-on-hide="false"
      class="flex min-h-0 flex-1 flex-col"
    >
      <tabs-list class="flex overflow-x-auto border-b border-line">
        <result-tab
          v-for="(tab, index) in state.tabs"
          :key="tab.id"
          :tab="tab"
          :default-label="tabLabel(tab)"
          :index="index"
          :count="state.tabs.length"
          :drag="tabDrag"
          @close="closeTab(tab)"
          @toggle-pin="togglePin(tab)"
          @rename="(name) => renameTab(tab, name)"
        />
      </tabs-list>
      <tabs-content
        v-for="tab in state.tabs"
        :key="tab.id"
        :value="tab.id"
        class="flex min-h-0 flex-1"
      >
        <query-tab-view
          :tab="tab"
          :displayed="active && selectedTab === tab.id"
          :refresh="refreshState(state, tab)"
          @refresh="refresh(tab)"
          @stop-refresh="session.stopRefresh(tab)"
        />
      </tabs-content>
    </tabs-root>
  </div>
</template>

<style scoped>
@reference "@/assets/main.css";

/* Top block (editor and buttons): the remembered share of the height
   (`--editor-split`); the editor keeps at least a few lines. */
.editor-pane {
  display: flex;
  flex: 0 0 var(--editor-split);
  flex-direction: column;
  gap: --spacing(2);
  min-block-size: calc(--spacing(16) + var(--control-height) + --spacing(2));
}
</style>
