<!--
  Pivot table of the Pivoteur (`PivotPanel.vue`), windowed on both axes
  (N-05): only the lines and the columns of the visible area, with a margin,
  exist in the page, read from the Worker by windows (`read`). The
  headers of the column levels stay on top when the table scrolls, the
  labels of the lines on the left (C-10), unless their columns take more
  than half of the visible width: they then scroll with the values, which
  stay within reach. The label of a group of columns stands at its start,
  and stays in sight while one of its columns is. The corner names the
  levels: each level of the columns at the end of its row, against its
  values, the levels of the lines at the start of the last row, above their
  labels.

  The form of the lines (`layout`) gives their labels: in compact form, one
  column, indented by depth; in outline and tabular forms, one column per
  level (`labelCells`). The grand total comes last. Each
  line has the height of a row of the result grid, each column its default
  width (`--pivot-column-width` for the values) or the one given to it: the
  scroll position gives the lines and the columns to draw (`drawnRange`,
  `columnRange` in `pivotWidths.ts`). A cell without any row
  stays empty, a measure that is NULL reads NULL, in the style of the grid
  (C-05). The layout of the headers is in `pivotLayout.ts`.

  Resize handles, as in the grid: the right edge of the header of a column
  of values, on the last row of the headers, and that of each column of
  labels, in the corner. A drag gives a width, which the panel saves at the
  release; a double click fits the column to the texts drawn, a second one
  gives back its default width. A width names its column by its values: the
  engine gives the position of those known with the table (`positions`),
  the grid learns that of a column resized since. Nothing is computed again.

  Sort buttons, as in the grid: the name of a level inverts its direction
  (C-11), the header of a single column ranks the lines by its values, the
  label of a line ranks the columns by its own, by their first measure (C-13).
  The button of a line shows when the pointer is on that line, or when it
  ranks the columns. The panel holds the order (`order`), and computes nothing
  to lay the table out again. While the new window is read, the previous one
  stays in place when it fits.
-->
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, useTemplateRef, watch } from 'vue'

import type { PivotColumnWidth } from '@/api/generated/PivotColumnWidth'
import { cssPixels } from '@/cssLength'

import type { PivotAxis } from './pivotConfig'
import { sameTarget } from './pivotConfig'
import type {
  PivotLayout,
  PivotLine,
  PivotOrder,
  PivotShape,
  PivotValueTarget,
  PivotWindow,
} from './pivotEngine'
import {
  type HeaderCell,
  cornerRows,
  headerCells,
  headerRowCount,
  labelCells,
  labelColumnCount,
  sameLayout,
  sortMark,
} from './pivotLayout'
import {
  type ColumnGeometry,
  type WidthBounds,
  columnRange,
  draggedSteps,
  fittedSteps,
  roomBefore,
  roomLength,
  stepsLength,
  targetKey,
  widenedColumns,
  windowTracks,
} from './pivotWidths'
import { NULL_TEXT, drawnRange } from './resultGrid'
import { vTooltip } from './ui/tooltip'

const props = defineProps<{
  /**
   * Id of the table shown: a new table reads its windows again.
   */
  table: number
  /**
   * Shape of the table.
   */
  shape: PivotShape
  /**
   * Label of each measure, in the order of its zone.
   */
  measureLabels: string[]
  /**
   * Names of the levels of the lines, the outermost first.
   */
  rowNames: string[]
  /**
   * Names of the levels of the columns, the outermost first.
   */
  columnNames: string[]
  /**
   * Order of the lines and of the columns.
   */
  order: PivotOrder
  /**
   * Layout of the table: the form of its lines and its totals.
   */
  layout: PivotLayout
  /**
   * Do the lines repeat the labels of their groups?
   */
  repeatLabels: boolean
  /**
   * Widths given to columns of values, named by their values.
   */
  columnWidths: PivotColumnWidth[]
  /**
   * Position in the table of the columns given a width, by their key (`targetKey`), as the
   * engine gave them with the table.
   */
  positions: ReadonlyMap<string, number>
  /**
   * Width of the labels in a single column, in steps of the spacing scale; the default width
   * without one.
   */
  labelsWidth?: number | undefined
  /**
   * Width of the column of labels of each level of the lines, in steps of the spacing scale;
   * the default width without one.
   */
  levelWidths: (number | undefined)[]
  /**
   * Reads a window of the table: lines and columns, start included, end
   * excluded.
   */
  read: (lines: [number, number], columns: [number, number]) => Promise<PivotWindow>
}>()

