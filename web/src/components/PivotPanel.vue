<!--
  Panel of the Pivoteur, under the results of a cell (`QueryCell.vue`,
  F-01): a pivot table of the result shown (`shownResult`), after the filter
  of its grid (C-01). It follows the result tab shown, each tab keeping its
  own configuration (F-04), saved with the tab: it survives a page reload, a
  refresh, and a new run of the queries, which carries it over on the server
  (`tab_pivot.rs`). When the columns of the result change, the configuration
  follows them by name, and the elements of a lost column leave it
  (`fittedConfig`), which also drops the precision of a column that holds no
  dates any more. The columns of the result and the zones (`PivotZones.vue`),
  then the table (`PivotGrid.vue`), which takes the remaining width, with its
  status bar: rows taken into account, groups, duration of the computation
  (F-06). The widths of the sections are those of the tab shown.

  The engine runs in a Worker of the page (`pivotClient.ts`), one per
  result: the rows are copied once the query has ended (F-07), while the
  panel is open; folded, the panel computes nothing and stops its Worker
  (F-02). The foreign keys of the columns, for their default measure, come
  from the catalog of the connection, read while the panel is open, without
  waiting for it. Each change of the configuration or of the filter of the grid
  computes the table again; a filter of the panel leaves the grid as it is
  (F-20); a new name only changes its labels, a sort (C-11, C-13) lays the table
  out again, without computing any aggregate, and a width only draws it again:
  each table gives the position of the columns given a width. An error of
  calculation empties the table and shows the value in cause with its row; any
  other refusal of the engine shows its message above the table, which keeps its
  last computation (F-22); a Worker that stops is replaced at the next change. A
  result without columns, or in error, has nothing to analyse (F-08).
-->
<script setup lang="ts">
import { LoaderCircle } from '@lucide/vue'
import { computed, onBeforeUnmount, ref, shallowRef, toRaw, watch } from 'vue'

import { updateResultPivot } from '@/api/editors'
import { errorMessage } from '@/api/errorMessage'
import {
  type Cell,
  type QueryResult,
  type QueryTab,
  formatDuration,
  formatRowCount,
  shownResult,
} from '@/api/querySession'

import { detectContent } from './contentKind'
import PanelHeader from './PanelHeader.vue'
import PivotLayoutPanel from './PivotLayoutPanel.vue'
import PivotGrid from './PivotGrid.vue'
import PivotZones from './PivotZones.vue'
import { PivotClient, type PivotTable } from './pivotClient'
import {
  type DatePrecision,
  type PivotAxis,
  type PivotConfig,
  activeFilterPlace,
  elementLabel,
  emptyConfig,
  engineLayout,
  engineOrder,
  engineRequest,
  engineTargets,
  fittedConfig,
  hasTable,
  isEmptyConfig,
  sameOrder,
  sameRequest,
  sameTableLayout,
  withColumnWidth,
  withLabelWidth,
  withLevelSort,
  withValueSort,
  zoneLabels,
} from './pivotConfig'
import {
  type PivotFailure,
  type PivotLayout,
  type PivotOrder,
  type PivotRequest,
  type PivotValueTarget,
  type PivotWindow,
  PivotError,
} from './pivotEngine'
import { targetKey } from './pivotWidths'
import { sameColumns, visibleRows } from './resultGrid'
import { isForeignKey, useCatalogStore } from '@/stores/catalog'
import { columnIcon } from './typeIcons'
import UiAlert from './ui/UiAlert.vue'
import UiStatusBar from './ui/UiStatusBar.vue'

const props = defineProps<{
  /**
   * Result tab shown, absent without any tab.
   */
  tab: QueryTab | undefined
  /**
   * Is the panel unfolded?
   */
  open: boolean
  /**
   * Connection of the cell.
   */
  connectionId: number
}>()

const emit = defineEmits<{
  /**
   * The user folds the panel.
   */
  collapse: []
}>()

/**
 * Result shown in the tab.
 */
const result = computed(() => (props.tab ? shownResult(props.tab) : null))

/**
 * Has the result columns to analyse? Not without columns, nor in error
 * (F-08).
 */
