<!--
  Content of a query's tab: error if any, form of the user filter, result
  grid, then the status bar, one item each:

  - number of rows, with a grid: those received, preceded by those the user
    filter keeps ("120 / 5 000 lignes");
  - execution time of the query by the server (until the columns arrive);
  - transfer and display time of the rows, then the received volume and the
    throughput;
  - end of the execution, date and time to the second: the instant the
    result reflects.

  During the query, the number of rows, the durations and the volume change
  once per second (shared clock, `clock.ts`); at the end, they take their
  final value at once. A query without a grid (waiting, failed, or without
  columns) shows only its durations.

  The user filter belongs to the tab (`filter`, `v-model:filter`): its form
  (`ResultFilter.vue`) shows above the grid while its button, right of the
  result tabs, keeps it expanded. New columns make it global, its text kept:
  the index of a targeted column would point to another column.

  The grid comes right under the line of the result tabs, which takes the
  place of its frame's top border; the error, the filter's form or the
  durations, when they come first, keep a little room under that line.

  During a refresh, the previous result stays displayed until the new
  columns arrive.

  The tab gives the menu of its download button (`downloadItems`), which the
  cell draws right of the result tabs, beside the refresh and filter buttons
  (`ResultTabActions.vue`). It downloads the result as CSV for Excel in
  French (`csv.ts`): the selection of the grid, or the whole displayed
  result without one, sorted and filtered as on screen, its columns in the
  displayed order. The file is built in the page, by chunks of rows. The
  same rows can become a CTE (`cte.ts`): downloaded or copied, for MySQL or
  MariaDB, or written in the editor in the dialect of its server, which the
  cell does (`insert-cte`). Columns sharing a name, or a discontinuous
  selection, refuse it. The menu also offers the full result: the server
  runs the query again, as it is written, without the limit of Prisme, on
  the current database of the editor, and writes its rows in the displayed
  order of the columns, neither sorted nor filtered by the grid; the exports
  manager of the top bar follows it (`stores/exports.ts`). Its total, when
  known in advance, gives a completed share: the number of rows of a
  displayed result that the limit did not cut.
-->
<script setup lang="ts">
import {
  ClipboardCopy,
  Database,
  DatabaseZap,
  FileCode,
  FileSpreadsheet,
  TextCursorInput,
} from '@lucide/vue'
import { computed, ref, useTemplateRef, watch } from 'vue'

import { errorMessage } from '@/api/errorMessage'
import { FLAVOR_NAMES } from '@/api/flavors'
import type { Flavor } from '@/api/generated/Flavor'
import type { ServerInfo } from '@/api/generated/ServerInfo'
import { fetchKeywords } from '@/api/keywords'
import {
  type QueryTab,
  type UserFilter,
  executionDuration,
  formatBytes,
  formatDuration,
  formatInstant,
  formatRowCount,
  shownResult,
  showsGrid,
  transferDuration,
} from '@/api/querySession'
import { useExportsStore } from '@/stores/exports'

import { useClock } from './clock'
import { csvBlob, duplicateNames } from './csv'
import { MYSQL_VALUES_SINCE, SQL_TYPE, cteStatement, cteText } from './cte'
import { downloadName, saveBlob } from './download'
import { versionAtLeast } from './indexOperations'
import { frameIsFull, sameColumns } from './resultGrid'
import ResultFilter from './ResultFilter.vue'
import ResultGrid from './ResultGrid.vue'
import type { MenuItem } from './ui/menuItem'
import { notify } from './ui/toasts'
import UiAlert from './ui/UiAlert.vue'
import UiStatusBar from './ui/UiStatusBar.vue'

const props = defineProps<{
  /**
   * Displayed query.
   */
  tab: QueryTab
  /**
   * Is the tab on screen (active tab of the active editor)? Its grid then
   * feeds the data explorer.
   */
  displayed: boolean
  /**
   * Connection of the editor, on which the full export runs.
   */
  connectionId: number
  /**
   * Current database of the editor, `null` if none.
   */
  database: string | null
  /**
   * Server of the editor, `null` until its session is ready: its flavor,
   * its version and its modes write the CTE.
   */
  server: ServerInfo | null
  /**
   * Width of an indentation level of the editor, in spaces.
   */
  indentSize: number
}>()

const emit = defineEmits<{
  /**
   * The user writes the rows as a CTE in the editor, in the dialect of its
   * server.
   */
  'insert-cte': [query: string]
}>()

