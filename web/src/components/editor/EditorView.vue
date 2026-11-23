<!--
  View of an editor, under its tab: the SQL editor, run, row limit, state
  and results.

  The view opens the editor's query session, on its
  connection, and keeps it open as long as it exists. The results are saved
  by the server; the view starts again from the saved tabs, and also saves
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
  column of the grid. Its result can be refreshed (button right of the
  tabs, with the grid's filter and export buttons): the query runs
  again in the tab, after the running batch if any. Its rows can also be
  written as a CTE after the last query of the editor, without running it.
  The row limit chosen here becomes the user's default limit.
  While a transaction is open on the session's connection, a status badge
  says so at the right end of the editor's header, until
  its COMMIT or ROLLBACK. The banners of the session (closed, server too old,
  error) stand under that header.
  The session can be closed from the object navigator ("Déconnecter",
  `disconnect`), or by the server: the view then shows it closed, with
  "Reconnecter", which opens a new one in place, on the last current
  database of the editor. Running a query or refreshing a tab on the
  closed session opens it again the same way, then runs. A
  session opened on a server older than the oldest version Prisme supports
  shows a warning.
  Three panels share the height, in a splitter of Prisme (`split/`), each
  under its header, in the style of the Pivoteur's: the editor ("Éditeur",
  with its buttons), the results ("Résultat", with the bar of their tabs)
  and the Pivoteur (`PivotPanel.vue`, which analyses the result of the tab
  shown). Each one opens and closes from the activity bar of the editor's
  tab (`PanelActivityBar.vue`), or closes by a drag of the line above it
  below its smallest height. Closed, it takes no room, and its content stays
  mounted, out of reach (`inert`): the editor, the grids and their scroll,
  the configuration of the Pivoteur. The results and the Pivoteur depend on
  the editor: they start closed, and the editor takes the whole height. The
  highest open panel fills the height the others leave; an open panel takes
  the height it had when it closed, or, without one, shares the room in
  equal parts with the panel that fills. All three closed, the room shows
  the sunken background. A run opens the results, which show its tabs. The
  heights and the closed panels are part of the display of this editor,
  saved with it (`editorDisplays` store). The padding of the sides is inside the panels: a panel clips its content at
  its edge, which may fall between two pixels, and would cut the left
  border of a field or of the grid set against it.
-->
<script setup lang="ts">
import { errorMessage } from '@/api/errorMessage'
import { FastForward, LoaderCircle, Play, ScanText, Square, WandSparkles } from '@lucide/vue'
import { TabsContent, TabsList, TabsRoot } from 'reka-ui'
import {
  computed,
  onMounted,
  onScopeDispose,
  provide,
  ref,
  shallowReactive,
  useTemplateRef,
  watch,
} from 'vue'

import { analyzeSql } from '@/api/analyze'
import { beautify } from '@/api/beautify'
import { serverLabel, versionWarning } from '@/api/connections'
import {
  closeSession,
  deleteResult,
  listResults,
  readResultChunk,
  reorderResults,
  updateResult,
} from '@/api/editors'
import type { AttachedDatabase } from '@/api/generated/AttachedDatabase'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import type { SelectionRange } from '@/api/generated/SelectionRange'
import type { TextEnd } from '@/api/generated/TextEnd'
import {
  type BatchProgress,
  type QueryTab,
  type RunTarget,
  type SessionStatus,
  batchProgress,
  stoppable,
  refreshState,
  useQuerySession,
} from '@/api/querySession'
import { innerLevel } from '@/api/namespaceLevels'
import { useCatalogStore } from '@/stores/catalog'
import { useConnectionsStore } from '@/stores/connections'
import { useSettingsStore } from '@/stores/settings'

