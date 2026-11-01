<!--
  Pivot table of the Pivoteur (`PivotPanel.vue`), windowed on both axes
  (N-05): only the lines and the columns of the visible area, with a margin,
  exist in the page, read from the Worker by windows (`read`). The
  headers of the column levels stay on top when the table scrolls, the
  labels of the lines on the left (C-10). The label of a group of columns
  stands at its start, and stays in sight while one of its columns is.

  The lines are in compact form (C-07): one column of labels, indented by
  depth, the subtotal of a group on its line, the grand total last. Each
  line has the height of a row of the result grid, each column of values
  the same width (`--pivot-column-width`): the scroll position gives the
  lines and the columns to draw (`drawnRange`). A cell without any row
  stays empty, a measure that is NULL reads NULL, in the style of the grid
  (C-05). The layout of the headers is in `pivotLayout.ts`.
-->
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, useTemplateRef, watch } from 'vue'

import { cssPixels } from '@/cssLength'

import type { PivotShape, PivotWindow } from './pivotEngine'
import { hasMeasureRow, headerCells, headerRowCount, lineLabel } from './pivotLayout'
import { NULL_TEXT, drawnRange } from './resultGrid'

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
   * Reads a window of the table: lines and columns, start included, end
   * excluded.
   */
  read: (lines: [number, number], columns: [number, number]) => Promise<PivotWindow>
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
 * Window to draw: that of the table shown only.
 */
const content = computed(() => (shown.value?.table === props.table ? shown.value : null))

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
 * Label of the single measure, on top of the corner, when no row of the
 * headers names it and the corner has room for two lines: the zone
 * Mesures names it otherwise.
 */
const cornerMeasure = computed(() =>
  props.measureLabels.length === 1 &&
  !hasMeasureRow(props.shape.columnLevels, 1) &&
  headerRows.value >= 2
    ? props.measureLabels[0]
    : null,
)

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
        <span v-if="cornerMeasure" class="truncate text-fg-secondary">{{ cornerMeasure }}</span>
        <span class="truncate">{{ rowNames.join(' / ') }}</span>
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
          {{ lineLabel(line) ?? NULL_TEXT }}
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
.pivot-label,
.pivot-cell,
.pivot-head-text {
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
  align-items: center;
  border-block-end: var(--line-width) solid var(--color-line);
  border-inline-start: var(--line-width) solid var(--color-line);
  font-weight: 600;

  &.total {
    color: var(--color-fg-strong);
  }

  &.measure {
    justify-content: flex-end;
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

/* Corner: the single measure on top, the names of the levels of the lines
   at the bottom; on the left of the headers when they scroll sideways. */
.pivot-corner {
  position: sticky;
  inset-inline-start: 0;
  z-index: 1;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  gap: --spacing(1);
  padding: --spacing(1);
  border-block-end: var(--line-width) solid var(--color-line);
  border-inline-end: var(--line-width) solid var(--color-line);
  background: var(--color-surface-raised);
  font-weight: 600;
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
   pass under them. */
.pivot-label {
  position: sticky;
  inset-inline-start: 0;
  z-index: 1;
  padding-inline-start: calc(--spacing(1) + --spacing(4) * var(--indent));
  border-block-end: var(--line-width) solid var(--color-line-faint);
  border-inline-end: var(--line-width) solid var(--color-line);
  background: var(--color-surface);
}

.pivot-cell {
  border-block-end: var(--line-width) solid var(--color-line-faint);
  border-inline-start: var(--line-width) solid var(--color-line-faint);
  text-align: end;
}
</style>