const analysable = computed(
  () => props.tab?.error === null && (result.value?.columns.length ?? 0) > 0,
)

/**
 * Are the rows of the result still being received? The panel waits for
 * the end of the query (F-07); the result kept during a refresh is whole.
 */
const receiving = computed(() => props.tab?.result === result.value && !props.tab?.done)

/**
 * Result the Worker works on: that of the tab shown, whole, while the
 * panel is open; `null` otherwise.
 */
const source = computed(() =>
  props.open && analysable.value && !receiving.value ? result.value : null,
)

/**
 * Keeps a configuration in its tab, and saves it with the tab. A refused save
 * shows in the top bar (`sync.ts`); the tab keeps its configuration on the
 * page.
 *
 * @param tab Tab of the configuration.
 * @param columns Names of the columns of its result.
 * @param value Configuration.
 */
function keepConfig(tab: QueryTab, columns: string[], value: PivotConfig): void {
  tab.pivot = { fields: [...columns], config: value }
  updateResultPivot(tab.id, tab.pivot).catch(() => {})
}

// The columns of a result have changed: the configuration is fitted to them
// once and for all. A result without columns, or in error, leaves it as it
// is, for the next run.
watch(
  () => [props.tab, result.value?.columns, result.value?.columnTypes] as const,
  ([tab, columns, types]) => {
    if (
      tab !== undefined &&
      columns !== undefined &&
      columns.length > 0 &&
      !isEmptyConfig(tab.pivot.config) &&
      !sameColumns(tab.pivot.fields, columns)
    ) {
      keepConfig(tab, columns, fittedConfig(tab.pivot, columns, types ?? []))
    }
  },
  { immediate: true },
)

/**
 * Configuration without any element, without a tab or a result.
 */
const NO_CONFIG = emptyConfig()

/**
 * Configuration of the tab shown.
 */
const config = computed<PivotConfig>({
  get() {
    const tab = props.tab
    const shown = result.value
    return tab && shown ? fittedConfig(tab.pivot, shown.columns, shown.columnTypes) : NO_CONFIG
  },
  set(value) {
    if (props.tab && result.value) {
      keepConfig(props.tab, result.value.columns, value)
    }
  },
})

/**
 * Table to ask the engine for the configuration shown. The same object
 * while only the names change: the table is not computed again.
 */
const request = computed<PivotRequest>((previous) => {
  const next = engineRequest(config.value)
  return previous !== undefined && sameRequest(previous, next) ? previous : next
})

/**
 * Order of the table of the configuration shown. The same object while it
 * stays the same: a change of it lays the table out again, without computing
 * it.
 */
const order = computed<PivotOrder>((previous) => {
  const next = engineOrder(config.value)
  return previous !== undefined && sameOrder(previous, next) ? previous : next
})

/**
 * Layout of the table of the configuration shown. The same object while it
 * stays the same: a change of it lays the table out again, without computing
 * it.
 */
const layout = computed<PivotLayout>((previous) => {
  const next = engineLayout(config.value)
  return previous !== undefined && sameTableLayout(previous, next) ? previous : next
})

/**
 * Inverts the direction of a level.
 *
 * @param axis Axis of the level.
 * @param level Position of the level.
 */
function sortLevel(axis: PivotAxis, level: number): void {
  config.value = withLevelSort(config.value, axis, level)
}

/**
 * Changes the sort of an axis by the values of a node of the other axis.
 *
 * @param axis Axis the values rank.
 * @param target Node and measure of the values.
 */
function sortValues(axis: PivotAxis, target: PivotValueTarget): void {
  config.value = withValueSort(config.value, axis, target)
}

/**
 * Gives a column of values a width, or its default width back.
 *
 * @param target Values of the node of the column, and its measure.
 * @param width Width, in steps of the spacing scale; `null` for the default width.
 */
function resizeColumn(target: PivotValueTarget, width: number | null): void {
  config.value = withColumnWidth(config.value, target, width)
}

/**
 * Gives the labels of the lines a width, or their default width back.
 *
 * @param level Level of the rows whose column it is; `null` for the labels in a single column.
 * @param width Width, in steps of the spacing scale; `null` for the default width.
 */