import EditorTransactionBadge from '@/components/editor/EditorTransactionBadge.vue'
import PanelHeader from '@/components/panel/PanelHeader.vue'
import PivotPanel from '@/components/pivot/PivotPanel.vue'
import ResultView from '@/components/result/ResultView.vue'
import { keepGridLayout } from '@/components/result/gridDisplay'
import { withSectionWidth } from '@/components/pivot/pivotConfig'
import { updateResultPivot } from '@/api/editors'
import { EDITOR_ID } from './editorId'
import { useEditorDisplaysStore } from '@/stores/editorDisplays'
import { type ViewPanel, fillingPanel } from './editorLayout'
import { nextShownTab } from './shownTab'
import ResultTab from '@/components/result/ResultTab.vue'
import ResultTabActions from '@/components/result/ResultTabActions.vue'
import SplitGroup from '@/components/split/SplitGroup.vue'
import SplitHandle from '@/components/split/SplitHandle.vue'
import SplitPanel from '@/components/split/SplitPanel.vue'
import { DEFAULT_INDENT_SIZE } from './editorIndent'
import SqlEditor from './SqlEditor.vue'
import { movedItems, useTabDrag } from '@/components/tab/tabDrag'
import { selectAllQuery } from '@/components/navigator/navigatorNodes'
import { cteStatement } from '@/components/result/cte'
import { indexOperations } from '@/components/navigator/indexOperations'
import { type Appending, appendBlock, appendQuery } from './queryAppend'
import UiAlert from '@/components/ui/UiAlert.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import { notify } from '@/components/ui/toasts'
import { type Tooltip, vTooltip } from '@/components/ui/tooltip'

const props = defineProps<{
  /**
   * Editor of the view.
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
  /**
   * Database to open the session on, in place of the connection's: the one
   * chosen in the navigator when its choice opens the session. Read once,
   * when the view is mounted.
   */
  openingDatabase?: string
}>()

const emit = defineEmits<{
  /**
   * The current database has changed (connection, `USE`); `null` if none.
   */
  database: [database: string | null]
  /**
   * The stage of the session has changed (opening, ready, running, closed).
   */
  status: [status: SessionStatus]
  /**
   * A transaction opened (`true`) or ended on the session.
   */
  transaction: [open: boolean]
  /**
   * The databases attached to the session were told, at its opening or
   * after a change; `null` until then, and for a server.
   */
  databases: [databases: AttachedDatabase[] | null]
  /**
   * The progress of the last batch has changed (`null` before the first one).
   */
  progress: [progress: BatchProgress | null]
}>()

/**
 * SQL text of the editor.
 */
const sql = defineModel<string>('sql', { required: true })

/**
 * Display of the editor, saved with it (`editorDisplays` store).
 */
const displays = useEditorDisplaysStore()
const display = displays.displayOf(props.editorId)

/**
 * Displayed tab: its id; at first, the one shown when the editor was left.
 */
const selectedTab = ref<string | number>(display.active_result ?? 0)

// The tab shown is saved with the display of the editor, once it has an id
// of the server.
watch(selectedTab, (id) => {
  if (typeof id === 'number' && id > 0) {
    display.active_result = id
  }
})

const session = useQuerySession(
  props.editorId,
  () => listResults(props.editorId),
  readResultChunk,
  () => (typeof selectedTab.value === 'number' ? selectedTab.value : undefined),
  props.openingDatabase,
)
const catalog = useCatalogStore()
const connections = useConnectionsStore()
const state = session.state

/**
 * Reset of the display of the editor (`editorDisplays.reset`): the grid of
 * each result tab takes its default layout back, its filter kept, and the
 * sections of its Pivoteur their default widths.
 */
function resetTabs(): void {
  for (const tab of state.tabs) {
    keepGridLayout(tab, null)
    const { config } = tab.pivot
    if (config.fields_px != null || config.zones_px != null) {
      tab.pivot = {
        ...tab.pivot,
        config: withSectionWidth(withSectionWidth(config, 'fields', null), 'zones', null),
      }
      updateResultPivot(tab.id, tab.pivot).catch(() => {})
    }
  }
}

onScopeDispose(displays.onReset(props.editorId, resetTabs))
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

// The grids under the view publish to the data explorer of this editor.
provide(EDITOR_ID, props.editorId)

/**
 * Panels of the editor's tab, part of its display (`editorDisplays` store):
 * those of the view are open or closed, with the heights the user gave the
 * results and the Pivoteur, in CSS pixels (`null`: a share of the room). The
 * editor fills the height when it is open, the highest panel open; the
 * results and the Pivoteur start closed.
 */
const panels = display.panels

/**
 * Opens or closes a panel of the view, as its splitter asks.
 *
 * @param panel Panel of the view.
 * @param folded Is it closed?
 */
function setFolded(panel: ViewPanel, folded: boolean): void {
  panels[panel].open = !folded
}

/**
 * Id of a panel, unique to the editor: the line that resizes it names it.
 *
 * @param panel Panel.
 */