const emit = defineEmits<{
  /**
   * The user clicks the sort of a level.
   */
  sortLevel: [axis: PivotAxis, level: number]
  /**
   * The user clicks the sort of a node, whose values rank the other axis.
   */
  sortValues: [axis: PivotAxis, target: PivotValueTarget]
  /**
   * The user gives a column of values a width, in steps of the spacing scale; `null` for its
   * default width.
   */
  resizeColumn: [target: PivotValueTarget, width: number | null]
  /**
   * The user gives the labels a width: those of a level of the lines, or `null` for their
   * single column.
   */
  resizeLabels: [level: number | null, width: number | null]
}>()

/**
 * Height of a line, and width of a column of values, of the labels in one
 * column and of a column of labels per level, in pixels, to turn the scroll
 * position into the lines and the columns to draw.
 *
 * Fixed sizes: read once (`docs/DEVELOPER.md`, section "Lengths in
 * pixels").
 */
const LINE_HEIGHT = cssPixels('--grid-row-height', 22)
const COLUMN_WIDTH = cssPixels('--pivot-column-width', 108)
const LABEL_WIDTH = cssPixels('--pivot-label-width', 180)
const LEVEL_WIDTH = cssPixels('--pivot-level-width', 120)

/**
 * Width of a step of the spacing scale, in pixels, and bounds of a width given to a column, in
 * steps: the smallest width and the cap of a width fitted to the content, those of the result
 * grid.
 *
 * Fixed sizes: read once (`docs/DEVELOPER.md`, section "Lengths in pixels").
 */
const STEP = cssPixels('--spacing', 3)
const BOUNDS: WidthBounds = {
  step: STEP,
  min: Math.ceil(cssPixels('--grid-column-min-width', 39) / STEP),
  max: Math.floor(cssPixels('--grid-fitted-max-width', 720) / STEP),
}

/**
 * Tooltip of a resize handle.
 */
const RESIZE_TIP =
  'Glisser pour changer la largeur, double clic pour l’ajuster au contenu ou revenir à la largeur par défaut'

/**
 * Lines and columns drawn beyond each edge of the visible area.
 */
const LINE_OVERSCAN = 20
const COLUMN_OVERSCAN = 4

const scroller = useTemplateRef<HTMLElement>('scroller')

/**
 * Scroll position of the table, in pixels.
 */
const scrollTop = ref(0)
const scrollLeft = ref(0)

/**
 * Size of the visible area of the table, in pixels.
 */
const viewportHeight = ref(0)
const viewportWidth = ref(0)

/**
 * Follows the scroll position of the table.
 */
function onScroll(): void {
  scrollTop.value = scroller.value?.scrollTop ?? 0
  scrollLeft.value = scroller.value?.scrollLeft ?? 0
}

/**
 * Follows the size of the visible area: window resized, panel moved.
 */
let resizeObserver: ResizeObserver | null = null

onMounted(() => {
  resizeObserver = new ResizeObserver(() => {
    viewportHeight.value = scroller.value?.clientHeight ?? 0
    viewportWidth.value = scroller.value?.clientWidth ?? 0
  })
  if (scroller.value) {
    resizeObserver.observe(scroller.value)
  }
})

onBeforeUnmount(() => resizeObserver?.disconnect())

/**
 * Does the table have a column of labels? Not without any level: its line
 * then gives the value of each measure (F-31).
 */
const labelled = computed(() => props.shape.rowLevels > 0 || props.shape.columnLevels > 0)

/**
 * Number of columns of labels of a line.
 */
const labelColumns = computed(() => labelColumnCount(props.layout.form, props.shape.rowLevels))

/**
 * Column being resized: a column of values, by its position in the table and
 * by its values, or a column of labels, by its position among them.
 */
type Resized =
  { kind: 'column'; index: number; target: PivotValueTarget } | { kind: 'labels'; column: number }

/**
 * Drag of the edge of a column: the column, the position of the pointer and
 * the width of the column at the start, in pixels, and the width reached, in
 * steps, `null` until the pointer moves; `null` without any drag.
 */
const drag = shallowRef<{
  resized: Resized
  x: number
  start: number
  width: number | null
} | null>(null)

/**
 * Positions learned for the table shown: those of the columns resized since
 * it was computed, which the engine could not give with it.
 */
