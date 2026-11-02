<!--
  Pivot table of the Pivoteur (`PivotPanel.vue`), windowed on both axes
  (N-05): only the lines and the columns of the visible area, with a margin,
  exist in the page, read from the Worker by windows (`read`). The
  headers of the column levels stay on top when the table scrolls, the
  labels of the lines on the left (C-10). The label of a group of columns
  stands at its start, and stays in sight while one of its columns is. The
  corner names the levels: each level of the columns at the end of its
  row, against its values, the levels of the lines at the start of the
  last row, above their labels.

  The lines are in compact form (C-07): one column of labels, indented by
  depth, the subtotal of a group on its line, the grand total last. Each
  line has the height of a row of the result grid, each column of values
  the same width (`--pivot-column-width`): the scroll position gives the
  lines and the columns to draw (`drawnRange`). A cell without any row
  stays empty, a measure that is NULL reads NULL, in the style of the grid
  (C-05). The layout of the headers is in `pivotLayout.ts`.

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

import { cssPixels } from '@/cssLength'

import type { PivotAxis } from './pivotConfig'
import { sameTarget } from './pivotConfig'
import type {
  PivotLine,
  PivotOrder,
  PivotShape,
  PivotValueTarget,
  PivotWindow,
} from './pivotEngine'
import {
  cornerRows,
  headerCells,
  headerRowCount,
  lineLabel,
  sameLayout,
  sortMark,
} from './pivotLayout'
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
}>()

/**
 * Height of a line and width of a column of values or of the labels, in
 * pixels, to turn the scroll position into the lines and the columns to
 * draw.
 *
 * Fixed sizes: read once (`docs/DEVELOPER.md`, section "Lengths in
 * pixels").
 */
const LINE_HEIGHT = cssPixels('--grid-row-height', 22)
const COLUMN_WIDTH = cssPixels('--pivot-column-width', 108)
const LABEL_WIDTH = cssPixels('--pivot-label-width', 180)

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
  drawnRange(
    scrollLeft.value,
    viewportWidth.value - (labelled.value ? LABEL_WIDTH : 0),
    COLUMN_WIDTH,
    props.shape.columns,
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
 * Columns of the grid of each line and of the headers: the labels, the room
 * of the columns before the window, then the columns of the window.
 */
const template = computed(() => {
  const tracks = labelled.value ? ['var(--pivot-label-width)'] : []
  tracks.push(`calc(var(--pivot-column-width) * ${content.value?.firstColumn ?? 0})`)
  const count = content.value?.content.columns.length ?? 0
  if (count > 0) {
    tracks.push(`repeat(${count}, var(--pivot-column-width))`)
  }
  return tracks.join(' ')
})

/**
 * Width of the whole table.
 */
const width = computed(
  () =>
    `calc(${labelled.value ? 'var(--pivot-label-width) + ' : ''}var(--pivot-column-width) * ${props.shape.columns})`,
)

/**
 * First grid column of the window, after the labels and the room before
 * it.
 */
const firstTrack = computed(() => (labelled.value ? 3 : 2))

/**
 * Header cells of the columns of the window.
 */
const headers = computed(() =>
  content.value === null
    ? []
    : headerCells(content.value.content.columns, props.shape.columnLevels, props.measureLabels),
)

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
</script>

<template>
  <div
    ref="scroller"
    class="pivot-scroller"
    :class="{ labelled }"
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
          <span class="pivot-corner-names">
            <span v-for="{ level, name } in row.rows" :key="level" class="pivot-corner-name">
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
      </div>
      <div
        v-for="cell in headers"
        :key="`${cell.row}-${cell.column}`"
        class="pivot-head"
        :class="[cell.kind, { null: cell.text === null }]"
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
          :class="{ null: lineLabel(line) === null }"
          :style="{ '--indent': Math.max(0, line.depth - 1) }"
        >
          <span class="pivot-label-text">{{ lineLabel(line) ?? NULL_TEXT }}</span>
          <button
            v-if="columnsSortable"
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
.pivot-label,
.pivot-cell {
  padding-inline: --spacing(1);

  /* NULL in small capitals, as in the grid. */
  &.null {
    color: var(--color-fg-faint);
    font-style: italic;
    font-variant-caps: all-small-caps;
  }
}

.pivot-header {
  position: sticky;
  inset-block-start: 0;
  z-index: 2;
  display: grid;
  min-inline-size: 100%;
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

  .labelled & {
    inset-inline-start: calc(var(--pivot-label-width) + --spacing(1));
  }
}

/* Corner: a row facing each row of the headers (`subgrid`), with the
   names of the levels; on the left of the headers when they scroll
   sideways. */
.pivot-corner {
  position: sticky;
  inset-inline-start: 0;
  z-index: 1;
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

/* Labels of the lines, indented by depth, on the left of the cells that
   pass under them; the sort button shows with the pointer on the line. */
.pivot-label {
  position: sticky;
  inset-inline-start: 0;
  z-index: 1;
  display: flex;
  justify-content: space-between;
  gap: --spacing(1);
  min-inline-size: 0;
  padding-inline-start: calc(--spacing(1) + --spacing(4) * var(--indent));
  border-block-end: var(--line-width) solid var(--color-line-faint);
  border-inline-end: var(--line-width) solid var(--color-line);
  background: var(--color-surface);

  .pivot-sort:not(.active, :focus-visible) {
    opacity: 0;
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
