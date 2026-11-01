<!--
  Panel of the Pivoteur, under the results of a cell (`QueryCell.vue`,
  F-01): a pivot table of the result shown (`shownResult`), after the filter
  of its grid (C-01). It follows the result tab shown, each tab keeping its
  own configuration (F-04), forgotten when the columns of its result
  change. The columns of the result and the zones (`PivotZones.vue`), then
  the table (`PivotGrid.vue`), which takes the remaining width, with its
  status bar: rows taken into account, groups, duration of the computation
  (F-06).

  The engine runs in a Worker of the page (`pivotClient.ts`), one per
  result: the rows are copied once the query has ended (F-07), while the
  panel is open; folded, the panel computes nothing and stops its Worker
  (F-02). Each change of the configuration or of the filter of the grid
  computes the table again. A refusal of the engine shows its message above
  the table, which keeps its last computation (F-22); a Worker that stops
  is replaced at the next change. A result without columns, or in error,
  has nothing to analyse (F-08).
-->
<script setup lang="ts">
import { LoaderCircle } from '@lucide/vue'
import { computed, onBeforeUnmount, ref, shallowReactive, shallowRef, toRaw, watch } from 'vue'

import { errorMessage } from '@/api/errorMessage'
import {
  type QueryResult,
  type QueryTab,
  formatDuration,
  formatRowCount,
  shownResult,
} from '@/api/querySession'

import { detectContent } from './contentKind'
import PanelHeader from './PanelHeader.vue'
import PivotGrid from './PivotGrid.vue'
import PivotZones from './PivotZones.vue'
import { PivotClient, type PivotTable } from './pivotClient'
import { emptyConfig, isEmptyConfig, measureLabel } from './pivotConfig'
import { type PivotFailure, type PivotRequest, type PivotWindow, PivotError } from './pivotEngine'
import { sameColumns, visibleRows } from './resultGrid'
import { columnIcon } from './typeIcons'
import UiAlert from './ui/UiAlert.vue'
import UiStatusBar from './ui/UiStatusBar.vue'

const props = defineProps<{
  /**
   * Result tab shown, absent without any tab.
   */
  tab: QueryTab | undefined
  /**
   * Ids of the tabs of the cell: the configuration of a closed tab is
   * forgotten.
   */
  tabIds: number[]
  /**
   * Is the panel unfolded?
   */
  open: boolean
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
 * Configuration of a tab, with the columns it was made for.
 */
interface TabConfig {
  /**
   * Names of the columns of the result, when the configuration was made.
   */
  columns: string[]
  /**
   * Configuration.
   */
  config: PivotRequest
}

/**
 * Configuration of each tab, by id. Shallow: a configuration goes to the
 * Worker as it is, and a reactive copy could not.
 */
const configs = shallowReactive(new Map<number, TabConfig>())

// A closed tab takes its configuration away.
watch(
  () => props.tabIds,
  (ids) => {
    for (const id of configs.keys()) {
      if (!ids.includes(id)) {
        configs.delete(id)
      }
    }
  },
)

/**
 * Configuration without any element, shared by the tabs without one.
 */
const NO_CONFIG = emptyConfig()

/**
 * Configuration of the tab shown: none when the columns of its result have
 * changed.
 */
const config = computed<PivotRequest>({
  get() {
    const entry = props.tab ? configs.get(props.tab.id) : undefined
    return entry && result.value && sameColumns(entry.columns, result.value.columns)
      ? entry.config
      : NO_CONFIG
  },
  set(value) {
    if (props.tab && result.value) {
      configs.set(props.tab.id, { columns: result.value.columns, config: value })
    }
  },
})

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
 * Table shown, with the configuration it was computed for.
 */
const shownTable = shallowRef<{ table: PivotTable; request: PivotRequest } | null>(null)

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
 * Message of a refusal of the engine, in French.
 *
 * @param refusal Refusal of the engine.
 * @param request Configuration refused.
 */
function refusalText(refusal: PivotFailure, request: PivotRequest): string {
  const names = result.value?.columns ?? []
  const column = refusal.column === undefined ? '?' : (names[refusal.column] ?? '?')
  const row = refusal.row === undefined ? '?' : (refusal.row + 1).toLocaleString('fr-FR')
  const measure =
    refusal.measure === undefined || request.measures[refusal.measure] === undefined
      ? '?'
      : measureLabel(request.measures[refusal.measure]!, names)
  switch (refusal.kind) {
    case 'invalid_number':
      return `La valeur de la ligne ${row} de « ${column} » n’est pas un nombre.`
    case 'too_large':
      return `La valeur de la ligne ${row} de « ${column} » dépasse 38 chiffres.`
    case 'overflow':
      return `${measure} dépasse 38 chiffres.`
    case 'not_numeric':
      return `${measure} demande une colonne de nombres.`
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
  const request = config.value
  computing.value = true
  try {
    const table = await current.compute(request)
    if (computation === computations) {
      shownTable.value = { table, request }
      failure.value = null
    }
  } catch (error) {
    if (computation === computations && current === client) {
      failure.value =
        error instanceof PivotError ? refusalText(error.failure, request) : errorMessage(error)
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
  if (isEmptyConfig(config.value)) {
    computations++
    computing.value = false
    shownTable.value = null
    failure.value = null
    return
  }
  sendKept(client, shown)
  void compute(client)
}

watch([source, config, () => props.tab?.filter], update, { immediate: true })

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
 * Label of each measure of the table shown.
 */
const measureLabels = computed(() =>
  (shownTable.value?.request.measures ?? []).map((measure) =>
    measureLabel(measure, result.value?.columns ?? []),
  ),
)

/**
 * Names of the levels of the lines of the table shown.
 */
const rowNames = computed(() =>
  (shownTable.value?.request.rows ?? []).map((column) => result.value?.columns[column] ?? '?'),
)

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
  <section class="flex min-h-0 min-w-0 flex-1 flex-col gap-2 px-4 pt-1 pb-4" aria-label="Pivoteur">
    <panel-header title="Pivoteur" side="bottom" @collapse="emit('collapse')" />
    <p v-if="!analysable" class="text-fg-secondary">Rien à analyser</p>
    <p v-else-if="receiving" role="status" class="flex items-center gap-2 text-fg-secondary">
      <LoaderCircle class="size-5 animate-spin" aria-hidden="true" />
      En attente de la fin de la requête…
    </p>
    <div v-else class="flex min-h-0 flex-1 gap-4">
      <pivot-zones
        v-model:config="config"
        :names="result?.columns ?? []"
        :types="result?.columnTypes ?? []"
        :marks="marks"
      />
      <div class="flex min-h-0 min-w-0 flex-1 flex-col gap-2">
        <ui-alert v-if="failure" tone="error" :title="failure" />
        <pivot-grid
          v-if="shownTable"
          :table="shownTable.table.id"
          :shape="shownTable.table.shape"
          :measure-labels="measureLabels"
          :row-names="rowNames"
          :read="readWindow"
        />
        <p
          v-else-if="isEmptyConfig(config)"
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
    </div>
  </section>
</template>