function resizeLabels(level: number | null, width: number | null): void {
  config.value = withLabelWidth(config.value, level, width)
}

/**
 * Mark of the type of each column, as in the headers of the grid.
 */
const marks = computed(() => {
  const shown = result.value
  if (shown === null) {
    return []
  }
  // Read so that the contents are detected again when rows arrive.
  void shown.rowCount
  return shown.columns.map((_, column) => {
    const type = shown.columnTypes[column]
    if (type === undefined) {
      return undefined
    }
    const content = type.family === 'text' ? detectContent(toRaw(shown.rows), column) : null
    return columnIcon(type, content)
  })
})

const catalog = useCatalogStore()

// Without a catalog, a foreign key takes the measure of any other column.
watch(
  source,
  (shown) => {
    if (shown !== null) {
      catalog.load(props.connectionId).catch(() => {})
    }
  },
  { immediate: true },
)

/**
 * Is each column part of a foreign key of its table?
 */
const foreignKeys = computed(() => {
  const known = catalog.catalogOf(props.connectionId)
  return (result.value?.columnTypes ?? []).map((type) => isForeignKey(known, type.origin))
})

/**
 * Client of the Worker, and the result it holds.
 */
let client: PivotClient | null = null
let clientResult: QueryResult | null = null

/**
 * Filter of the grid sent last to the Worker, `''` for all the rows.
 */
let sentFilter: string | null = null

/**
 * Are the rows being copied into the Worker?
 */
const copying = ref(false)

/**
 * Is a table being computed?
 */
const computing = ref(false)

/**
 * Table shown, with the request, the configuration and the layout it was
 * computed for, and the position of each column given a width, by its key
 * (`targetKey`).
 */
const shownTable = shallowRef<{
  table: PivotTable
  request: PivotRequest
  config: PivotConfig
  layout: PivotLayout
  positions: ReadonlyMap<string, number>
} | null>(null)

/**
 * Message of the last refusal of the engine, `null` without one.
 */
const failure = ref<string | null>(null)

/**
 * Number of the last computation asked: the answers of the earlier ones
 * are dropped.
 */
let computations = 0

/**
 * Stops the Worker and forgets its table.
 */
function closeClient(): void {
  client?.close()
  client = null
  clientResult = null
  sentFilter = null
  copying.value = false
}

onBeforeUnmount(closeClient)

/**
 * Starts a Worker for a result, and copies its rows.
 *
 * @param shown Result to copy.
 */
function startClient(shown: QueryResult): PivotClient {
  const started = PivotClient.start(shown)
  client = started
  clientResult = shown
  copying.value = true
  const copied = () => {
    if (client === started) {
      copying.value = false
    }
  }
  started.loaded.then(copied, copied)
  return started
}

/**
 * Sends the rows the filter of the grid keeps, when it has changed.
 *
 * @param current Client of the Worker.
 * @param shown Result of the Worker.
 */
function sendKept(current: PivotClient, shown: QueryResult): void {
  const filter = props.tab?.filter
  const text = filter?.text.trim() ?? ''
  const key = text === '' ? '' : `${filter?.column ?? ''} ${filter?.text}`
  if (key !== sentFilter) {
    sentFilter = key
    current.setKept(filter && key !== '' ? visibleRows(toRaw(shown.rows), filter, null) : null)
  }
}

/**
 * Refusals that are errors of calculation: the table empties, its values being wrong.
 */
const CALCULATION_ERRORS = [
  'invalid_number',
  'too_large',
  'invalid_date',
  'overflow',
  'filter_overflow',
]

/**
 * Message of a refusal of the engine, in French.
 *
 * @param refusal Refusal of the engine.
 * @param refused Configuration refused.
 */