const learned = shallowRef<{ table: number; positions: Map<string, number> }>({
  table: 0,
  positions: new Map(),
})

/**
 * Position in the table of each column given a width, by its key.
 */
const positions = computed<ReadonlyMap<string, number>>(() =>
  learned.value.table === props.table
    ? new Map([...props.positions, ...learned.value.positions])
    : props.positions,
)

/**
 * Columns of values of the table, with the widths given to them, that of the
 * column being dragged included.
 */
const geometry = computed<ColumnGeometry>(() => {
  const widened = widenedColumns(props.columnWidths, positions.value)
  const dragged = drag.value
  if (dragged?.resized.kind === 'column' && dragged.width !== null) {
    const { index } = dragged.resized
    const others = widened.filter((column) => column.index !== index)
    widened.splice(0, widened.length, ...others, { index, width: dragged.width })
    widened.sort((a, b) => a.index - b.index)
  }
  return { count: props.shape.columns, standard: COLUMN_WIDTH, step: STEP, widened }
})

/**
 * Width given to each column of labels, in steps, that of the column being
 * dragged included; `undefined` for the default width.
 */
const labelWidths = computed(() => {
  const widths =
    labelColumns.value > 1
      ? Array.from({ length: labelColumns.value }, (_, level) => props.levelWidths[level])
      : [props.labelsWidth]
  const dragged = drag.value
  if (dragged?.resized.kind === 'labels' && dragged.width !== null) {
    widths[dragged.resized.column] = dragged.width
  }
  return widths
})

/**
 * Width of each column of labels, as a CSS length.
 */
const labelLengths = computed(() => {
  const standard = labelColumns.value > 1 ? 'var(--pivot-level-width)' : 'var(--pivot-label-width)'
  return labelWidths.value.map((width) => (width === undefined ? standard : stepsLength(width)))
})

/**
 * Width of each column of labels, in pixels.
 */
const labelPixels = computed(() => {
  const standard = labelColumns.value > 1 ? LEVEL_WIDTH : LABEL_WIDTH
  return labelWidths.value.map((width) => (width === undefined ? standard : width * STEP))
})

/**
 * Width of the labels of a line, as a CSS length.
 */
const labelsLength = computed(() =>
  labelLengths.value.length === 1
    ? (labelLengths.value[0] ?? '0px')
    : `calc(${labelLengths.value.join(' + ')})`,
)

/**
 * Width of the labels of a line in the table, in pixels.
 */
const labelsPixels = computed(() =>
  labelled.value ? labelPixels.value.reduce((sum, width) => sum + width, 0) : 0,
)

/**
 * Columns of the labels of a line: the last one takes what the sort button
 * leaves.
 */
const labelTracks = computed(() => [...labelLengths.value.slice(0, -1), 'minmax(0, 1fr)'].join(' '))

/**
 * Right edge of each column of labels, as a CSS length from the start of the
 * labels: the place of its resize handle.
 */
const labelEdges = computed(() =>
  labelLengths.value.map((_, column) =>
    column === labelLengths.value.length - 1
      ? labelsLength.value
      : `calc(${labelLengths.value.slice(0, column + 1).join(' + ')})`,
  ),
)

/**
 * Do the labels stay on the left when the table scrolls sideways? A single
 * column does; several as long as they leave the values half of the width.
 */
const frozen = computed(
  () =>
    labelled.value && (labelColumns.value === 1 || labelsPixels.value * 2 <= viewportWidth.value),
)

/**
 * Number of rows of the headers.
 */
const headerRows = computed(() =>
  headerRowCount(props.shape.columnLevels, props.measureLabels.length),
)

/**
 * Lines and columns to draw, after the scroll position: the headers cover
 * the top of the visible area, the labels its left.
 */
const drawnLines = computed(() =>
  drawnRange(
    scrollTop.value,
    viewportHeight.value - headerRows.value * LINE_HEIGHT,
    LINE_HEIGHT,
    props.shape.lines,
    LINE_OVERSCAN,
  ),
)
const drawnColumns = computed(() =>
  columnRange(
    geometry.value,
    frozen.value ? scrollLeft.value : Math.max(0, scrollLeft.value - labelsPixels.value),
    viewportWidth.value - (frozen.value ? labelsPixels.value : 0),
    COLUMN_OVERSCAN,
  ),
)

/**
 * Window shown: its table, its lines and columns, and its content.
 */
