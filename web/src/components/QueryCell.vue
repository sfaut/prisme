<!--
  SQL cell: editor, run, row limit, state and results.

  The cell is that of an editor: it opens the editor's query session, on its
  connection, and keeps it open as long as it exists. The results are saved
  by the server; the cell starts again from the saved tabs, and also saves
  their name, their pinning, their order and their closing. Ctrl+Enter or
  "Exécuter" runs
  the query under the cursor; Ctrl+Shift+Enter or "Tout exécuter" runs
  all the queries in parallel, Ctrl+Shift+Enter only those the selected
  text touches, if any. "Stopper" stops them all.
  "Enjoliver" rewrites the text as "beautiful SQL", once the connection's
  catalog is loaded (it gives the case of the user's functions).
  Each query has its result tab (`ResultTab.vue`), with its progress bar. A
  tab can be renamed, pinned (it stays when the query is run again), closed
  (its query is stopped if it is running) or moved by dragging it, like a
  column of the grid.
  The row limit chosen here becomes the user's default limit.
  The buttons are above the editor. A horizontal bar, under the editor,
  splits the height between the editor (with its buttons) and the results:
  the top third by default, a remembered setting
  (`EditorResizer`).
-->
<script setup lang="ts">
import { errorMessage } from '@/api/errorMessage'
import { LoaderCircle, Play, Square, WandSparkles } from 'lucide-vue-next'
import { TabsContent, TabsList, TabsRoot, TabsTrigger } from 'reka-ui'
import { computed, onMounted, ref, useTemplateRef, watch } from 'vue'

import { analyzeSql } from '@/api/analyze'
import { beautify } from '@/api/beautify'
import { deleteResult, listResults, reorderResults, updateResult } from '@/api/editors'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import type { SelectionRange } from '@/api/generated/SelectionRange'
import type { TextEnd } from '@/api/generated/TextEnd'
import { type QueryTab, useQuerySession } from '@/api/querySession'
import { useCatalogStore } from '@/stores/catalog'
import { useSettingsStore } from '@/stores/settings'

import QueryTabView from './QueryTabView.vue'
import EditorResizer from './EditorResizer.vue'
import ResultTab from './ResultTab.vue'
import SqlEditor from './SqlEditor.vue'
import { movedItems, useTabDrag } from './tabDrag'
import { loadEditorSplit } from './editorSplit'
import { selectAllQuery } from './explorerNodes'
import { appendQuery } from './queryAppend'
import { completionSchema } from './sqlCompletion'
import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import UiSelect from './ui/UiSelect.vue'
import { notify } from './ui/toasts'

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
 * Choices of the row limit, formatted for the list.
 */
const limitOptions = computed(() =>
  (settings.settings?.row_limit_choices ?? []).map((choice) => ({
    label: `${choice.toLocaleString('fr-FR')} lignes`,
    value: choice,
  })),
)

/**
 * Flavor and version of the server, for the header of the cell.
 */
const serverLabel = computed(() => {
  const server = state.server
  if (!server) {
    return 'Connexion…'
  }
  return `${server.flavor === 'mariadb' ? 'MariaDB' : 'MySQL'} ${server.version}`
})

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
  return `${tab.query + 1} · ${short.length > 40 ? `${short.slice(0, 40)}…` : short}`
}

/**
 * Shows a failure to save on the server side, as an error toast. Changing the
 * screen back is up to the caller: only it knows what it changed.
 *
 * @param failure Received error.
 */