function refusalText(refusal: PivotFailure, refused: PivotConfig): string {
  const names = result.value?.columns ?? []
  const types = result.value?.columnTypes ?? []
  const column = refusal.column === undefined ? '?' : (names[refusal.column] ?? '?')
  const row = refusal.row === undefined ? '?' : (refusal.row + 1).toLocaleString('fr-FR')
  const rows = result.value === null ? [] : toRaw(result.value.rows)
  const value = rows[refusal.row ?? -1]?.[refusal.column ?? -1] ?? '?'
  const measure =
    refusal.measure === undefined
      ? '?'
      : elementLabel(refused, { zone: 'measures', index: refusal.measure }, names)
  const filter =
    refusal.filter === undefined
      ? '?'
      : elementLabel(refused, activeFilterPlace(refused, refusal.filter), names, types)
  switch (refusal.kind) {
    case 'invalid_number':
      return `La valeur « ${value} » de la ligne ${row} de « ${column} » n’est pas un nombre.`
    case 'too_large':
      return `La valeur « ${value} » de la ligne ${row} de « ${column} » dépasse 38 chiffres.`
    case 'invalid_date':
      return types[refusal.column ?? -1]?.family === 'time'
        ? `La valeur « ${value} » de la ligne ${row} de « ${column} » n’est pas une heure de la journée.`
        : `La valeur « ${value} » de la ligne ${row} de « ${column} » n’est pas une date du calendrier.`
    case 'overflow':
      return `${measure} dépasse 38 chiffres.`
    case 'not_numeric':
      return `${measure} demande une colonne de nombres.`
    case 'filter_overflow':
      return `Le filtre ${filter} dépasse 38 chiffres.`
    case 'filter_not_numeric':
      return `Le filtre ${filter} demande une colonne de nombres.`
    case 'filter_operand':
      return `Une valeur du filtre ${filter} ne se compare pas à ce qu’il filtre.`
    case 'crash':
    case 'module':
      return `Le moteur du Pivoteur s’est arrêté : ${refusal.detail ?? refusal.kind}`
    default:
      return `Le moteur du Pivoteur refuse le calcul (${refusal.kind}).`
  }
}

/**
 * Computes the table of the configuration shown.
 *
 * @param current Client of the Worker.
 */
async function compute(current: PivotClient): Promise<void> {
  const computation = ++computations
  const asked = request.value
  const named = config.value
  const arranged = layout.value
  const targets = engineTargets(named)
  computing.value = true
  try {
    const table = await current.compute(asked, order.value, arranged, targets)
    if (computation === computations) {
      const positions = new Map(
        targets.flatMap((target, index): [string, number][] => {
          const position = table.positions[index]
          return position === null || position === undefined ? [] : [[targetKey(target), position]]
        }),
      )
      shownTable.value = { table, request: asked, config: named, layout: arranged, positions }
      failure.value = null
    }
  } catch (error) {
    if (computation === computations && current === client) {
      failure.value =
        error instanceof PivotError ? refusalText(error.failure, named) : errorMessage(error)
      if (error instanceof PivotError && CALCULATION_ERRORS.includes(error.failure.kind)) {
        shownTable.value = null
      }
      if (error instanceof PivotError && ['crash', 'module'].includes(error.failure.kind)) {
        closeClient()
      }
    }
  } finally {
    if (computation === computations) {
      computing.value = false
    }
  }
}

/**
 * Follows the result, the configuration and the filter of the grid: starts
 * the Worker of a new result, then computes the table.
 */
function update(): void {
  const shown = source.value
  if (shown !== clientResult || client === null) {
    closeClient()
    shownTable.value = null
    failure.value = null
    if (shown !== null) {
      startClient(shown)
    }
  }
  if (client === null || shown === null) {
    return
  }
  if (!hasTable(config.value)) {
    computations++
    computing.value = false
    shownTable.value = null
    failure.value = null
    return
  }
  sendKept(client, shown)
  void compute(client)
}

watch([source, request, order, layout, () => props.tab?.filter], update, { immediate: true })

/**
 * Reads the distinct values of a column of the result shown, or the periods of a precision of its
 * dates.
 *
 * @param column Index of the column.
 * @param precision What the column keeps of its dates.
 */
function readValues(column: number, precision: DatePrecision): Promise<Cell[]> {
  return client ? client.values(column, precision) : Promise.reject(new Error('No pivot engine'))
}

/**
 * Reads a window of the table shown.
 *
 * @param lines Lines, start included, end excluded.
 * @param columns Columns of values, start included, end excluded.
 */