interface ShownWindow {
  /**
   * Id of the table.
   */
  table: number
  /**
   * Shape of the table.
   */
  shape: PivotShape
  /**
   * Number of the reading that gave it, which keeps the latest one.
   */
  reading: number
  /**
   * First line of the window.
   */
  firstLine: number
  /**
   * First column of the window.
   */
  firstColumn: number
  /**
   * Content of the window.
   */
  content: PivotWindow
}

const shown = shallowRef<ShownWindow | null>(null)

/**
 * Number of the last reading asked.
 */
let readings = 0

// The table or the drawn area changes: the window that covers it is read,
// unless the window shown covers it already. The answers come in order;
// the latest one of the table shown is kept.
watch(
  [() => props.table, drawnLines, drawnColumns],
  ([table, lines, columns]) => {
    const current = shown.value
    if (
      current !== null &&
      current.table === table &&
      current.firstLine <= lines.start &&
      current.firstLine + current.content.lines.length >= lines.end &&
      current.firstColumn <= columns.start &&
      current.firstColumn + current.content.columns.length >= columns.end
    ) {
      return
    }
    const reading = ++readings
    props
      .read([lines.start, lines.end], [columns.start, columns.end])
      .then((content) => {
        const latest = shown.value
        if (
          table === props.table &&
          (latest === null || latest.table !== table || latest.reading < reading)
        ) {
          shown.value = {
            table,
            shape: props.shape,
            reading,
            firstLine: lines.start,
            firstColumn: columns.start,
            content,
          }
        }
      })
      // A window refused (a table replaced since) gives way to the next one;
      // the panel reports the refusals of the engine.
      .catch(() => undefined)
  },
  { immediate: true },
)

/**
 * Window to draw: that of the table shown, or, while it is read, that of the
 * previous table when it takes the same room.
 */
const content = computed(() => {
  const window = shown.value
  return window !== null && (window.table === props.table || sameLayout(window.shape, props.shape))
    ? window
    : null
})

/**
 * Position in the table of the first column of the window drawn.
 */
const firstColumn = computed(() => content.value?.firstColumn ?? 0)

/**
 * Columns of the grid of each line and of the headers: the labels, the room
 * of the columns before the window, then the columns of the window.
 */
const template = computed(() => {
  const tracks = labelled.value ? [labelsLength.value] : []
  tracks.push(roomLength(roomBefore(geometry.value, firstColumn.value)))
  const count = content.value?.content.columns.length ?? 0
  tracks.push(...windowTracks(geometry.value.widened, firstColumn.value, count))
  return tracks.join(' ')
})

/**
 * Width of the whole table.
 */
const width = computed(() => {
  const columns = roomLength(roomBefore(geometry.value, props.shape.columns))
  return labelled.value ? `calc(${labelsLength.value} + ${columns})` : columns
})

/**
 * First grid column of the window, after the labels and the room before
 * it.
 */
const firstTrack = computed(() => (labelled.value ? 3 : 2))

/**
 * Header cells of the columns of the window, with the column each one
 * resizes: that of a cell of a single column on the last row of the headers.
 */
const headers = computed(() => {
  const columns = content.value?.content.columns ?? []
  return headerCells(columns, props.shape.columnLevels, props.measureLabels).map(
    (cell: HeaderCell) => {
      const column = columns[cell.column]
      const resizes = cell.span === 1 && cell.row + cell.rowSpan === headerRows.value
      const resized: Resized | null =
        resizes && column !== undefined
          ? {
              kind: 'column',
              index: firstColumn.value + cell.column,
              target: { path: column.path.map(([, value]) => value), measure: column.measure ?? 0 },
            }
          : null
      return { ...cell, resized }
    },
  )
})

/**
 * Rows of the corner, with the names of the levels.
 */
const corner = computed(() => cornerRows(headerRows.value, props.rowNames, props.columnNames))

/**
 * Lines of the window, with their position in the table.
 */
const lines = computed(() =>
  (content.value?.content.lines ?? []).map((line, offset) => ({
    index: (content.value?.firstLine ?? 0) + offset,
    line,
    cells: content.value?.content.cells[offset] ?? [],
  })),
)

/**
 * Can the values of a column rank the lines? With lines to rank.
 */
const linesSortable = computed(() => props.shape.rowLevels > 0)

/**
 * Can the values of a line rank the columns? With columns to rank, and a
 * measure to read.
 */
