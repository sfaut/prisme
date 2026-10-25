<!--
  Compact result grid.

  - Sort: ▲▼ button of the header (ascending, descending, none).
  - User filter: text searched in all the columns (global filter) or in one
    column (targeted filter). The form is collapsed; its button, a funnel
    as in the console, expands it (and collapses it: the user filter stays
    applied, the button shows it in the accent color).
  - Buttons: the content of the `actions` slot (refresh of the result),
    then the user filter button. For the result tab shown, they go to the
    bar of the result tabs (`actionsTarget`, by a `Teleport` of Vue);
    otherwise, they stay at the left of the bottom bar.
  - Bottom bar: number of loaded rows ("120 / 5 000 lignes" when a user
    filter is active), then the content of the `footer` slot (duration of
    the query). The data explorer opens from its rail, on the right edge of
    the window.
  - Data explorer: the grid on screen (`displayed`) gives the panel on the
    right the selected values of a JSON or XML column (`dataExplorer`
    store): a column of type `JSON`, or a text column that holds JSON or
    XML.
  - Row numbers: they stay visible on the left when the grid scrolls
    sideways, as the header stays on top. Their column looks like the gutter
    of the SQL editor (code font, size, padding, alignment) and takes the
    width of the largest number.
  - Selection: click on a cell, on a row number or on a column name;
    Shift+click extends, Ctrl+click adds, dragging extends too (on the row
    numbers, dragging selects a group of rows). A click on the top left
    corner selects everything.
  - Copy: Ctrl+C copies the selection with the column names on the first
    line (a profile setting may remove them). Ctrl+A selects everything,
    Escape clears the selection.
  - Width: dragging the right edge of a header changes the width of the
    column; a double click on this edge fits it to its longest value (with a
    cap), a second one gives back its estimated width. The chosen widths are
    found again when the same query is run again
    (`gridWidths.ts`). A text too long is truncated (…): widening its column
    shows it in full.
  - Order: dragging a column name moves it. A new run restores the order of
    the `SELECT`. Selection and copy follow the displayed order; sort, user filter
    and widths stay attached to the column.
  - Alignment: numbers, dates and times on the right, after the type
    announced by the server.
  - Types: each header shows the icon of the family of its column's type
    (`typeIcons.ts`); the native name of the type (`VARCHAR(20)`,
    `DECIMAL(10,2)`...) shows on hover. A text column that holds JSON or XML
    shows the icon of this format (`contentKind.ts`).
  - Links: in a column of web addresses, each address is a link, preceded
    by an external link icon and opened in a new tab.

  Only the visible rows exist in the page (virtual scrolling): the grid
  stays smooth with tens of thousands of rows. The logic (sort, user filter,
  selection, copy) is in `resultGrid.ts`.
-->
<script setup lang="ts">
import { ExternalLink, Funnel, Search, X } from '@lucide/vue'
import { useVirtualizer } from '@tanstack/vue-virtual'
import {
  computed,
  nextTick,
  onBeforeUnmount,
  reactive,
  ref,
  shallowRef,
  useTemplateRef,
  watch,
} from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { Cell } from '@/api/querySession'
import { cssPixels } from '@/cssLength'
import { type ExplorerContent, useDataExplorerStore } from '@/stores/dataExplorer'

import {
  type UserFilter,
  type SelectMode,
  type SortState,
  MIN_COLUMN_WIDTH,
  MOVE_THRESHOLD,
  NULL_TEXT,
  dropPosition,
  emptySelection,
  estimatedWidth,
  fittedWidth,
  moveColumn,
  isFixedLengthText,
  isSelected,
  nextSort,
  resizedWidth,
  selectAll,
  selectCell,
  selectColumn,
  selectRow,
  selectionToText,
  selectedColumnCells,
  createViewMemory,
  sameColumns,
  updateView,
} from './resultGrid'
import { rememberWidths, rememberedWidths } from './gridWidths'
import { type ContentKind, detectContent, isWebAddress } from './contentKind'
import { columnIcon } from './typeIcons'
import UiButton from './ui/UiButton.vue'
import UiInput from './ui/UiInput.vue'
import UiSelect from './ui/UiSelect.vue'
import { notify } from './ui/toasts'
import { vTooltip } from './ui/tooltip'