function panelId(panel: ViewPanel): string {
  return `editor-${props.editorId}-${panel}-panel`
}

/**
 * Panel that fills the height the others leave.
 */
const fillPanel = computed(() => fillingPanel(panels))

/**
 * Tab shown, whose buttons (refresh, filter, export) stand right of the
 * result tabs.
 */
const shownTab = computed(() => state.tabs.find((tab) => tab.id === selectedTab.value))

/**
 * Mounted views of the tabs, by tab id: the view of the tab shown gives the
 * choices of its export menu.
 */
const tabViews = shallowReactive(new Map<number, InstanceType<typeof ResultView>>())

/**
 * Expands or collapses the form of a tab's user filter; collapsed, the filter
 * stays applied.
 *
 * @param tab Tab of the query.
 */
function toggleFilter(tab: QueryTab): void {
  tab.filterOpen = !tab.filterOpen
}

// The tab shown follows the changes of the tabs (`nextShownTab`).
watch(
  () => state.tabs,
  (tabs, previous) => {
    selectedTab.value = nextShownTab(
      tabs.map((tab) => tab.id),
      previous?.map((tab) => tab.id),
      typeof selectedTab.value === 'number' ? selectedTab.value : 0,
    )
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
const STOP_TIP: Tooltip = {
  title: 'Arrêter',
  description: 'Toutes les requêtes et tous les chargements en cours',
}

/**
 * Do queries run?
 */
const running = computed(() => state.status === 'running')

/**
 * Can a run start? On a closed session too: it opens again first.
 */
const canRun = computed(() => state.status === 'ready' || state.status === 'closed')

/**
 * Warning of a session opened on a server older than the oldest version
 * Prisme supports, `null` otherwise.
 */
const versionNotice = computed(() => {
  const warning = state.server ? versionWarning(state.server) : null
  return state.server && warning ? `${serverLabel(state.server)}\n${warning}` : null
})

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
const exactTip = computed((): Tooltip => {
  if (stopsExact.value) {
    return STOP_TIP
  }
  if (!textSelected.value) {
    return {
      title: 'Exécuter le texte sélectionné',
      description: 'Sélectionnez d’abord le texte à exécuter',
    }
  }
  return {
    title: 'Exécuter le texte sélectionné',
    description:
      'Tel quel : en parallèle si les requêtes ne font que lire, l’une après l’autre dès qu’une écrit (INSERT, UPDATE, DELETE…)',
  }
})

/**
 * Starts a run: notes the button that launched it, unfolds the results,
 * which show its tabs, and sends it.
 *
 * @param kind Button that launches the run.
 * @param text Text of the editor.
 * @param options Cursor or selection of the run.
 */
function launch(kind: typeof launcher.value, text: string, options: RunTarget): void {
  launcher.value = kind
  panels.results.open = true
  session.run(text, options)
}

/**
 * Runs the query under the cursor.
 *
 * @param cursor Position of the cursor; when absent, that of the editor.
 */
function run(cursor?: number): void {
  if (sql.value.trim() !== '' && canRun.value) {
    launch('one', sql.value, { cursor: cursor ?? editor.value?.cursor() ?? 0 })
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
      launch('one', appending.text, { cursor: appending.start })
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
 * Writes the CTE of a result (`cte.ts`) after the last query of the editor,
 * ended by the separator in force, alone on its line: nothing runs.
 *
 * @param query CTE, without its separator.
 */
async function insertCte(query: string): Promise<void> {
  try {
    await appendToEditor((text, end) => appendBlock(text, cteStatement(query, end.delimiter), end))
  } catch (failure) {
    state.error = errorMessage(failure)
  }
}

/**
 * Writes the `IN` clause of a selection of the grid at the editor's cursor,
 * in place of the selected text if any; before the editor is mounted, at
 * the end of the text.
 *
 * @param clause `IN` clause (`inClause.ts`).
 */
function insertIn(clause: string): void {
  if (editor.value) {
    editor.value.insertText(clause)
  } else {
    sql.value += clause
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
 * Databases of the connection's catalog (loaded by the navigator), offered
 * by the editor's completion.
 */
const catalogSchemas = computed(() => catalog.catalogOf(props.connection.id)?.schemas ?? [])

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
  if (sql.value.trim() !== '' && canRun.value) {
    launch('all', sql.value, selection.length > 0 ? { selection } : {})
  }
}

/**
 * Runs only the selected text: of each query the passages touch, the part
 * inside them. Nothing without a selection.
 *
 * @param selection Passages selected in the editor.
 */
function runExact(selection: SelectionRange[] = []): void {
  if (selection.length > 0 && canRun.value) {
    launch('exact', sql.value, { selection, exact: true })
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
// first editor opens. The saved result tabs are read by the query session.
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

watch(
  () => state.transaction,
  (open) => emit('transaction', open),
  { immediate: true },
)

watch(
  () => state.databases,
  (databases) => emit('databases', databases),
  { immediate: true },
)

// A database copied into a file: a message says where.
watch(
  () => state.backedUp,
  (copy) => {
    if (copy !== null) {
      notify(`Base « ${copy.schema} » enregistrée dans ${copy.location.address}`)
    }
  },
)

/**
 * Was a database just attached "Dans la connexion"? The list of the
 * connections is then read again once the session tells its databases.
 */
let savedAttachment = false

// The session attached or detached a database: the navigator shows its
// schemas again, and the list of the connections a database saved in the
// connection. The first list, told as the session opens, changes nothing.
watch(
  () => state.databases,
  (_, before) => {
    if (before !== null) {
      catalog.load(props.connection.id, true, props.editorId).catch(() => {})
    }
    if (savedAttachment) {
      savedAttachment = false
      connections.load().catch(() => {})
    }
  },
)

// A request refused: no database was saved.
watch(
  () => state.error,
  () => {
    savedAttachment = false
  },
)

watch(
  () => batchProgress(state),
  (progress, before) => {
    if (progress?.progress !== before?.progress || progress?.fraction !== before?.fraction) {
      emit('progress', progress)
    }
  },
  { immediate: true },
)

/**
 * Closes the session on the server at once ("Déconnecter" of the object
 * navigator): its connection to the target server closes, and the WebSocket
 * with it; the view then shows the session closed, with "Reconnecter".
 */
async function disconnect(): Promise<void> {
  try {
    await closeSession(props.editorId)
  } catch (failure) {
    state.error = errorMessage(failure)
  }
}

/**
 * Copies a database of the session into a new file ("Enregistrer dans un
 * fichier…" of the navigator).
 *
 * @param schema Schema of the database.
 * @param location Location of the file, as typed.
 */
function backupDatabase(schema: string, location: string): void {
  session.backupDatabase(schema, location)
}

/**
 * Attaches a database to the session ("Attacher une base…" of the
 * navigator), and with `save` to the connection too.
 *
 * @param schema Schema of the database.
 * @param location Location of the database, as typed, or `:memory:`.
 * @param save Is it also added to the connection?
 */
function attachDatabase(schema: string, location: string, save: boolean): void {
  savedAttachment = save
  session.attachDatabase(schema, location, save)
}

/**
 * Detaches a database from the session ("Détacher pour la session" of the
 * navigator); it comes back with the next session.
 *
 * @param schema Schema of the database.
 */
function detachDatabase(schema: string): void {
  session.detachDatabase(schema)
}

defineExpose({
  run,
  runWhenReady,
  showTableWhenReady,
  writeIndexOperationsWhenReady,
  changeDatabaseWhenReady,
  backupDatabase,
  attachDatabase,
  detachDatabase,
  disconnect,
})
</script>

<template>
  <split-group direction="vertical" class="min-h-0 flex-1">
    <!--
      Top panel: the header (title, buttons, state of the transaction), the
      banners of the session, then the editor, which takes the remaining
      room. Closed, the editor stays mounted, out of reach.
    -->
    <split-panel
      :id="panelId('editor')"
      :folded="!panels.editor.open"
      :fill="fillPanel === 'editor'"
      min-size="var(--editor-min-height)"
      folded-size="0px"
      class="flex flex-col"
      @update:folded="setFolded('editor', $event)"
    >
      <div
        class="flex min-h-0 min-w-0 flex-1 flex-col gap-3 px-4 pt-2 pb-3"
        :inert="!panels.editor.open"
      >
        <panel-header title="Éditeur">
          <template #inline>
            <div class="flex min-w-0 items-center gap-3">
              <ui-button
                v-tooltip="
                  stopsOne
                    ? STOP_TIP
                    : { title: 'Exécuter la requête sous le curseur', shortcut: 'Ctrl-Entrée' }
                "
                label="Exécuter"
                :icon="stopsOne ? Square : Play"
                variant="solid"
                :tone="stopsOne ? 'error' : 'primary'"
                :disabled="!stopsOne && !canRun"
                @click="stopsOne ? session.cancel() : run()"
              />
              <ui-button
                v-tooltip="exactTip"
                label="Exécuter précisément"
                :icon="stopsExact ? Square : ScanText"
                :tone="stopsExact ? 'error' : 'neutral'"
                :disabled="!stopsExact && (!canRun || !textSelected)"
                @click="stopsExact ? session.cancel() : runExact(editor?.selection())"
              />
              <ui-button
                v-tooltip="
                  stopsAll
                    ? STOP_TIP
                    : {
                        title: 'Exécuter toutes les requêtes',
                        description:
                          'Ou celles que touche le texte sélectionné ; en parallèle si elles ne font que lire, l’une après l’autre dès qu’une écrit (INSERT, UPDATE, DELETE…) ou qu’une transaction est ouverte',
                        shortcut: 'Ctrl-Maj-Entrée',
                      }
                "
                label="Exécuter tout"
                :icon="stopsAll ? Square : FastForward"
                :tone="stopsAll ? 'error' : 'neutral'"
                :disabled="!stopsAll && !canRun"
                @click="stopsAll ? session.cancel() : runAll(editor?.selection())"
              />
              <ui-button
                label="Enjoliver"
                :icon="WandSparkles"
                :disabled="sql.trim() === '' || !catalogReady"
                v-tooltip="
                  catalogReady
                    ? {
                        title: 'Enjoliver les requêtes',
                        description: 'Mots-clefs en capitales ; Ctrl-Z annule',
                      }
                    : {
                        title: 'Enjoliver les requêtes',
                        description: 'Disponible une fois le catalogue de la connexion chargé',
                      }
                "
                @click="prettify"
              />
              <ui-select
                :model-value="settings.settings?.row_limit"
                :items="limitOptions"
                label="Limite de lignes"
                class="w-43"
                @update:model-value="(limit) => limit !== undefined && changeLimit(limit)"
              />
            </div>
          </template>
          <!-- State of the session, at the height of the buttons. -->
          <template v-if="state.transaction" #actions>
            <span class="flex items-center pe-3">
              <editor-transaction-badge />
            </span>
          </template>
        </panel-header>

        <ui-alert v-if="state.status === 'closed'" tone="warning" title="Session fermée.">
          <ui-button label="Reconnecter" @click="session.reopen()" />
        </ui-alert>
        <ui-alert v-if="versionNotice" tone="warning" :title="versionNotice" />
        <ui-alert v-if="state.error" tone="error" :title="state.error" />

        <sql-editor
          ref="editor"
          v-model="sql"
          v-model:position="display.sql"
          :flavor="state.server?.flavor ?? 'mysql'"
          :version="state.server?.version ?? null"
          :ansi-quotes="state.server?.ansi_quotes ?? false"
          :schemas="catalogSchemas"
          :level="innerLevel(state.server?.capabilities)"
          :default-schema="state.database"
          :indent-size="indentSize"
          @run="run"
          @run-all="runAll"
          @selection="(selected) => (textSelected = selected)"
          class="min-h-0 flex-1"
        />
      </div>
    </split-panel>
    <!--
      Line between the editor and the results, both open: it resizes the
      results; below their smallest height, they close.
    -->
    <split-handle
      v-if="panels.editor.open && panels.results.open"
      :controls="panelId('results')"
      tooltip="Glisser pour redimensionner le résultat, en deçà de sa hauteur minimale pour le fermer"
    />

    <!--
      Middle panel: the results. Its first line is its header, the bar of
      its tabs, between its title and its buttons.
    -->
    <split-panel
      :id="panelId('results')"
      v-model:size="panels.results.size"
      :folded="!panels.results.open"
      :fill="fillPanel === 'results'"
      min-size="var(--results-min-height)"
      folded-size="0px"
      class="flex flex-col"
      @update:folded="setFolded('results', $event)"
    >
      <!--
        The result grid must have a bounded height: otherwise its scroll area
        grows with the result, all the rows are drawn and the interface
        freezes. Each level, down to the grid, therefore takes the remaining
        room without being able to exceed it (flex-1 and min-h-0). The hidden
        tabs stay mounted (`unmount-on-hide` set to false): the grid keeps its
        scroll position.
      -->
      <tabs-root
        v-model="selectedTab"
        :unmount-on-hide="false"
        class="flex min-h-0 min-w-0 flex-1 flex-col gap-3 px-4 pt-3 pb-3"
        :inert="!panels.results.open"
      >
        <div class="flex shrink-0 items-center gap-3">
          <h2 class="shrink-0 text-base font-semibold text-fg-strong">Résultat</h2>
          <div class="flex min-w-0 flex-1">
            <tabs-list
              v-if="state.tabs.length > 0"
              class="flex min-w-0 items-center gap-2 overflow-x-auto"
            >
              <result-tab
                v-for="(tab, index) in state.tabs"
                :key="tab.id"
                :tab="tab"
                :default-label="tabLabel(tab)"
                :index="index"
                :count="state.tabs.length"
                :drag="tabDrag"
                :stoppable="stoppable(state, tab)"
                @close="closeTab(tab)"
                @stop="session.stopQuery(tab)"
                @toggle-pin="togglePin(tab)"
                @rename="(name) => renameTab(tab, name)"
              />
            </tabs-list>
          </div>
          <!-- Buttons of the tab shown: refresh, filter, export. -->
          <div class="flex items-center">
            <result-tab-actions
              v-if="shownTab"
              :tab="shownTab"
              :refresh="refreshState(state, shownTab)"
              :export-groups="tabViews.get(shownTab.id)?.exportGroups"
              @refresh="refresh(shownTab)"
              @stop-refresh="session.stopRefresh(shownTab)"
              @toggle-filter="toggleFilter(shownTab)"
            />
          </div>
        </div>

        <div
          v-if="state.loading"
          role="status"
          class="flex flex-1 items-center justify-center gap-3 text-base text-fg-secondary"
        >
          <LoaderCircle class="size-5 animate-spin" />
          Chargement des résultats…
        </div>
        <p
          v-else-if="state.tabs.length === 0"
          class="flex flex-1 items-center justify-center text-center text-fg-secondary"
        >
          Pas de résultat
        </p>

        <tabs-content
          v-for="tab in state.tabs"
          :key="tab.id"
          :value="tab.id"
          class="flex min-h-0 flex-1"
        >
          <result-view
            :ref="
              (view) =>
                view
                  ? tabViews.set(tab.id, view as InstanceType<typeof ResultView>)
                  : tabViews.delete(tab.id)
            "
            v-model:filter="tab.filter"
            :tab="tab"
            :displayed="active && selectedTab === tab.id"
            :connection-id="connection.id"
            :editor-id="editorId"
            :database="state.database"
            :server="state.server"
            :indent-size="indentSize"
            @insert-cte="insertCte"
            @insert-in="insertIn"
          />
        </tabs-content>
      </tabs-root>
    </split-panel>
    <!--
      Line above the Pivoteur, open under another open panel: it resizes the
      Pivoteur; below its smallest height, it closes.
    -->
    <split-handle
      v-if="panels.pivot.open && fillPanel !== 'pivot'"
      :controls="panelId('pivot')"
      tooltip="Glisser pour redimensionner le Pivoteur, en deçà de sa hauteur minimale pour le fermer"
    />

    <!--
      Panel of the Pivoteur, closed by default. Closed, the panel stays
      mounted and keeps the configuration of each tab; it computes nothing
      and stops its Worker.
    -->
    <split-panel
      :id="panelId('pivot')"
      v-model:size="panels.pivot.size"
      :folded="!panels.pivot.open"
      :fill="fillPanel === 'pivot'"
      min-size="var(--pivot-min-height)"
      folded-size="0px"
      class="flex flex-col"
      @update:folded="setFolded('pivot', $event)"
    >
      <pivot-panel
        :tab="shownTab"
        :open="panels.pivot.open"
        :connection-id="connection.id"
        :editor-id="editorId"
      />
    </split-panel>
    <!--
      All the panels closed: the height they leave stays free, on the sunken
      background, which tells it from a panel. Always there, without any
      height while a panel fills: its growth follows the closing of the last
      panel (`SplitGroup.vue`), which the room would otherwise halve at once.
    -->
    <div
      data-split-room
      class="basis-0 bg-surface-sunken"
      :class="fillPanel === null ? 'grow' : 'grow-0'"
    />
  </split-group>
</template>