/**
 * User filter of the tab's grid.
 */
const filter = defineModel<UserFilter>('filter', { required: true })

const clock = useClock()

/**
 * Displayed result (see `shownResult`).
 */
const shown = computed(() => shownResult(props.tab))

/**
 * Does the grid come first, right under the result tabs (no error nor
 * filter form above it)?
 */
const attached = computed(() => !props.tab.error && !props.tab.filterOpen && showsGrid(props.tab))

// New columns: a targeted filter becomes global, its text kept. The same
// columns again (a session resumed after a reconnection) keep it.
watch(
  () => shown.value?.columns,
  (columns, previous) => {
    if (
      columns !== undefined &&
      previous !== undefined &&
      !sameColumns(columns, previous) &&
      filter.value.column !== null
    ) {
      filter.value = { ...filter.value, column: null }
    }
  },
)

const grid = useTemplateRef<InstanceType<typeof ResultGrid>>('grid')

/**
 * Number of rows the user filter keeps, `null` without a filter.
 */
const keptCount = computed(() =>
  filter.value.text === '' ? null : (grid.value?.keptCount ?? null),
)

/**
 * Number of rows shown in the status bar: read on each tick of the clock,
 * and at the end of the query.
 */
const shownCount = ref(shown.value?.rowCount ?? 0)

/**
 * Received volume shown in the status bar, in bytes, read like the number of
 * rows.
 */
const shownBytes = ref(props.tab.bytes)

watch(
  [clock, () => props.tab.done, shown],
  () => {
    shownCount.value = shown.value?.rowCount ?? 0
    shownBytes.value = props.tab.bytes
  },
  { immediate: true },
)

/**
 * Instant the durations are measured at: the clock's during the query, the
 * current one after, so that an ended query is no longer redrawn on each tick.
 */
const now = computed(() =>
  props.tab.startedAt !== null && !props.tab.done ? clock.value : performance.now(),
)

/**
 * Execution time of the query, in milliseconds; `null` while it waits for
 * its turn.
 */
const execution = computed(() => executionDuration(props.tab, now.value))

/**
 * Transfer time of the rows, in milliseconds; `null` before the columns.
 */
const transfer = computed(() => transferDuration(props.tab, now.value))

/**
 * Throughput of the transfer, in bytes per second; `null` before it has
 * lasted.
 */
const rate = computed(() =>
  transfer.value === null || transfer.value <= 0
    ? null
    : (shownBytes.value * 1000) / transfer.value,
)

/**
 * Does the status bar have an item to show?
 */
const hasStatus = computed(
  () => showsGrid(props.tab) || execution.value !== null || transfer.value !== null,
)

const exports = useExportsStore()

/**
 * Name of the downloaded files, before their date: that of the tab.
 */
const fileLabel = computed(() => props.tab.name ?? `requête-${props.tab.number + 1}`)

/**
 * What each dialect of the CTE reads, in the tooltip of its choice.
 */
const CTE_HINTS: Record<Flavor, string> = {
  mysql: 'Lignes en VALUES ROW (…), que lit MySQL 8.0.19 et suivants.',
  mariadb: 'Lignes en VALUES (…), que lit MariaDB.',
}

/**
 * Does the server of the editor read the CTE of its own dialect? MySQL reads
 * `VALUES ROW (…)` from 8.0.19.
 */
const readsCte = computed(
  () =>
    props.server !== null &&
    (props.server.flavor === 'mariadb' || versionAtLeast(props.server.version, MYSQL_VALUES_SINCE)),
)

/**
 * Choices of the download menu, drawn by the cell right of the result tabs.
 * The first one says what it downloads, so that a forgotten selection shows;
 * it waits for the end of the query. The CTE follows, as a file or in the
 * clipboard, for each dialect (that of the editor's server first), then in
 * the editor; it needs a row and the session. The last one exports the full
 * result.
 */