const props = defineProps<{
  /**
   * Names of the columns.
   */
  columns: string[]
  /**
   * For each column, are its values right-aligned (numbers, dates and
   * times)?
   */
  rightAligned: boolean[]
  /**
   * Type of each column; empty if unknown (headers without a type).
   */
  columnTypes: ColumnTypeInfo[]
  /**
   * Rows of the result; the array can grow during the reception, without
   * being reactive: `rowCount` signals the new rows.
   */
  rows: Cell[][]
  /**
   * Number of rows received.
   */
  rowCount: number
  /**
   * Number of rows shown in the bottom bar, refreshed less often than
   * `rowCount` during the reception; when absent, `rowCount`.
   */
  shownCount?: number
  /**
   * Text of the query: the chosen widths are remembered for it, and found
   * again if it is run again.
   */
  sql: string
  /**
   * Is the grid on screen (active result tab of the active editor)? Only
   * this grid feeds the data explorer.
   */
  displayed?: boolean
  /**
   * Does the grid come right under a line (that of the result tabs)? Its
   * frame then leaves out its top border while the filter is closed, so as
   * not to double that line.
   */
  attached?: boolean
  /**
   * Where the buttons of the grid go (refresh, filter): the bar of the
   * result tabs, for the tab shown. Absent, they stay at the left of the
   * bottom bar.
   */
  actionsTarget?: HTMLElement | null
}>()

/**
 * Height of a row, in pixels, for virtual scrolling (`--grid-row-height` of
 * the theme): the virtualizer reads the scroll position in pixels to know
 * which rows to draw. The page places the rows and sizes the body with the
 * theme's variable itself, in CSS.
 */
const ROW_HEIGHT = cssPixels('--grid-row-height', 22)

const scroller = useTemplateRef<HTMLDivElement>('scroller')

const userFilter = reactive<UserFilter>({ text: '', column: null })

/**
 * Is the user filter form expanded?
 */
const userFilterOpen = ref(false)

const userFilterBox = useTemplateRef<HTMLDivElement>('userFilterBox')

/**
 * Label of the user filter button: what a click does, and whether the
 * filter keeps only some rows.
 */
const userFilterLabel = computed(
  () =>
    `${userFilterOpen.value ? 'Masquer' : 'Afficher'} le filtre${userFilter.text === '' ? '' : ' (filtre actif)'}`,
)

/**
 * Expands or collapses the user filter form; when expanded, its field takes the focus.
 */
async function toggleUserFilter(): Promise<void> {
  userFilterOpen.value = !userFilterOpen.value
  if (userFilterOpen.value) {
    await nextTick()
    userFilterBox.value?.querySelector('input')?.focus()
  }
}

/**
 * Number of rows of the bottom bar: loaded rows, preceded by the kept rows
 * when a user filter is active.
 */
const countLabel = computed(() => {
  const loaded = props.shownCount ?? props.rowCount
  const total = `${loaded.toLocaleString('fr-FR')} ${loaded > 1 ? 'lignes' : 'ligne'}`
  return userFilter.text === '' ? total : `${view.value.length.toLocaleString('fr-FR')} / ${total}`
})
const sort = ref<SortState | null>(null)
const selection = shallowRef(emptySelection())

/**
 * Mouse drag in progress: range of cells, or group of rows (started on a
 * row number); `null` without a drag.
 */
let dragging: 'cells' | 'rows' | null = null

/**
 * Gives the keyboard focus to the grid, so that Ctrl+C, Ctrl+A and Escape
 * apply to it. The selection clicks prevent the default behavior (selection
 * of the page's text), which would also have given this focus.
 */
function focusGrid(): void {
  scroller.value?.focus({ preventScroll: true })
}

/**
 * Starts a drag, which ends when the button is released.
 *
 * @param kind What the drag selects.
 */
function startDragging(kind: 'cells' | 'rows'): void {
  dragging = kind
  window.addEventListener('mouseup', () => (dragging = null), { once: true })
}

/**
 * Memory of the view, kept from one update to the next (see `updateView`).
 */
const viewMemory = createViewMemory()

/**
 * Rows examined per frame when the view is computed: bounded, so that a large
 * result (a session resumed after a reconnection, for example) does not
 * freeze the interface.
 */
const VIEW_CHUNK = 2000

/**
 * View: indexes of the source rows, filtered and sorted. It covers the rows
 * examined so far (see `refreshView`).
 */
const view = shallowRef<number[]>([])