const columnsSortable = computed(
  () => props.shape.columnLevels > 0 && props.measureLabels.length > 0,
)

/**
 * Mark of the sort of a level: its direction, unless values rank its axis.
 *
 * @param axis Axis of the level.
 * @param level Position of the level.
 */
function levelMark(axis: PivotAxis, level: number): string {
  const { descending, values } = props.order[axis]
  return sortMark(values === null ? descending[level] === true : null)
}

/**
 * Direction of the sort by these values, `null` when they do not rank the axis.
 *
 * @param axis Axis the values rank.
 * @param target Node and measure of the values.
 */
function valueDirection(axis: PivotAxis, target: PivotValueTarget): boolean | null {
  const { values } = props.order[axis]
  return values !== null && sameTarget(values, target) ? values.descending : null
}

/**
 * Values of a line, as they can rank the columns: by their first measure.
 *
 * @param line Line of the table.
 */
function lineTarget(line: PivotLine): PivotValueTarget {
  return { path: line.path, measure: 0 }
}

/**
 * Width of a column, in pixels.
 *
 * @param resized Column.
 */
function columnPixels(resized: Resized): number {
  if (resized.kind === 'labels') {
    return labelPixels.value[resized.column] ?? 0
  }
  const widened = geometry.value.widened.find((column) => column.index === resized.index)
  return widened === undefined ? COLUMN_WIDTH : widened.width * STEP
}

/**
 * Width given to a column, in steps; `undefined` for the default width.
 *
 * @param resized Column.
 */
function givenWidth(resized: Resized): number | undefined {
  if (resized.kind === 'labels') {
    return labelColumns.value > 1 ? props.levelWidths[resized.column] : props.labelsWidth
  }
  const key = targetKey(resized.target)
  return props.columnWidths.find((column) => targetKey(column) === key)?.width
}

/**
 * Gives a column a width, through the panel, which saves it.
 *
 * @param resized Column.
 * @param width Width, in steps; `null` for the default width.
 */
function giveWidth(resized: Resized, width: number | null): void {
  if (resized.kind === 'labels') {
    emit('resizeLabels', labelColumns.value > 1 ? resized.column : null, width)
    return
  }
  // The width finds its column before the engine gives its position again.
  const known = learned.value.table === props.table ? learned.value.positions : []
  learned.value = {
    table: props.table,
    positions: new Map(known).set(targetKey(resized.target), resized.index),
  }
  emit('resizeColumn', resized.target, width)
}

/**
 * Starts the drag of the edge of a column.
 *
 * @param event Press on the handle.
 * @param resized Column.
 */
function onResizeStart(event: PointerEvent, resized: Resized): void {
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  drag.value = { resized, x: event.clientX, start: columnPixels(resized), width: null }
}

/**
 * Follows the pointer during the drag: the column takes the width reached.
 *
 * @param event Pointer move.
 */
function onResizeMove(event: PointerEvent): void {
  const dragged = drag.value
  if (dragged !== null) {
    const width = draggedSteps(dragged.start, event.clientX - dragged.x, BOUNDS)
    drag.value = { ...dragged, width }
  }
}

/**
 * Ends the drag: the column keeps the width reached, if the pointer moved.
 */
function onResizeEnd(): void {
  const dragged = drag.value
  if (dragged !== null && dragged.width !== null) {
    giveWidth(dragged.resized, dragged.width)
  }
  drag.value = null
}

/**
 * Drops the drag: the column keeps its width.
 */
function onResizeCancel(): void {
  drag.value = null
}

/**
 * Width an element of a column takes with its whole content, in pixels: that
 * of a copy beside it, out of the flow, at its widest.
 *
 * @param element Element of the column.
 */
function wholeWidth(element: HTMLElement): number {
  const copy = element.cloneNode(true) as HTMLElement
  copy.style.position = 'absolute'
  copy.style.visibility = 'hidden'
  copy.style.inlineSize = 'max-content'
  element.after(copy)
  const width = copy.getBoundingClientRect().width
  copy.remove()
  return width
}

/**
 * Elements drawn whose width follows that of a column: the cells and the
 * headers of a single column of values; the labels of a level and its name;
 * the labels of the lines and the corner when they stand in a single column.
 *
 * @param resized Column.
 */
function columnElements(resized: Resized): HTMLElement[] {
  const selector =
    resized.kind === 'column'
      ? `[data-column="${resized.index}"]`
      : labelColumns.value > 1
        ? `[data-level="${resized.column}"]`
        : '.pivot-label, .pivot-corner'
  return [...(scroller.value?.querySelectorAll<HTMLElement>(selector) ?? [])]
}