const downloadItems = computed<MenuItem[]>(() => {
  const size = grid.value?.selectedSize ?? null
  const kept = grid.value?.keptCount ?? 0
  const label =
    size === null
      ? `Résultat affiché en CSV (${formatRowCount(kept, null)})`
      : `Sélection en CSV (${formatRowCount(size.rows, null)} × ${size.columns} ${size.columns > 1 ? 'colonnes' : 'colonne'})`
  const scope = size === null ? 'le résultat affiché' : 'la sélection'
  const cteOff = !props.tab.done || props.server === null || (size?.rows ?? kept) === 0
  const flavors: Flavor[] =
    props.server?.flavor === 'mariadb' ? ['mariadb', 'mysql'] : ['mysql', 'mariadb']
  const dialects = (action: (flavor: Flavor) => Promise<void>): MenuItem[] =>
    flavors.map((flavor) => ({
      label: FLAVOR_NAMES[flavor],
      icon: Database,
      hint: CTE_HINTS[flavor],
      onSelect: () => void action(flavor),
    }))
  const server = props.server
  return [
    { label, icon: FileSpreadsheet, disabled: !props.tab.done, onSelect: () => void downloadCsv() },
    {
      label: `${size === null ? 'Résultat affiché' : 'Sélection'} en CTE (.sql)`,
      icon: FileCode,
      disabled: cteOff,
      items: dialects(downloadCte),
    },
    {
      label: `Copier ${scope} en CTE`,
      icon: ClipboardCopy,
      disabled: cteOff,
      items: dialects(copyCte),
    },
    {
      label: `Insérer ${scope} en CTE dans l’éditeur`,
      icon: TextCursorInput,
      disabled: cteOff || !readsCte.value,
      hint:
        server !== null && !readsCte.value
          ? `MySQL ${server.version} ne lit pas VALUES ROW (…) : il faut MySQL 8.0.19 ou suivant.`
          : 'Ajoute la CTE après la dernière requête de l’éditeur, sans l’exécuter, dans le dialecte de son serveur.',
      onSelect: () => void insertCte(),
    },
    {
      label: 'Résultat complet en CSV (requête relancée)',
      icon: DatabaseZap,
      hint: 'Relance la requête sans la limite de Prisme et suit l’export dans le gestionnaire d’exports, en haut à droite. Les colonnes gardent leur ordre affiché ; le tri et le filtre de la grille ne s’appliquent pas.',
      onSelect: () => void exportFull(),
    },
  ]
})

/**
 * Names shared by several displayed columns, refused by a download: they
 * could not be told apart in the file.
 *
 * @param fields Index of each downloaded column in the result.
 */
function sharedNames(fields: number[]): string[] {
  const columns = shown.value?.columns ?? []
  return duplicateNames(fields.map((field) => columns[field] ?? ''))
}

/**
 * Downloads the selection, or the displayed result, as CSV. Columns sharing a
 * name refuse the download: they could not be told apart in the file.
 */
async function downloadCsv(): Promise<void> {
  const result = shown.value
  if (result === null || grid.value === null) {
    return
  }
  const frame = await grid.value.exportFrame()
  const duplicates = sharedNames(frame.fields)
  if (duplicates.length > 0) {
    notify(`Téléchargement refusé : colonnes de même nom (${duplicates.join(', ')}).`, 'error')
    return
  }
  const blob = await csvBlob(frame, result.rows, result.columns, result.columnTypes)
  saveBlob(blob, downloadName(fileLabel.value, 'csv'))
  notify(`CSV téléchargé : ${formatRowCount(frame.rows.length, null)}.`)
}

/**
 * Writes the selection, or the displayed result, as a CTE (`cte.ts`), in the
 * modes of the editor's session. Columns sharing a name, or a discontinuous
 * selection, refuse it: the names of a CTE must differ, and an unselected
 * cell has no value to write.
 *
 * @param flavor Dialect written.
 * @returns
 *     The CTE, without its separator, and its number of rows; `null` if
 *     refused.
 */
async function cteOf(flavor: Flavor): Promise<{ query: string; rows: number } | null> {
  const result = shown.value
  const server = props.server
  if (result === null || grid.value === null || server === null) {
    return null
  }
  const frame = await grid.value.exportFrame()
  const duplicates = sharedNames(frame.fields)
  if (duplicates.length > 0) {
    notify(`CTE refusée : colonnes de même nom (${duplicates.join(', ')}).`, 'error')
    return null
  }
  if (!frameIsFull(frame)) {
    notify(
      'CTE refusée : la sélection est discontinue, une cellule non sélectionnée n’a pas de valeur à écrire.',
      'error',
    )
    return null
  }
  const keywords = await fetchKeywords(flavor, flavor === server.flavor ? server.version : null)
  const query = await cteText(frame, result.rows, result.columns, result.columnTypes, {
    flavor,
    ansiQuotes: server.ansi_quotes,
    backslashEscapes: !server.no_backslash_escapes,
    reserved: new Set(keywords.reserved),
    indentSize: props.indentSize,
  })
  return { query, rows: frame.rows.length }
}