/**
 * Number of the last view computation: a newer one stops the previous.
 */
let viewRun = 0

/**
 * Waits for the next frame of the display.
 */
function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()))
}

/**
 * Computes the view of the rows, one chunk per frame: the first chunk at once,
 * then the others while the browser draws. A newer computation (new rows,
 * user filter or sort) stops this one.
 */
async function refreshView(): Promise<void> {
  const run = ++viewRun
  const rows = props.rows
  for (;;) {
    view.value = updateView(viewMemory, rows, userFilter, sort.value, VIEW_CHUNK)
    if (viewMemory.scanned >= rows.length || run !== viewRun) {
      return
    }
    await nextFrame()
  }
}

watch(
  () => [props.rows, props.rowCount, userFilter.text, userFilter.column, sort.value],
  refreshView,
  {
    immediate: true,
  },
)

onBeforeUnmount(() => {
  viewRun++
})

/**
 * Widths chosen with the mouse, by column index; the other columns keep
 * their estimated width.
 */
const resized = ref<Record<number, string>>(rememberedWidths(props.sql, props.columns))

// New columns: the widths remembered for the query, the order of the SELECT,
// and no sort. The targeted filter's column index would point to another
// column, so the user filter becomes global; its text is kept. The same columns
// again (a session resumed after a reconnection) keep everything, as chosen by
// the user.
watch(
  () => props.columns,
  (columns, previous) => {
    if (previous !== undefined && sameColumns(columns, previous)) {
      return
    }
    resized.value = rememberedWidths(props.sql, columns)
    order.value = columns.map((_, index) => index)
    sort.value = null
    userFilter.column = null
  },
)

// Every chosen width is remembered for the query.
watch(resized, (widths) => rememberWidths(props.sql, props.columns, widths))

/**
 * Display order: index in the `SELECT` of each displayed column. The
 * displayed positions serve the selection; the indexes of the `SELECT`
 * serve the sort, the user filter and the widths.
 */
const order = ref(props.columns.map((_, index) => index))

/**
 * Estimated widths of the columns, on the header and the first rows.
 */
const estimated = computed(() => {
  // Read so that the widths recompute when new rows arrive.
  void props.rowCount
  return props.columns.map((_, column) => estimatedWidth(headerText(column), props.rows, column))
})

/**
 * Recognized content of each text column (JSON or XML), after its first
 * values; `null` for the other columns.
 */
const contentKinds = computed<(ContentKind | null)[]>(() => {
  // Read so that detection runs again when new rows arrive.
  void props.rowCount
  return props.columns.map((_, column) =>
    props.columnTypes[column]?.family === 'text' ? detectContent(props.rows, column) : null,
  )
})

/**
 * For each column, are its values shown in monospace (`CHAR`)?
 */
const monospaced = computed(() =>
  props.columns.map((_, column) => isFixedLengthText(props.columnTypes[column])),
)

/**
 * Mark of each column in its header (icon or text), absent without a type.
 */
const columnMarks = computed(() =>
  props.columns.map((_, column) => {
    const type = props.columnTypes[column]
    return type ? columnIcon(type, contentKinds.value[column] ?? null) : undefined
  }),
)

/**
 * Label of the recognized content of a column, for hovering its header.
 */
const CONTENT_LABELS: Record<ContentKind, string> = {
  json: 'contenu JSON',
  xml: 'contenu XML',
  url: 'adresses web',
}

/**
 * Hover text of a column header: name, native type and recognized content.
 *
 * @param column Index of the column.
 */
function headerTitle(column: number): string {
  const type = props.columnTypes[column]
  const content = contentKinds.value[column]
  const details = [type?.name, content ? CONTENT_LABELS[content] : undefined].filter(Boolean)
  const name = props.columns[column] ?? ''
  return `${details.length > 0 ? `${name} ${details.join(', ')}` : name} (glisser pour déplacer)`
}

/**
 * Text of a column header, to estimate its width: its name, plus the room
 * of the type mark (icon, or text such as "123").
 *
 * @param column Index of the column.
 */
function headerText(column: number): string {
  const name = props.columns[column] ?? ''
  const mark = columnMarks.value[column]
  if (mark === undefined) {
    return name
  }
  return `${name}${' '.repeat(typeof mark === 'string' ? mark.length + 1 : 2)}`
}