/**
 * Fits a column to the texts drawn, within the bounds; if it already is,
 * gives back its default width.
 *
 * @param resized Column.
 */
function fitWidth(resized: Resized): void {
  // Fixed sizes: the elements give pixels, the width keeps whole steps
  // (`docs/DEVELOPER.md`, section "Lengths in pixels").
  const current = columnPixels(resized)
  const needs = columnElements(resized).map(
    (element) => current + wholeWidth(element) - element.getBoundingClientRect().width,
  )
  const fitted = fittedSteps(needs, BOUNDS)
  giveWidth(resized, givenWidth(resized) === fitted ? null : fitted)
}

/**
 * Does a line show values? Not that of a group without its subtotal, whose
 * sort button would rank the columns by values out of sight.
 *
 * @param line Line of the table.
 */
function showsValues(line: PivotLine): boolean {
  return (
    line.depth === 0 ||
    line.depth >= props.shape.rowLevels ||
    props.layout.rows.subtotals[line.depth - 1] !== false
  )
}
</script>

<template>
  <div
    ref="scroller"
    class="pivot-scroller"
    :class="{ frozen }"
    :style="{ '--labels-width': labelsLength }"
    tabindex="0"
    @scroll.passive="onScroll"
  >
    <div
      class="pivot-header"
      :style="{
        inlineSize: width,
        gridTemplateColumns: template,
        gridTemplateRows: `repeat(${headerRows}, var(--grid-row-height))`,
      }"
    >
      <div
        v-if="labelled"
        class="pivot-corner"
        :style="{ gridRow: `1 / span ${headerRows}`, gridColumn: 1 }"
      >
        <div v-for="(row, index) in corner" :key="index" class="pivot-corner-row">
          <span
            class="pivot-corner-names"
            :class="{ levels: labelColumns > 1 }"
            :style="labelColumns > 1 ? { gridTemplateColumns: labelTracks } : undefined"
          >
            <span
              v-for="{ level, name } in row.rows"
              :key="level"
              class="pivot-corner-name"
              :data-level="level"
            >
              <span class="pivot-corner-text">{{ name }}</span>
              <button
                class="pivot-sort"
                :class="{ active: order.rows.descending[level] && order.rows.values === null }"
                type="button"
                :aria-label="`Trier par ${name}`"
                v-tooltip="`Trier par ${name}`"
                @click="emit('sortLevel', 'rows', level)"
              >
                {{ levelMark('rows', level) }}
              </button>
            </span>
          </span>
          <span v-if="row.column" class="pivot-corner-name">
            <span class="pivot-corner-text">{{ row.column.name }}</span>
            <button
              class="pivot-sort"
              :class="{
                active: order.columns.descending[row.column.level] && order.columns.values === null,
              }"
              type="button"
              :aria-label="`Trier par ${row.column.name}`"
              v-tooltip="`Trier par ${row.column.name}`"
              @click="emit('sortLevel', 'columns', row.column!.level)"
            >
              {{ levelMark('columns', row.column.level) }}
            </button>
          </span>
        </div>
        <div
          v-for="(edge, column) in labelEdges"
          :key="`resizer ${column}`"
          class="pivot-resizer"
          :style="{ insetInlineStart: `calc(${edge} - var(--resize-handle-width) / 2)` }"
          v-tooltip="RESIZE_TIP"
          @pointerdown.stop.prevent="onResizeStart($event, { kind: 'labels', column })"
          @pointermove="onResizeMove"
          @pointerup="onResizeEnd"
          @pointercancel="onResizeCancel"
          @dblclick="fitWidth({ kind: 'labels', column })"
        />
      </div>
      <div
        v-for="cell in headers"
        :key="`${cell.row}-${cell.column}`"
        class="pivot-head"
        :class="[cell.kind, { null: cell.text === null }]"
        :data-column="cell.span === 1 ? firstColumn + cell.column : undefined"
        :style="{
          gridRow: `${cell.row + 1} / span ${cell.rowSpan}`,
          gridColumn: `${firstTrack + cell.column} / span ${cell.span}`,
        }"
      >
        <span class="pivot-head-text">{{ cell.text ?? NULL_TEXT }}</span>
        <button
          v-if="cell.target && linesSortable"
          class="pivot-sort"
          :class="{ active: valueDirection('rows', cell.target) !== null }"
          type="button"
          aria-label="Trier les lignes par cette colonne"
          v-tooltip="'Trier les lignes par cette colonne'"
          @click="emit('sortValues', 'rows', cell.target!)"
        >
          {{ sortMark(valueDirection('rows', cell.target)) }}
        </button>
        <div
          v-if="cell.resized"
          class="pivot-resizer"
          v-tooltip="RESIZE_TIP"
          @pointerdown.stop.prevent="onResizeStart($event, cell.resized)"
          @pointermove="onResizeMove"
          @pointerup="onResizeEnd"
          @pointercancel="onResizeCancel"
          @dblclick="fitWidth(cell.resized)"
        />
      </div>
    </div>

    <div
      class="pivot-body"
      :style="{ inlineSize: width, blockSize: `calc(var(--grid-row-height) * ${shape.lines})` }"
    >
      <div
        v-for="{ index, line, cells } in lines"
        :key="index"
        class="pivot-line"
        :class="{ subtotal: line.depth < shape.rowLevels, 'grand-total': line.depth === 0 }"
        :style="{
          gridTemplateColumns: template,
          transform: `translateY(calc(var(--grid-row-height) * ${index}))`,
        }"
      >
        <div
          v-if="labelled"
          class="pivot-label"
          :style="{ '--indent': labelColumns > 1 ? 0 : Math.max(0, line.depth - 1) }"
        >
          <div class="pivot-label-cells" :style="{ gridTemplateColumns: labelTracks }">
            <span
              v-for="(text, level) in labelCells(line, shape.rowLevels, layout.form, repeatLabels)"
              :key="level"
              class="pivot-label-text"
              :class="{ null: text === null }"
              :data-level="level"
              >{{ text ?? NULL_TEXT }}</span
            >
          </div>
          <button
            v-if="columnsSortable && showsValues(line)"
            class="pivot-sort"
            :class="{ active: valueDirection('columns', lineTarget(line)) !== null }"
            type="button"
            aria-label="Trier les colonnes par cette ligne"
            v-tooltip="'Trier les colonnes par cette ligne'"
            @click="emit('sortValues', 'columns', lineTarget(line))"
          >
            {{ sortMark(valueDirection('columns', lineTarget(line))) }}
          </button>
        </div>
        <div class="pivot-room" />
        <div
          v-for="(value, column) in cells"
          :key="column"
          class="pivot-cell"
          :class="{ null: value === null }"
          :data-column="firstColumn + column"
        >
          {{ value === false ? '' : (value ?? NULL_TEXT) }}
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
@reference "@/assets/main.css";