/**
 * Downloads the CTE of the selection, or of the displayed result, as a
 * `.sql` file.
 *
 * @param flavor Dialect written.
 */
async function downloadCte(flavor: Flavor): Promise<void> {
  try {
    const cte = await cteOf(flavor)
    if (cte === null) {
      return
    }
    const blob = new Blob([`${cteStatement(cte.query, ';')}\n`], { type: SQL_TYPE })
    saveBlob(blob, downloadName(fileLabel.value, 'sql'))
    notify(`CTE ${FLAVOR_NAMES[flavor]} téléchargée : ${formatRowCount(cte.rows, null)}.`)
  } catch (failure) {
    notify(errorMessage(failure), 'error')
  }
}

/**
 * Copies the CTE of the selection, or of the displayed result, to the
 * clipboard.
 *
 * @param flavor Dialect written.
 */
async function copyCte(flavor: Flavor): Promise<void> {
  try {
    const cte = await cteOf(flavor)
    if (cte === null) {
      return
    }
    await navigator.clipboard.writeText(`${cteStatement(cte.query, ';')}\n`)
    notify(`CTE ${FLAVOR_NAMES[flavor]} copiée : ${formatRowCount(cte.rows, null)}.`)
  } catch (failure) {
    notify(errorMessage(failure), 'error')
  }
}

/**
 * Hands the CTE of the selection, or of the displayed result, to the cell,
 * which writes it in the editor, in the dialect of its server.
 */
async function insertCte(): Promise<void> {
  if (props.server === null) {
    return
  }
  try {
    const cte = await cteOf(props.server.flavor)
    if (cte !== null) {
      emit('insert-cte', cte.query)
    }
  } catch (failure) {
    notify(errorMessage(failure), 'error')
  }
}

/**
 * Exports the full result, its columns in the displayed order. The total is
 * known in advance when the displayed result is complete: ended without
 * error, and not cut by the limit of Prisme.
 */
async function exportFull(): Promise<void> {
  const result = shown.value
  if (result === null || grid.value === null) {
    return
  }
  const columns = [...grid.value.order]
  const duplicates = sharedNames(columns)
  if (duplicates.length > 0) {
    notify(`Export refusé : colonnes de même nom (${duplicates.join(', ')}).`, 'error')
    return
  }
  const complete = props.tab.done && props.tab.error === null && !result.truncated
  const name = downloadName(fileLabel.value, 'csv')
  try {
    await exports.begin({
      connection_id: props.connectionId,
      database: props.database ?? undefined,
      sql: props.tab.sql,
      columns,
      name,
      expected_rows: complete ? result.rowCount : undefined,
    })
    notify(`Export lancé : ${name}. Suivi dans le gestionnaire d’exports, en haut à droite.`)
  } catch (failure) {
    notify(errorMessage(failure), 'error')
  }
}

defineExpose({ downloadItems })
</script>

<template>
  <!--
    min-w-0: without it, the tab (flex item of a row) never shrinks below the
    width of its content, and a wide grid overflowed, cut off on the right,
    instead of scrolling in its own area.
  -->
  <div class="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-3" :class="{ 'pt-1': !attached }">
    <ui-alert v-if="tab.error" tone="error" :title="tab.error" />
    <result-filter
      v-if="shown && showsGrid(tab) && tab.filterOpen"
      v-model="filter"
      :columns="shown.columns"
    />
    <result-grid
      v-if="shown && showsGrid(tab)"
      ref="grid"
      :attached="attached"
      :filter="filter"
      :columns="shown.columns"
      :right-aligned="shown.rightAligned"
      :column-types="shown.columnTypes"
      :rows="shown.rows"
      :row-count="shown.rowCount"
      :sql="tab.sql"
      :displayed="displayed"
    />
    <ui-status-bar v-if="hasStatus">
      <span v-if="showsGrid(tab)">{{ formatRowCount(shownCount, keptCount) }}</span>
      <span v-if="execution !== null">exécution {{ formatDuration(execution) }}</span>
      <template v-if="transfer !== null">
        <span>transfert {{ formatDuration(transfer) }}</span>
        <span>{{ formatBytes(shownBytes) }}</span>
      </template>
      <span v-if="rate !== null">{{ formatBytes(rate) }}/s</span>
      <span v-if="shown?.executedAt">{{ formatInstant(shown.executedAt) }}</span>
    </ui-status-bar>
  </div>
</template>