/**
 * Column template of the grid: row numbers, as wide as their content, then
 * each column, at its chosen or estimated width (CSS lengths relative to
 * the root or to the grid's font).
 */
const template = computed(() =>
  [
    'max-content',
    ...order.value.map(
      (column) => resized.value[column] ?? estimated.value[column] ?? MIN_COLUMN_WIDTH,
    ),
  ].join(' '),
)

/**
 * Number of digits of the largest row number: the row numbers and the
 * corner take its width (`--row-digits`), so that their column keeps the
 * same width in every row, as the gutter of CodeMirror does.
 */
const rowDigits = computed(() => String(props.rowCount).length)

/**
 * Resizing in progress: column, starting position of the pointer and
 * starting width.
 */
let resizing: { column: number; x: number; width: number; rootSize: number } | null = null

/**
 * Starts resizing a column; the pointer stays captured even outside the
 * handle.
 *
 * @param event Press on the handle.
 * @param column Index of the column.
 */
function onResizeStart(event: PointerEvent, column: number): void {
  const handle = event.currentTarget as HTMLElement
  handle.setPointerCapture(event.pointerId)
  // The drag starts from the width drawn, in pixels like the pointer.
  const width = handle.parentElement?.getBoundingClientRect().width ?? 0
  const rootSize = Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16
  resizing = { column, x: event.clientX, width, rootSize }
}

/**
 * Follows the pointer during the resizing.
 *
 * @param event Pointer move.
 */
function onResizeMove(event: PointerEvent): void {
  if (resizing !== null) {
    resized.value = {
      ...resized.value,
      [resizing.column]: resizedWidth(
        resizing.width,
        event.clientX - resizing.x,
        resizing.rootSize,
      ),
    }
  }
}

/**
 * Ends the resizing.
 */
function onResizeEnd(): void {
  resizing = null
}

/**
 * Fits a column to its longest value, within a cap; if it already is, gives
 * back its estimated width.
 *
 * @param column Index of the column.
 */
function fitWidth(column: number): void {
  const width = fittedWidth(headerText(column), props.rows, column)
  if (resized.value[column] === width) {
    const others = { ...resized.value }
    delete others[column]
    resized.value = others
  } else {
    resized.value = { ...resized.value, [column]: width }
  }
}

const userFilterOptions = computed(() => [
  { label: 'Toutes les colonnes', value: -1 },
  ...props.columns.map((name, index) => ({ label: name, value: index })),
])

const virtualizer = useVirtualizer(
  computed(() => ({
    count: view.value.length,
    getScrollElement: () => scroller.value,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  })),
)

// Sort or user filter changed: the positions change, the selection no longer makes sense.
watch([userFilter, sort], () => {
  selection.value = emptySelection()
})

/**
 * Selection mode after the keys held during the click.
 *
 * @param event Click.
 */
function modeOf(event: MouseEvent): SelectMode {
  if (event.shiftKey) {
    return 'extend'
  }
  return event.ctrlKey || event.metaKey ? 'add' : 'replace'
}

/**
 * Start of a cell selection (and of a possible drag).
 */
function onCellDown(event: MouseEvent, row: number, column: number): void {
  selection.value = selectCell(selection.value, row, column, modeOf(event))
  focusGrid()
  startDragging('cells')
}

/**
 * Drag over a cell: the range extends up to it, or the group of rows up to
 * its row.
 */
function onCellEnter(row: number, column: number): void {
  if (dragging === 'cells') {
    selection.value = selectCell(selection.value, row, column, 'extend')
  } else if (dragging === 'rows') {
    onRowEnter(row)
  }
}

/**
 * Click on a row number: selection of the whole row, and start of a
 * possible drag over other rows.
 */
function onRowHeader(event: MouseEvent, row: number): void {
  selection.value = selectRow(selection.value, row, props.columns.length, modeOf(event))
  focusGrid()
  startDragging('rows')
}

/**
 * Drag over a row number: the group of rows extends up to it.
 */
function onRowEnter(row: number): void {
  if (dragging === 'rows') {
    selection.value = selectRow(selection.value, row, props.columns.length, 'extend')
  }
}

/**
 * Column move in progress: displayed position of the grabbed column,
 * starting abscissa of the pointer, and has the drag passed the threshold?
 */
let moving: { from: number; x: number; started: boolean } | null = null

/**
 * Position where the moved column would be inserted, `null` outside a move.
 */