function reportFailure(failure: unknown): void {
  notify(`Enregistrement refusé : ${errorMessage(failure)}`, 'error')
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
 * Runs the query under the cursor.
 *
 * @param cursor Position of the cursor; when absent, that of the editor.
 */
function run(cursor?: number): void {
  if (sql.value.trim() !== '') {
    session.run(sql.value, { cursor: cursor ?? editor.value?.cursor() ?? 0 })
  }
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
 * Shows the content of a table: its query is appended after the last query
 * of the editor, without overwriting anything (`queryAppend.ts`), the cursor
 * on the next line, ready to edit it; the query starts right away. The end of
 * the text is analysed by the server again if the text changes meanwhile.
 *
 * @param schema Name of the database.
 * @param table Name of the table.
 */
async function showTable(schema: string, table: string): Promise<void> {
  const query = selectAllQuery(schema, table, state.server?.ansi_quotes ?? false)
  try {
    let text: string
    let end: TextEnd
    do {
      text = sql.value
      end = (await analyzeSql(text, state.server?.flavor ?? 'mysql')).text_end
    } while (text !== sql.value)
    const appending = appendQuery(text, query, end)
    if (editor.value) {
      editor.value.changeText(appending.changes, appending.text.length)
    } else {
      sql.value = appending.text
    }
    session.run(appending.text, { cursor: appending.start })
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
 * the connection's catalog (loaded by the explorer).
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
 * catalog's spelling, according to the flavor and the version of the server.
 * Ctrl+Z undoes it.
 */
async function prettify(): Promise<void> {
  try {
    sql.value = await beautify(sql.value, {
      catalog: catalog.catalogNames(props.connection.id),
      flavor: state.server?.flavor,
      version: state.server?.version,
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
  if (sql.value.trim() !== '') {
    session.run(sql.value, selection.length > 0 ? { selection } : {})
  }
}

/**
 * Changes the user's default row limit.
 *
 * @param rowLimit New limit.
 */
async function changeLimit(rowLimit: number): Promise<void> {
  try {
    await settings.setRowLimit(rowLimit)
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

watch(
  () => state.database,
  (database) => emit('database', database),
)

defineExpose({ run, runWhenReady, showTableWhenReady, changeDatabaseWhenReady })
</script>

<template>
  <div ref="cellRoot" class="flex min-h-0 flex-1 flex-col gap-gap">
    <!-- Top block: the buttons, then the editor, which takes the remaining room. -->
    <div class="editor-pane" :style="{ '--editor-split': `${editorSplit * 100}%` }">
      <div class="flex items-center gap-gap">
        <ui-button
          label="Exécuter"
          :icon="Play"
          variant="solid"
          tone="accent"
          :disabled="state.status !== 'ready' && state.status !== 'running'"
          :loading="state.status === 'running'"
          @click="run()"
        />
        <ui-button
          label="Tout exécuter"
          title="Toutes les requêtes, en parallèle (Ctrl-Maj-Entrée)"
          :disabled="state.status !== 'ready'"
          @click="runAll()"
        />
        <ui-button
          label="Stopper"
          :icon="Square"
          tone="error"
          title="Arrête toutes les requêtes et tous les chargements en cours"
          :disabled="state.status !== 'running'"
          @click="session.cancel()"
        />
        <ui-button
          label="Enjoliver"
          :icon="WandSparkles"
          :disabled="sql.trim() === '' || !catalogReady"
          :title="
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
        <span class="text-trim text-fg-muted">{{ connection.name }} · {{ serverLabel }}</span>
      </div>

      <sql-editor
        ref="editor"
        v-model="sql"
        :flavor="state.server?.flavor ?? 'mysql'"
        :version="state.server?.version ?? null"
        :ansi-quotes="state.server?.ansi_quotes ?? false"
        :schema="completion"
        :default-schema="state.database"
        @run="run"
        @run-all="runAll"
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
      class="m-auto flex items-center gap-gap text-ui text-fg-muted"
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
      class="flex min-h-0 flex-1 flex-col gap-1"
    >
      <tabs-list class="flex overflow-x-auto border-b border-line">
        <tabs-trigger
          v-for="(tab, index) in state.tabs"
          :key="tab.id"
          :value="tab.id"
          as="div"
          :class="{
            'drop-before': tabDrag.dropAt.value === index,
            'drop-after':
              tabDrag.dropAt.value === state.tabs.length && index === state.tabs.length - 1,
          }"
          @pointerdown="tabDrag.onDown($event, index)"
          @pointermove="tabDrag.onMove"
          @pointerup="tabDrag.onDrop"
          @pointercancel="tabDrag.onDrop"
          class="group relative flex h-control max-w-tab min-w-0 shrink-0 cursor-pointer items-center rounded-t-control ps-control-x pe-1 text-ui font-medium text-fg-muted outline-none select-none hover:text-fg-strong focus-visible:bg-surface-muted data-[state=active]:bg-surface-elevated data-[state=active]:text-fg-strong"
        >
          <result-tab
            :tab="tab"
            :default-label="tabLabel(tab)"
            @close="closeTab(tab)"
            @toggle-pin="togglePin(tab)"
            @rename="(name) => renameTab(tab, name)"
          />
        </tabs-trigger>
      </tabs-list>
      <tabs-content
        v-for="tab in state.tabs"
        :key="tab.id"
        :value="tab.id"
        class="flex min-h-0 flex-1 outline-none"
      >
        <query-tab-view :tab="tab" :displayed="active && selectedTab === tab.id" />
      </tabs-content>
    </tabs-root>
  </div>
</template>

<style scoped>
/* Top block (editor and buttons): the remembered share of the height
   (`--editor-split`); the editor keeps at least a few lines. */
.editor-pane {
  display: flex;
  flex: 0 0 var(--editor-split);
  flex-direction: column;
  gap: var(--gap);
  min-block-size: calc(var(--editor-min-height) + var(--control-height) + var(--gap));
}

/* Insertion marker of a moved tab, as for the grid's columns. */
.drop-before {
  box-shadow: inset var(--focus-width) 0 0 var(--accent);
}

.drop-after {
  box-shadow: inset calc(-1 * var(--focus-width)) 0 0 var(--accent);
}
</style>