function readWindow(lines: [number, number], columns: [number, number]): Promise<PivotWindow> {
  const shown = shownTable.value
  return client && shown
    ? client.window(shown.table.id, lines, columns)
    : Promise.reject(new Error('No pivot table'))
}

/**
 * Configuration that names the elements of the table shown: the one
 * shown, when only its names differ from that of the table.
 */
const tableConfig = computed(() => {
  const shown = shownTable.value
  return shown === null || shown.request === request.value ? config.value : shown.config
})

/**
 * Labels of the measures and names of the levels of the table shown.
 */
const tableLabels = computed(() => {
  const names = result.value?.columns ?? []
  return {
    measures: zoneLabels(tableConfig.value, 'measures', names),
    rows: zoneLabels(tableConfig.value, 'rows', names),
    columns: zoneLabels(tableConfig.value, 'columns', names),
  }
})

/**
 * Rows taken into account, out of those of the result when the filter of
 * the grid keeps only some of them.
 */
const rowsText = computed(() => {
  const shape = shownTable.value?.table.shape
  const total = result.value?.rowCount ?? 0
  return shape ? formatRowCount(total, shape.rows === total ? null : shape.rows) : ''
})

/**
 * Number of groups of the table shown.
 */
const groupsText = computed(() => {
  const groups = shownTable.value?.table.shape.groups ?? 0
  return `${groups.toLocaleString('fr-FR')} ${groups > 1 ? 'groupes' : 'groupe'}`
})
</script>

<template>
  <section class="flex min-h-0 min-w-0 flex-1 flex-col gap-2 px-4 pt-1 pb-3" aria-label="Pivoteur">
    <panel-header title="Pivoteur" side="bottom" @collapse="emit('collapse')">
      <template v-if="analysable && !receiving" #actions>
        <pivot-layout-panel v-model:config="config" :names="result?.columns ?? []" />
      </template>
    </panel-header>
    <p v-if="!analysable" class="text-fg-secondary">Rien à analyser</p>
    <p v-else-if="receiving" role="status" class="flex items-center gap-2 text-fg-secondary">
      <LoaderCircle class="size-5 animate-spin" aria-hidden="true" />
      En attente de la fin de la requête…
    </p>
    <!-- Its sections take the widths of each tab as they start. -->
    <pivot-zones
      v-else
      :key="tab?.id"
      v-model:config="config"
      :names="result?.columns ?? []"
      :types="result?.columnTypes ?? []"
      :marks="marks"
      :foreign-keys="foreignKeys"
      :read-values="readValues"
    >
      <div class="flex min-h-0 min-w-0 flex-1 flex-col gap-3">
        <ui-alert v-if="failure" tone="error" :title="failure" />
        <pivot-grid
          v-if="shownTable"
          :table="shownTable.table.id"
          :shape="shownTable.table.shape"
          :lengths="shownTable.table.lengths"
          :measure-labels="tableLabels.measures"
          :row-names="tableLabels.rows"
          :column-names="tableLabels.columns"
          :order="order"
          :layout="shownTable.layout"
          :repeat-labels="config.repeat_labels === true"
          :column-widths="tableConfig.column_widths ?? []"
          :positions="shownTable.positions"
          :labels-width="tableConfig.labels_width"
          :level-widths="tableConfig.rows.map((level) => level.width)"
          :read="readWindow"
          @sort-level="sortLevel"
          @sort-values="sortValues"
          @resize-column="resizeColumn"
          @resize-labels="resizeLabels"
        />
        <p
          v-else-if="!hasTable(config)"
          class="flex flex-1 items-center justify-center text-center text-balance text-fg-secondary"
        >
          Glissez des colonnes dans les zones Lignes, Colonnes et Mesures, ou ajoutez-les par leur
          menu « + »
        </p>
        <div v-else class="flex-1" />
        <ui-status-bar role="status">
          <span v-if="copying">Copie des lignes…</span>
          <span v-else-if="computing">Calcul…</span>
          <template v-if="shownTable">
            <span>{{ rowsText }}</span>
            <span>{{ groupsText }}</span>
            <span>calculé en {{ formatDuration(shownTable.table.durationMs) }}</span>
          </template>
        </ui-status-bar>
      </div>
    </pivot-zones>
  </section>
</template>