const dropAt = ref<number | null>(null)

/**
 * Press on a column name: selection of the whole column, and start of a
 * possible move.
 *
 * @param event Press on the name.
 * @param position Displayed position of the column.
 */
function onColumnHeader(event: PointerEvent, position: number): void {
  selection.value = selectColumn(selection.value, position, view.value.length, modeOf(event))
  focusGrid()
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  moving = { from: position, x: event.clientX, started: false }
}

/**
 * Follows the pointer during a column move.
 *
 * @param event Pointer move.
 */
function onColumnMove(event: PointerEvent): void {
  if (moving === null) {
    return
  }
  moving.started ||= Math.abs(event.clientX - moving.x) > MOVE_THRESHOLD
  if (moving.started) {
    dropAt.value = dropPosition(headerCenters(), event.clientX)
  }
}

/**
 * Release: the column takes its new place and stays selected.
 */
function onColumnDrop(): void {
  if (moving?.started && dropAt.value !== null) {
    const moved = moveColumn(order.value, moving.from, dropAt.value)
    order.value = moved.order
    selection.value = selectColumn(emptySelection(), moved.position, view.value.length, 'replace')
  }
  moving = null
  dropAt.value = null
}

/**
 * Abscissa of the middle of each displayed column header, from left to right.
 */
function headerCenters(): number[] {
  const headers = scroller.value?.querySelectorAll('.grid-column-header') ?? []
  return Array.from(headers, (header) => {
    const rect = header.getBoundingClientRect()
    return rect.left + rect.width / 2
  })
}

/**
 * Cell of a view row.
 *
 * @param row Position of the row in the view.
 * @param column Index of the column in the `SELECT`.
 */
function cellAt(row: number, column: number): Cell {
  const source = view.value[row]
  return source === undefined ? null : (props.rows[source]?.[column] ?? null)
}

/**
 * Changes the sort of a column: ascending, descending, none.
 */
function toggleSort(column: number): void {
  sort.value = nextSort(sort.value, column)
}

/**
 * Copies the selection to the clipboard.
 *
 * @param withHeaders Adds the column names on the first line.
 */
async function copy(withHeaders: boolean): Promise<void> {
  if (selection.value.ranges.length === 0) {
    return
  }
  const text = selectionToText(
    selection.value,
    view.value,
    props.rows,
    props.columns,
    withHeaders,
    order.value,
  )
  try {
    await navigator.clipboard.writeText(text)
    notify('Sélection copiée.')
  } catch {
    notify('Copie refusée par le navigateur.', 'error')
  }
}

/**
 * Selects all the rows and all the columns (Ctrl+A, click on the corner).
 */
function selectEverything(): void {
  selection.value = selectAll(view.value.length, props.columns.length)
  focusGrid()
}

/**
 * Keyboard shortcuts of the grid.
 */
function onKeydown(event: KeyboardEvent): void {
  const command = event.ctrlKey || event.metaKey
  if (command && event.key.toLowerCase() === 'c') {
    event.preventDefault()
    // With the headers, always for now: a setting of the user's profile may
    // remove them.
    void copy(true)
  } else if (command && event.key.toLowerCase() === 'a') {
    event.preventDefault()
    selectEverything()
  } else if (event.key === 'Escape') {
    selection.value = emptySelection()
  }
}

const explorer = useDataExplorerStore()

/**
 * Identity of the grid with the data explorer.
 */
const gridId = Symbol('result grid')

/**
 * What the selection gives the data explorer.
 */
function explorerContent(): ExplorerContent {
  const selected = selectedColumnCells(selection.value, view.value, props.rows, order.value)
  if (selected === null) {
    return { state: 'none' }
  }
  if (selected === 'several') {
    return { state: 'several' }
  }
  const column = props.columns[selected.column] ?? ''
  const kind =
    props.columnTypes[selected.column]?.family === 'json'
      ? 'json'
      : contentKinds.value[selected.column]
  return kind === 'json' || kind === 'xml'
    ? { state: 'values', column, kind, cells: selected.cells }
    : { state: 'scalar', column }
}

// The grid on screen keeps the open explorer up to date: selection, new rows
// (a sorted view moves them), detected content. Off screen, it withdraws.
watch(
  [() => props.displayed, () => explorer.open, selection, view, contentKinds],
  () => {
    if (!props.displayed) {
      explorer.release(gridId)
    } else if (explorer.open) {
      explorer.show(gridId, explorerContent())
    }
  },
  { immediate: true },
)