.pivot-scroller {
  position: relative;
  flex: 1;
  min-block-size: 0;
  overflow: auto;
  border: var(--line-width) solid var(--color-line);
  font-size: var(--text-base);
  user-select: none;
}

/*
 * Cells, headers and labels: the text is centered on its capitals
 * (`text-box`) in the middle of the line (`align-content` on a block), cut
 * with an ellipsis when its column is too narrow.
 */
.pivot-label-text,
.pivot-cell,
.pivot-head-text,
.pivot-corner-text {
  overflow: hidden;
  align-content: center;
  text-overflow: ellipsis;
  text-box: trim-both cap alphabetic;
  white-space: nowrap;
}

.pivot-head,
.pivot-label-text,
.pivot-cell {
  padding-inline: --spacing(1);

  /* NULL in small capitals, as in the grid. */
  &.null {
    color: var(--color-fg-faint);
    font-style: italic;
    font-variant-caps: all-small-caps;
  }
}

/* The handle of the last column stays within the table. */
.pivot-header {
  position: sticky;
  inset-block-start: 0;
  z-index: 2;
  display: grid;
  min-inline-size: 100%;
  overflow-x: clip;
  background: var(--color-surface-raised);
}

/*
 * Headers of the columns: the label of a value or of a total at the
 * start of its group, that of a measure at the end, above the numbers.
 * The label of a group stays in sight (`sticky`) when its start leaves
 * the visible area, on the right of the labels of the lines; the cell
 * keeps its overflow visible, as a hidden one would hold the label
 * inside the cell.
 */
