<!--
  Content of a query's tab: error if any, form of the user filter, result
  grid, then the status bar, one item each:

  - number of rows, with a grid: those received, preceded by those the user
    filter keeps ("120 / 5 000 lignes"); while the rows of an ended query
    are still arriving (saved rows read again, or taken back by the session),
    those received out of those awaited ("Chargement : 120 000 / 1 000 000
    lignes"), followed by those the user filter keeps;
  - execution time of the query by the server (until the columns arrive);
  - transfer and display time of the rows, then the received volume and the
    throughput;
  - end of the execution, date and time to the second: the instant the
    result reflects.

  During the query, the number of rows, the durations and the volume change
  once per second (shared clock, `clock.ts`); at the end, they take their
  final value at once. A query without a grid (waiting, failed, or without
  columns) shows only its durations. Without a grid, the free room keeps the
  status bar at the bottom, and shows "Calcul en cours…" with a spinner until
  the query ends. A statement without columns that succeeded then shows what
  it did (`outcomeSummary`: "3 lignes insérées", "2 lignes modifiées sur 3
  trouvées"), from its verb and from what its engine reported, repeated at
  the start of the status bar, with the id of the row it inserted under it. Its
  warnings, when the engine counted some, turn its icon into a warning and
  show as a badge in the status bar ("2 avertissements"); their text follows
  the summary, one line each, level and code first ("Avertissement 1265"),
  then how many the engine did not keep. The free room scrolls when they do
  not fit.

  The user filter belongs to the tab (`filter`, `v-model:filter`): its form
  (`ResultFilter.vue`) shows above the grid while its button, right of the
  result tabs, keeps it expanded. New columns make it global, its text kept:
  the index of a targeted column would point to another column.

  A query stopped by the user (`interrupted`) shows no error: its grid holds
  the rows received, the status bar starts with a "Résultat tronqué" badge;
  stopped before its columns, it says "Requête arrêtée.".

  The grid comes right under the line of the result tabs, which takes the
  place of its frame's top border; the error, the filter's form or the
  durations, when they come first, keep a little room under that line.

  During a refresh, the previous result stays displayed until the new
  columns arrive.

  The tab gives the menu of its export button (`exportGroups`), which the
  editor view draws right of the result tabs, beside the refresh and filter buttons
  (`ResultTabActions.vue`), its choices grouped by destination: file,
  clipboard, editor. It downloads the result as CSV for Excel in
  French (`csv.ts`): the selection of the grid, or the whole displayed
  result without one, sorted and filtered as on screen, its columns in the
  displayed order. The file is built in the page, by chunks of rows. The
  same rows can become a CTE (`cte.ts`): downloaded or copied, for MySQL or
  MariaDB, or written in the editor in the dialect of its server, which the
  editor view does (`insert-cte`). Columns sharing a name, or a discontinuous
  selection, refuse it. The selection can also become an `IN` clause
  (`inClause.ts`), in the dialect of the editor's server: copied, or
  written at the editor's cursor (`insert-in`). It needs a selection whose
  rows are all selected on the same columns; otherwise its choices stay
  inactive, their tooltip saying why. The menu also offers the full result:
  the server runs the query again, as it is written, without the limit of Prisme, on
  the current database of the editor, and writes its rows in the displayed
  order of the columns, neither sorted nor filtered by the grid; the exports
  manager of the top bar follows it (`stores/exports.ts`). Its total, when
  known in advance, gives a completed share: the number of rows of a
  displayed result that the limit did not cut.
-->
<script setup lang="ts">
import {
  CircleCheck,
  ClipboardCopy,
  Database,
  DatabaseZap,
  FileCode,
  FileSpreadsheet,
  LoaderCircle,
  TextCursorInput,
  TriangleAlert,
} from '@lucide/vue'
import { computed, ref, useTemplateRef, watch } from 'vue'

import { errorMessage } from '@/api/errorMessage'
import { FLAVOR_NAMES } from '@/api/flavors'
import type { Flavor } from '@/api/generated/Flavor'
import type { NoticeLevel } from '@/api/generated/NoticeLevel'
import type { ServerInfo } from '@/api/generated/ServerInfo'
import { fetchKeywords } from '@/api/keywords'
import {
  type QueryTab,
  type UserFilter,
  executionDuration,
  failed,
  formatBytes,
  formatDuration,
  formatInstant,
  formatRowCount,
  NOTICE_LABELS,
  droppedNotices,
  formatWarnings,
  insertedIdLabel,
  outcomeSummary,
  shownResult,
  showsGrid,
  transferDuration,
} from '@/api/querySession'
import { useExportsStore } from '@/stores/exports'

import { useClock } from '@/utils/clock'
import { csvBlob, duplicateNames } from './csv'
import { MYSQL_VALUES_SINCE, SQL_TYPE, cteStatement, cteText } from './cte'
import { inClause } from './inClause'
import { downloadName, saveBlob } from '@/utils/download'
import { versionAtLeast } from '@/components/navigator/indexOperations'
import { frameIsFull, sameColumns } from './resultGrid'
import ResultFilter from './ResultFilter.vue'
import ResultGrid from './ResultGrid.vue'
import { keepGridLayout, scheduleGridSave } from './gridDisplay'
import type { MenuGroup, MenuItem } from '@/components/ui/menuItem'
import { notify } from '@/components/ui/toasts'
import UiAlert from '@/components/ui/UiAlert.vue'
import UiStatusBar from '@/components/ui/UiStatusBar.vue'
import { type Tooltip, vTooltip } from '@/components/ui/tooltip'

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
   * Id of the editor, whose console receives the queries of the full
   * export.
   */
  editorId: number
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
  /**
   * The user writes the selection as an `IN` clause at the editor's cursor.
   */
  'insert-in': [clause: string]
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
 * Error shown in a banner: none for a query stopped by the user, whose rows
 * stay a result.
 */
const shownError = computed(() => (failed(props.tab) ? props.tab.error : null))

/**
 * What a statement without columns did, once it succeeded; `null` for a
 * result with a grid, a query running, failed or stopped.
 */
const summary = computed(() => {
  const result = shown.value
  if (result === null || showsGrid(props.tab) || !props.tab.done || props.tab.error !== null) {
    return null
  }
  return outcomeSummary(result.verb, result.outcome)
})

/**
 * Id of the row a statement without columns inserted, under its summary;
 * `null` without a summary or without an id.
 */
const insertedId = computed(() =>
  summary.value === null || shown.value === null ? null : insertedIdLabel(shown.value.outcome),
)

/**
 * Number of warnings of a statement without columns, 0 without a summary or
 * when the engine counted none.
 */
const warningCount = computed(() =>
  summary.value === null ? 0 : (shown.value?.outcome.warnings ?? 0),
)

/**
 * Warnings and notes of a statement without columns, as the engine sent
 * them; empty without a summary.
 */
const notices = computed(() =>
  summary.value === null || shown.value === null ? [] : shown.value.outcome.notices,
)

/**
 * Number of warnings the engine counted without keeping them.
 */
const dropped = computed(() =>
  summary.value === null || shown.value === null ? 0 : droppedNotices(shown.value.outcome),
)

/**
 * Color of the level of a notice, the classes written out so that Tailwind
 * finds them.
 */
const NOTICE_TONES: Record<NoticeLevel, string> = {
  note: 'text-info',
  warning: 'text-warning',
  error: 'text-error',
}

/**
 * Was the reception of the rows stopped by the user? The grid then holds
 * those received before the stop.
 */
const cut = computed(() => props.tab.interrupted && showsGrid(props.tab))

/**
 * Does the grid come first, right under the result tabs (no error nor
 * filter form above it)?
 */
const attached = computed(() => !shownError.value && !props.tab.filterOpen && showsGrid(props.tab))

// New columns: a targeted filter becomes global, its text kept. The same
// columns again (a session resumed after a reconnection) keep it. The
// filter carried over from the tab a new run replaced (`grid`) keeps its
// column only if the columns are the same.
watch(
  () => shown.value?.columns,
  (columns, previous) => {
    const made = previous ?? props.tab.grid?.columns
    if (
      columns !== undefined &&
      made !== undefined &&
      !sameColumns(columns, made) &&
      filter.value.column !== null
    ) {
      filter.value = { ...filter.value, column: null }
    }
  },
  { immediate: true },
)

// The filter and its form are saved with the display of the grid.
watch(
  () => [props.tab.filter, props.tab.filterOpen],
  () => scheduleGridSave(props.tab),
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
  () =>
    showsGrid(props.tab) ||
    summary.value !== null ||
    execution.value !== null ||
    transfer.value !== null,
)

const exports = useExportsStore()

/**
 * Name of the downloaded files, before their date: that of the tab.
 */
const fileLabel = computed(() => props.tab.name ?? `requête-${props.tab.number + 1}`)

/**
 * What each dialect of the CTE reads, in the tooltip of its choice.
 */
const CTE_HINTS: Record<Flavor, Tooltip> = {
  mysql: { title: 'Lignes en VALUES ROW (…)', description: 'Lu par MySQL 8.0.19 et suivants' },
  mariadb: { title: 'Lignes en VALUES (…)', description: 'Lu par MariaDB' },
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
 * Choices of the export menu, drawn by the editor view right of the result tabs,
 * grouped by destination. In a file: the CSV first, whose label says what it
 * holds, so that a forgotten selection shows; it waits for the end of the
 * query. Then the CTE, for each dialect (that of the editor's server first),
 * and the full result. In the clipboard: the CTE, for each dialect. In the
 * editor: the CTE, in the dialect of its server. The CTE needs a row and the
 * session. In the clipboard and in the editor, the selection as an `IN`
 * clause too, in the dialect of the editor's server, inactive without a
 * selection or with a malformed one.
 */
const exportGroups = computed<MenuGroup[]>(() => {
  const size = grid.value?.selectedSize ?? null
  const kept = grid.value?.keptCount ?? 0
  const label =
    size === null
      ? `Résultat affiché en CSV (${formatRowCount(kept, null)})`
      : `Sélection en CSV (${formatRowCount(size.rows, null)} × ${size.columns} ${size.columns > 1 ? 'colonnes' : 'colonne'})`
  const scope = size === null ? 'Résultat affiché' : 'Sélection'
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
  const inReason: Tooltip | null =
    size === null
      ? { title: 'Sélectionnez une ou plusieurs colonnes' }
      : grid.value?.selectedFull
        ? null
        : {
            title: 'Sélection malformée',
            description: 'Chaque ligne sélectionnée doit l’être sur les mêmes colonnes',
          }
  const inOff = inReason !== null || server === null
  return [
    {
      label: 'Fichier',
      items: [
        {
          label,
          icon: FileSpreadsheet,
          disabled: !props.tab.done,
          onSelect: () => void downloadCsv(),
        },
        {
          label: `${scope} en CTE (.sql)`,
          icon: FileCode,
          disabled: cteOff,
          items: dialects(downloadCte),
        },
        {
          label: 'Résultat complet en CSV (requête relancée)',
          icon: DatabaseZap,
          hint: {
            title: 'Relancer la requête sans la limite de Prisme',
            description:
              'L’export se suit dans le gestionnaire d’exports, en haut à droite. Les colonnes gardent leur ordre affiché ; le tri et le filtre de la grille ne s’appliquent pas.',
          },
          onSelect: () => void exportFull(),
        },
      ],
    },
    {
      label: 'Presse-papiers',
      items: [
        {
          label: `${scope} en CTE`,
          icon: ClipboardCopy,
          disabled: cteOff,
          items: dialects(copyCte),
        },
        {
          label: 'Sélection en clause IN',
          icon: ClipboardCopy,
          disabled: inOff,
          hint: inReason ?? {
            title: 'Copier les valeurs distinctes en clause IN',
            description:
              'Dans le dialecte du serveur de l’éditeur ; une colonne seule omet NULL, qui ne correspond jamais',
          },
          onSelect: () => void copyIn(),
        },
      ],
    },
    {
      label: 'Éditeur',
      items: [
        {
          label: `${scope} en CTE`,
          icon: TextCursorInput,
          disabled: cteOff || !readsCte.value,
          hint:
            server !== null && !readsCte.value
              ? {
                  title: `Indisponible avec MySQL ${server.version}`,
                  description: 'VALUES ROW (…) demande MySQL 8.0.19 ou suivant',
                }
              : {
                  title: 'Ajouter la CTE après la dernière requête',
                  description: 'Sans l’exécuter, dans le dialecte du serveur de l’éditeur',
                },
          onSelect: () => void insertCte(),
        },
        {
          label: 'Sélection en clause IN',
          icon: TextCursorInput,
          disabled: inOff,
          hint: inReason ?? {
            title: 'Écrire les valeurs distinctes en clause IN',
            description: 'Au curseur de l’éditeur, ou à la place du texte sélectionné',
          },
          onSelect: () => void insertIn(),
        },
      ],
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
 * Hands the CTE of the selection, or of the displayed result, to the editor
 * view, which writes it in the editor, in the dialect of its server.
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
 * Writes the selection as an `IN` clause (`inClause.ts`), in the dialect and
 * the modes of the editor's session. A selection of NULL values alone
 * refuses it: an `IN` needs a value.
 *
 * @returns The clause; `null` if refused.
 */
async function inOf(): Promise<string | null> {
  const result = shown.value
  const server = props.server
  if (result === null || grid.value === null || server === null || !grid.value.selectedFull) {
    return null
  }
  const frame = await grid.value.exportFrame()
  const clause = await inClause(frame, result.rows, result.columnTypes, {
    flavor: server.flavor,
    backslashEscapes: !server.no_backslash_escapes,
    indentSize: props.indentSize,
  })
  if (clause === null) {
    notify('Clause IN refusée : la sélection ne contient que NULL.', 'error')
  }
  return clause
}

/**
 * Copies the selection as an `IN` clause to the clipboard.
 */
async function copyIn(): Promise<void> {
  try {
    const clause = await inOf()
    if (clause !== null) {
      await navigator.clipboard.writeText(clause)
      notify('Clause IN copiée.')
    }
  } catch (failure) {
    notify(errorMessage(failure), 'error')
  }
}

/**
 * Hands the selection as an `IN` clause to the editor view, which writes it
 * at the editor's cursor.
 */
async function insertIn(): Promise<void> {
  try {
    const clause = await inOf()
    if (clause !== null) {
      emit('insert-in', clause)
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
      editor_id: props.editorId,
    })
    notify(`Export lancé : ${name}. Suivi dans le gestionnaire d’exports, en haut à droite.`)
  } catch (failure) {
    notify(errorMessage(failure), 'error')
  }
}

defineExpose({ exportGroups })
</script>

<template>
  <!--
    min-w-0: without it, the tab (flex item of a row) never shrinks below the
    width of its content, and a wide grid overflowed, cut off on the right,
    instead of scrolling in its own area.
  -->
  <div class="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-3" :class="{ 'pt-1': !attached }">
    <ui-alert v-if="shownError" tone="error" :title="shownError" />
    <ui-alert v-else-if="tab.interrupted && !showsGrid(tab)" tone="info" title="Requête arrêtée." />
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
      :layout="tab.grid"
      :displayed="displayed"
      @update:layout="(layout) => keepGridLayout(tab, layout)"
    />
    <!--
      Without a grid, this block takes its room and keeps the status bar at the
      bottom; it says the query runs until it ends, then what it did.
    -->
    <div
      v-else
      role="status"
      class="flex min-h-0 flex-1 items-center-safe justify-center gap-3 overflow-auto text-base text-fg-secondary"
    >
      <template v-if="!tab.done">
        <LoaderCircle class="size-5 animate-spin" aria-hidden="true" />
        Calcul en cours…
      </template>
      <template v-else-if="summary !== null">
        <TriangleAlert v-if="warningCount > 0" class="size-5 text-warning" aria-hidden="true" />
        <CircleCheck v-else class="size-5 text-success" aria-hidden="true" />
        <div class="flex min-w-0 flex-col gap-1">
          <span class="text-fg">{{ summary }}</span>
          <span v-if="insertedId !== null">{{ insertedId }}</span>
          <ul v-if="notices.length > 0" class="flex flex-col gap-1">
            <li v-for="(notice, index) in notices" :key="index" class="flex gap-2">
              <span class="shrink-0 font-medium" :class="NOTICE_TONES[notice.level]">
                {{ NOTICE_LABELS[notice.level] }} {{ notice.code }}
              </span>
              <span class="wrap-break-word text-fg">{{ notice.message }}</span>
            </li>
          </ul>
          <span v-if="dropped > 0">
            {{ formatWarnings(dropped) }} de plus, non conservés par le serveur
          </span>
        </div>
      </template>
    </div>
    <ui-status-bar v-if="hasStatus">
      <span
        v-if="cut"
        class="rounded-full bg-warning/14 px-2 font-medium text-warning"
        v-tooltip="{
          title: 'Réception arrêtée',
          description: `${formatRowCount(shown?.rowCount ?? 0, null)} reçues`,
        }"
      >
        Résultat tronqué
      </span>
      <span
        v-if="warningCount > 0"
        class="rounded-full bg-warning/14 px-2 font-medium text-warning"
      >
        {{ formatWarnings(warningCount) }}
      </span>
      <span v-if="summary !== null">{{ summary }}</span>
      <template v-if="showsGrid(tab)">
        <span v-if="tab.awaitedRows !== null" class="tabular-nums">
          Chargement : {{ formatRowCount(tab.awaitedRows, shownCount) }}
        </span>
        <span v-if="tab.awaitedRows === null || keptCount !== null">
          {{ formatRowCount(shownCount, keptCount) }}
        </span>
      </template>
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