onBeforeUnmount(() => explorer.release(gridId))

/**
 * Value of the targeted column list: -1 for the global filter, all the columns.
 */
const userFilterColumn = computed({
  get: () => userFilter.column ?? -1,
  set: (value: number | undefined) => {
    userFilter.column = value === undefined || value < 0 ? null : value
  },
})
</script>

<template>
  <div class="result-grid">
    <div v-if="userFilterOpen" ref="userFilterBox" class="grid-toolbar">
      <ui-input
        v-model="userFilter.text"
        :icon="Search"
        placeholder="Filtrer"
        aria-label="Texte cherché"
        class="grid-user-filter"
      >
        <template v-if="userFilter.text !== ''" #trailing>
          <ui-button
            :icon="X"
            variant="ghost"
            small
            aria-label="Vider le filtre"
            @click="userFilter.text = ''"
          />
        </template>
      </ui-input>
      <ui-select
        v-model="userFilterColumn"
        :items="userFilterOptions"
        label="Colonne filtrée"
        class="grid-user-filter-column"
      />
    </div>

    <div
      ref="scroller"
      class="grid-scroller"
      :class="{ attached: attached && !userFilterOpen }"
      :style="{ '--row-digits': rowDigits }"
      tabindex="0"
      @keydown="onKeydown"
    >
      <div class="grid-row grid-header" :style="{ gridTemplateColumns: template }">
        <div
          class="grid-corner"
          v-tooltip="'Tout sélectionner'"
          @mousedown.prevent="selectEverything"
        />
        <div
          v-for="(column, position) in order"
          :key="column"
          class="grid-column-header"
          :class="{
            selected: view.length > 0 && isSelected(selection, 0, position),
            'drop-before': dropAt === position,
            'drop-after': dropAt === order.length && position === order.length - 1,
          }"
        >
          <span
            class="grid-column-name"
            v-tooltip="headerTitle(column)"
            @pointerdown.prevent="onColumnHeader($event, position)"
            @pointermove="onColumnMove"
            @pointerup="onColumnDrop"
            @pointercancel="onColumnDrop"
          >
            <template v-if="columnMarks[column]">
              <span
                v-if="typeof columnMarks[column] === 'string'"
                class="grid-type-glyph"
                aria-hidden="true"
              >
                {{ columnMarks[column] }}
              </span>
              <component
                :is="columnMarks[column]"
                v-else
                class="grid-type-icon"
                aria-hidden="true"
              />
            </template>
            <span class="grid-column-label">{{ columns[column] }}</span>
          </span>
          <button
            class="grid-sort"
            type="button"
            v-tooltip="`Trier par ${columns[column]}`"
            @click="toggleSort(column)"
          >
            {{ sort?.column === column ? (sort.direction === 'asc' ? '▲' : '▼') : '⇅' }}
          </button>
          <div
            class="grid-resizer"
            v-tooltip="
              'Glisser pour changer la largeur, double clic pour l’ajuster au contenu ou revenir à la largeur estimée'
            "
            @pointerdown.stop.prevent="onResizeStart($event, column)"
            @pointermove="onResizeMove"
            @pointerup="onResizeEnd"
            @pointercancel="onResizeEnd"
            @dblclick="fitWidth(column)"
          />
        </div>
      </div>

      <div
        class="grid-body"
        :style="{ blockSize: `calc(var(--grid-row-height) * ${virtualizer.options.count})` }"
      >
        <div
          v-for="item in virtualizer.getVirtualItems()"
          :key="item.index"
          class="grid-row"
          :style="{
            gridTemplateColumns: template,
            transform: `translateY(calc(var(--grid-row-height) * ${item.index}))`,
          }"
        >
          <div
            class="grid-row-header"
            @mousedown.prevent="onRowHeader($event, item.index)"
            @mouseenter="onRowEnter(item.index)"
          >
            {{ item.index + 1 }}
          </div>
          <div
            v-for="(column, position) in order"
            :key="column"
            class="grid-cell"
            :class="{
              null: cellAt(item.index, column) === null,
              right: rightAligned[column],
              mono: monospaced[column],
              selected: isSelected(selection, item.index, position),
            }"
            @mousedown.prevent="onCellDown($event, item.index, position)"
            @mouseenter="onCellEnter(item.index, position)"
          >
            <a
              v-if="contentKinds[column] === 'url' && isWebAddress(cellAt(item.index, column))"
              class="grid-link"
              :href="cellAt(item.index, column) ?? undefined"
              target="_blank"
              rel="noopener noreferrer"
              @mousedown.stop
              ><ExternalLink class="grid-link-icon" aria-hidden="true" /><span
                class="grid-link-text"
                >{{ cellAt(item.index, column) }}</span
              ></a
            >
            <template v-else>{{ cellAt(item.index, column) ?? NULL_TEXT }}</template>
          </div>
        </div>
      </div>
    </div>

    <div class="grid-footer">
      <!--
        Actions of the result (refresh), then the user filter button: in the
        bar of the result tabs for the tab shown, here otherwise.
      -->
      <teleport :to="actionsTarget" :disabled="!actionsTarget">
        <slot name="actions" />
        <ui-button
          :icon="Funnel"
          variant="ghost"
          :tone="userFilter.text === '' ? 'neutral' : 'accent'"
          small
          :class="{ 'bg-surface-elevated': userFilterOpen }"
          :aria-pressed="userFilterOpen"
          :aria-label="userFilterLabel"
          v-tooltip="userFilterLabel"
          class="grid-user-filter-toggle"
          @click="toggleUserFilter"
        />
      </teleport>
      <!-- The slot follows the number in the same text, without the container's gap. -->
      <span class="grid-count">{{ countLabel }}<slot name="footer" /></span>
    </div>
  </div>