.pivot-head {
  position: relative;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: --spacing(1);
  border-block-end: var(--line-width) solid var(--color-line);
  border-inline-start: var(--line-width) solid var(--color-line);
  font-weight: 600;

  &.total {
    color: var(--color-fg-strong);
  }

  /* The label above the numbers, its sort button at the other end. */
  &.measure {
    flex-direction: row-reverse;
  }
}

/* Resize handle on the right edge of a column, as in the grid, under the
   corner; those of the columns of labels are placed by their edge. */
.pivot-resizer {
  position: absolute;
  inset-block: 0;
  inset-inline-end: calc(var(--resize-handle-width) / -2);
  z-index: 1;
  inline-size: var(--resize-handle-width);
  cursor: col-resize;
  touch-action: none;

  .pivot-corner > & {
    inset-inline-end: auto;
  }

  &:hover {
    background: var(--resize-hover);
  }
}

/* Sort button, as in the grid. */
.pivot-sort {
  flex: none;
  padding-inline: --spacing(1);
  border: none;
  background: none;
  color: var(--color-fg-faint);
  font-size: var(--text-sm);
  font-weight: 400;
  cursor: pointer;

  &:hover,
  &.active {
    color: var(--color-fg-strong);
  }
}

.pivot-head-text {
  position: sticky;
  inset-inline-start: --spacing(1);
  min-inline-size: 0;

  /* Clipped sideways only, for the ellipsis: trimmed to its capitals
     (`text-box`), the box would cut the descenders below. */
  overflow-x: clip;
  overflow-y: visible;

  .frozen & {
    inset-inline-start: calc(var(--labels-width) + --spacing(1));
  }
}

/* The corner and the labels stay on the left of what scrolls sideways. */
.frozen .pivot-corner,
.frozen .pivot-label {
  position: sticky;
  inset-inline-start: 0;
}

/* Corner: a row facing each row of the headers (`subgrid`), with the
   names of the levels, above the headers that pass under it. */
.pivot-corner {
  position: relative;
  z-index: 2;
  display: grid;
  grid-template-rows: subgrid;
  padding-inline: --spacing(1);
  border-block-end: var(--line-width) solid var(--color-line);
  border-inline-end: var(--line-width) solid var(--color-line);
  background: var(--color-surface-raised);
  font-weight: 600;
}

/* Names of the levels of the lines at the start of a row, that of a
   level of the columns at its end; each one shrinks with an ellipsis
   when the corner is too narrow for both, its sort button in sight. */
.pivot-corner-row {
  display: flex;
  justify-content: space-between;
  gap: --spacing(2);
  min-inline-size: 0;
}

.pivot-corner-names,
.pivot-corner-name {
  display: flex;
  min-inline-size: 0;
}

.pivot-corner-names {
  gap: --spacing(1);

  /* One per column of labels, above it; the name of a level of the columns
     takes the end of the last one. */
  &.levels {
    display: grid;
    flex: 1;
    gap: 0;
  }
}

.pivot-body {
  position: relative;
  min-inline-size: 100%;
}

.pivot-line {
  position: absolute;
  inset-block-start: 0;
  inset-inline-start: 0;
  display: grid;
  block-size: var(--grid-row-height);

  /* Group lines carry the subtotals; the grand total closes the table. */
  &.subtotal {
    font-weight: 600;
  }

  &.grand-total > * {
    border-block-start: var(--line-width) solid var(--color-line);
  }
}

/* Labels of the lines, on the left of the cells that pass under them; the
   sort button shows with the pointer on the line. */
.pivot-label {
  z-index: 1;
  display: flex;
  justify-content: space-between;
  min-inline-size: 0;
  padding-inline-end: --spacing(1);
  border-block-end: var(--line-width) solid var(--color-line-faint);
  border-inline-end: var(--line-width) solid var(--color-line);
  background: var(--color-surface);

  .pivot-sort:not(.active, :focus-visible) {
    opacity: 0;
  }
}

/* One column of labels indented by depth, or one per level, each with its
   line. */
.pivot-label-cells {
  display: grid;
  flex: 1;
  min-inline-size: 0;
}

.pivot-label-text {
  padding-inline-start: calc(--spacing(1) + --spacing(4) * var(--indent));

  & + & {
    border-inline-start: var(--line-width) solid var(--color-line-faint);
  }
}

.pivot-line:hover .pivot-sort {
  opacity: 1;
}

.pivot-cell {
  border-block-end: var(--line-width) solid var(--color-line-faint);
  border-inline-start: var(--line-width) solid var(--color-line-faint);
  text-align: end;
}
</style>