</template>

<style scoped>
@reference "@/assets/main.css";

.result-grid {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: --spacing(2);
  min-height: 0;
}

/* A little room under the line of the result tabs, which takes the place
   of the frame's top border when the toolbar is closed. */
.grid-toolbar {
  display: flex;
  align-items: center;
  gap: --spacing(2);
  padding-block-start: --spacing(1);
}

.grid-user-filter {
  max-inline-size: --spacing(60);
}

.grid-user-filter-column {
  max-inline-size: --spacing(50);
}

/* Status of the result, discreet: in the small size, at the bottom of the
   panel, which clips what goes past it. Its line keeps its full height, the
   descenders included (no `text-box` trim). */
.grid-footer {
  display: flex;
  align-items: center;
  gap: --spacing(2);
  color: var(--fg-muted);
  font-size: var(--font-size-small);
}

.grid-scroller {
  position: relative;
  flex: 1;
  min-block-size: --spacing(30);
  /* Safeguard: a scroll area taller than the window would draw all the rows
     and freeze the interface. */
  max-block-size: 100vh;
  overflow: auto;
  border: var(--line-width) solid var(--line);
  font-size: var(--font-size-ui);
  user-select: none;

  &.attached {
    border-block-start-width: 0;
  }
}

.grid-row {
  display: grid;
  block-size: var(--grid-row-height);
}

/*
 * Cells, headers and row numbers: the text is centered on its capitals
 * (`text-box`) in the middle of the row (`align-content` on a block).
 */
.grid-cell,
.grid-row-header,
.grid-column-name {
  align-content: center;
  text-box: trim-both cap alphabetic;
}

.grid-header {
  position: sticky;
  inset-block-start: 0;
  z-index: 1;
  inline-size: max-content;
  min-inline-size: 100%;
  background: var(--surface-elevated);
}

.grid-body {
  position: relative;

  & .grid-row {
    position: absolute;
    inset-block-start: 0;
    inset-inline-start: 0;
    inline-size: max-content;
    min-inline-size: 100%;
  }
}

/* Row numbers and corner stay on the left when the grid scrolls sideways,
   above the cells and the headers (and their resize handles) that pass
   under them. They look like the gutter of the SQL editor (`editorTheme.ts`):
   code font, its size, one unit on each side, on the right. Each one is as
   wide as the largest number (`--row-digits` digits of the code font, whose
   digits all have the width of `1ch`), and has room for `--gutter-digits`
   digits at least, like the editor's gutter: the column, as wide as its
   content, keeps the same width in every row. */
.grid-corner,
.grid-row-header {
  position: sticky;
  inset-inline-start: 0;
  z-index: 2;
  padding-inline: --spacing(1);
  border-inline-end: var(--line-width) solid var(--line);
  background: var(--surface-elevated);
  color: var(--fg-dimmed);
  font-family: var(--font-code);
  font-size: var(--gutter-font-size);
  text-align: end;
  inline-size: calc(
    max(var(--gutter-digits), var(--row-digits)) * 1ch + --spacing(2) + var(--line-width)
  );
}

.grid-row-header {
  cursor: e-resize;
}

.grid-corner {
  cursor: se-resize;
}

/* The first column is closed on the left by the border of the row numbers. */
.grid-corner + .grid-column-header,
.grid-row-header + .grid-cell {
  border-inline-start: none;
}

/* The bold goes on the headers, not on their row: the widths in `em` of the
   template are read with the row's font, which must be that of the body's
   rows for the columns to line up. */
.grid-column-header {
  position: relative;
  display: flex;
  min-inline-size: 0;
  border-inline-start: var(--line-width) solid var(--line);
  font-weight: 600;

  /* Only the last column carries a right border: the others are closed by
     the left border of their neighbour. */
  &:last-child {
    border-inline-end: var(--line-width) solid var(--line);
  }

  &.drop-before {
    box-shadow: inset var(--marker-width) 0 0 var(--accent);
  }

  &.drop-after {
    box-shadow: inset calc(-1 * var(--marker-width)) 0 0 var(--accent);
  }
}

.grid-resizer {
  position: absolute;
  inset-block: 0;
  inset-inline-end: calc(var(--resize-handle-width) / -2);
  z-index: 1;
  inline-size: var(--resize-handle-width);
  cursor: col-resize;
  touch-action: none;

  &:hover {
    background: var(--resize-hover);
  }
}

.grid-column-name {
  display: flex;
  flex: 1;
  gap: --spacing(1);
  min-inline-size: 0;
  padding-inline: --spacing(1);
  white-space: nowrap;
  cursor: grab;
}

/* Neutral icon of the type's family, before the name. */
.grid-type-icon {
  flex: none;
  align-self: center;
  inline-size: var(--icon-size-sm);
  block-size: var(--icon-size-sm);
  color: var(--fg-dimmed);
}

/* Text mark (123, .00), as tall as the icons. */
.grid-type-glyph {
  flex: none;
  align-content: center;
  color: var(--fg-dimmed);
  font-family: var(--font-code);
  font-size: var(--font-size-small);
  font-weight: 500;
  letter-spacing: -0.05em;
  text-box: trim-both cap alphabetic;
}

/*
 * The name takes the whole height of the header and centers its capitals
 * there (`align-content`): cut to its cap height, it would lose its
 * descenders ("q", "_").
 */
.grid-column-label {
  flex: 0 1 auto;
  overflow: hidden;
  align-content: center;
  text-overflow: ellipsis;
  text-box: trim-both cap alphabetic;
}

.grid-sort {
  flex: none;
  padding-inline: --spacing(1);
  border: none;
  background: none;
  color: var(--fg-dimmed);
  font-size: var(--font-size-small);
  cursor: pointer;

  &:hover {
    color: var(--fg-strong);
  }
}

.grid-cell {
  padding-inline: --spacing(1);
  overflow: hidden;
  border-block-end: var(--line-width) solid var(--line-muted);
  border-inline-start: var(--line-width) solid var(--line-muted);
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: cell;

  &:last-child {
    border-inline-end: var(--line-width) solid var(--line-muted);
  }

  &.right {
    text-align: end;
  }

  /* Fixed-length text (CHAR): codes and references, in monospace. */
  &.mono {
    font-family: var(--font-code);
  }

  /* NULL in small capitals: more discreet than a value of the column. */
  &.null {
    color: var(--fg-dimmed);
    font-style: italic;
    font-variant-caps: all-small-caps;
  }
}

.selected {
  background: var(--selection);
}

/* Web address of a column of links. */
/* Link: the icon, then the address, cut with an ellipsis when the column is
   too narrow. */
.grid-link {
  display: inline-flex;
  align-items: center;
  gap: --spacing(1);
  max-inline-size: 100%;
  vertical-align: top;
  color: var(--accent);
  text-decoration: none;
  cursor: pointer;

  &:hover {
    text-decoration: underline;
  }
}

/* External link icon, before the address. */
.grid-link-icon {
  flex-shrink: 0;
  inline-size: var(--icon-size-sm);
  block-size: var(--icon-size-sm);
}

.grid-link-text {
  min-inline-size: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
