<!--
  Line of a splitter of Prisme (`splitLayout.ts`), between
  two panels of a group: it resizes the panel it controls (`controls`), next
  to it, and the panel that fills the group takes the rest. The pointer grabs
  it a few steps around the line, and the line follows it, captured, until it
  is released; the size is kept then, as shown, without rounding. Keys, as the "window splitter" pattern
  of WAI-ARIA: the arrows move the line by a `rem`, Home and End give the
  panel its smallest and its largest size, Enter does what the double click
  does (`activate`: fold, unfold, or default width, as the parent decides).
  A grip marks the middle of the line, unless the parent leaves it out
  (`grip`).
-->
<script setup lang="ts">
import { computed, nextTick, ref, useTemplateRef } from 'vue'

import { vTooltip } from '../ui/tooltip'
import {
  type DragPanel,
  type DragResult,
  type DragStart,
  FAR,
  KEY_STEPS,
  type SplitPanelEntry,
  dragSizes,
  useSplitGroup,
} from './splitLayout'

const props = withDefaults(
  defineProps<{
    /**
     * Id of the panel the line resizes, next to it.
     */
    controls: string
    /**
     * Tooltip: what dragging and the double click do.
     */
    tooltip: string
    /**
     * Does the line show its grip, in its middle? Without it, only the band
     * of the hover shows where to grab it.
     */
    grip?: boolean
  }>(),
  { grip: true },
)

const emit = defineEmits<{
  /**
   * Double click, or Enter: the action of the line (fold, unfold...).
   */
  activate: []
}>()

const group = useSplitGroup('SplitHandle')

const handle = useTemplateRef<HTMLElement>('handle')

/**
 * Is the line followed by the pointer?
 */
const dragging = ref(false)

/**
 * Values of the line for screen readers: the size of its panel, its smallest
 * and its largest, in CSS pixels; read when the line gets the focus, and after
 * each move.
 */
const values = ref<{ now: number; min: number; max: number } | null>(null)

/**
 * Orientation of the line itself: vertical between panels side by side.
 */
const orientation = computed(() => (group.direction() === 'horizontal' ? 'vertical' : 'horizontal'))

/**
 * A drag or a key under way: the state of the group at its start, the panel
 * of the line, and the position of the pointer then.
 */
interface Move {
  start: DragStart
  target: SplitPanelEntry
  /**
   * Is the panel of the line before it? The line then grows it towards the
   * end of the group.
   */
  targetBefore: boolean
  pointer: number
  result: DragResult | null
}

let move: Move | null = null

/**
 * Length of an element along the group, in pixels.
 *
 * @param element Element.
 */
function length(element: HTMLElement): number {
  const box = element.getBoundingClientRect()
  return group.direction() === 'horizontal' ? box.width : box.height
}

/**
 * Smallest size of a panel along the group, in pixels.
 *
 * @param element Element of the panel.
 */
function minLength(element: HTMLElement): number {
  const property = group.direction() === 'horizontal' ? 'min-inline-size' : 'min-block-size'
  return Number.parseFloat(getComputedStyle(element).getPropertyValue(property)) || 0
}

/**
 * A panel as the computation of the drag sees it. Its smallest size comes
 * from the panel, not from its style, which a folded panel sets to 0.
 *
 * @param entry Panel.
 */
function dragPanel(entry: SplitPanelEntry): DragPanel {
  const element = entry.element()!
  const minSize = entry.minSize()
  const foldedSize = entry.foldedSize()
  return {
    size: length(element),
    min: minSize === undefined ? 0 : group.measure(minSize),
    folded: foldedSize === undefined ? null : group.measure(foldedSize),
    isFolded: entry.isFolded(),
  }
}

/**
 * State of the group as the line sees it now; `null` if its panel is missing.
 *
 * @param pointer Position of the pointer along the group.
 */
function begin(pointer: number): Move | null {
  const element = handle.value
  const panels = group.panels()
  const target = panels.find((panel) => panel.id() === props.controls)
  if (element === null || target === undefined) {
    return null
  }
  const before = (panel: SplitPanelEntry): boolean =>
    (panel.element()!.compareDocumentPosition(element) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0
  const targetBefore = before(target)
  const beyond = panels.filter((panel) => before(panel) !== targetBefore)
  if (!targetBefore) {
    beyond.reverse()
  }
  // The panel that fills gives its room; without one, the room left free.
  const fill = beyond.find((panel) => panel.isFill())
  const fillRoom =
    fill !== undefined
      ? length(fill.element()!) - minLength(fill.element()!)
      : panels.some((panel) => panel.isFill())
        ? 0
        : group.freeRoom()
  return {
    start: {
      target: dragPanel(target),
      fillRoom: Math.max(0, fillRoom),
    },
    target,
    targetBefore,
    pointer,
    result: null,
  }
}

/**
 * Shows the size of a move of the line, folding or unfolding its panel.
 *
 * @param current Move under way.
 * @param delta Move of the line towards the room of the panel that fills.
 */
function show(current: Move, delta: number): void {
  const result = dragSizes(current.start, delta)
  current.result = result
  if (result.folded !== current.target.isFolded()) {
    current.target.fold(result.folded)
  }
  current.target.showSize(result.size)
}

/**
 * Keeps the size shown, as it is, when the move changed it.
 *
 * @param current Move ended.
 */
function keep(current: Move): void {
  const { result, start, target } = current
  if (result === null) {
    return
  }
  if (result.size === null) {
    target.showSize(null)
  } else if (start.target.isFolded || Math.abs(result.size - start.target.size) > 1e-6) {
    target.keepSize(result.size)
  } else {
    target.showSize(null)
  }
  // Read once the page shows the size kept.
  void nextTick(readValues)
}

/**
 * Position of the pointer along the group.
 *
 * @param event Event of the pointer.
 */
function position(event: PointerEvent): number {
  return group.direction() === 'horizontal' ? event.clientX : event.clientY
}

/**
 * Starts following the pointer, on the main button.
 *
 * @param event Press of the pointer.
 */
function onPointerDown(event: PointerEvent): void {
  if (event.button !== 0 || !event.isPrimary) {
    return
  }
  move = begin(position(event))
  if (move === null) {
    return
  }
  // No selection of text, no focus: the drag is the pointer's alone.
  event.preventDefault()
  handle.value?.setPointerCapture(event.pointerId)
  dragging.value = true
  group.resizing(true)
}

/**
 * Moves the line with the pointer.
 *
 * @param event Move of the pointer.
 */
function onPointerMove(event: PointerEvent): void {
  if (!dragging.value || move === null) {
    return
  }
  const delta = (position(event) - move.pointer) * (move.targetBefore ? 1 : -1)
  show(move, delta)
}

/**
 * Ends the drag: the sizes shown are kept.
 */
function onPointerEnd(): void {
  if (!dragging.value || move === null) {
    return
  }
  dragging.value = false
  group.resizing(false)
  keep(move)
  move = null
}

/**
 * Moves the line by a key.
 *
 * @param event Key pressed on the line.
 */
function onKeyDown(event: KeyboardEvent): void {
  if (event.key === 'Enter') {
    event.preventDefault()
    emit('activate')
    return
  }
  const horizontal = group.direction() === 'horizontal'
  const forward = horizontal ? 'ArrowRight' : 'ArrowDown'
  const backward = horizontal ? 'ArrowLeft' : 'ArrowUp'
  if (![forward, backward, 'Home', 'End'].includes(event.key)) {
    return
  }
  event.preventDefault()
  const current = begin(0)
  if (current === null) {
    return
  }
  const { start } = current
  const from = start.target.isFolded ? (start.target.folded ?? 0) : start.target.size
  const sign = current.targetBefore ? 1 : -1
  const delta =
    event.key === 'Home'
      ? start.target.min - from
      : event.key === 'End'
        ? FAR
        : (event.key === forward ? sign : -sign) * KEY_STEPS * group.measure('var(--spacing)')
  show(current, delta)
  keep(current)
}

/**
 * Reads the values of the line for screen readers.
 */
function readValues(): void {
  const current = begin(0)
  if (current === null) {
    values.value = null
    return
  }
  const { start } = current
  const largest = dragSizes(start, FAR).size
  values.value = {
    now: Math.round(start.target.isFolded ? 0 : start.target.size),
    min: Math.round(start.target.min),
    max: Math.round(largest ?? start.target.min),
  }
}
</script>

<template>
  <div
    ref="handle"
    class="split-handle"
    role="separator"
    tabindex="0"
    :aria-orientation="orientation"
    :aria-controls="controls"
    :aria-valuenow="values?.now"
    :aria-valuemin="values?.min"
    :aria-valuemax="values?.max"
    :data-orientation="group.direction()"
    :data-dragging="dragging || undefined"
    :data-grip="grip ? undefined : 'none'"
    v-tooltip="tooltip"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="onPointerEnd"
    @pointercancel="onPointerEnd"
    @lostpointercapture="onPointerEnd"
    @dblclick="emit('activate')"
    @keydown="onKeyDown"
    @focus="readValues"
  >
    <span class="split-hit" aria-hidden="true" />
  </div>
</template>

<style scoped>
@reference "@/assets/main.css";

/* Drawing of the line, its states read from the page: hover and focus by the
   keyboard, drag by `data-dragging`. Above its neighbours: the band of the
   hover overlaps them. The line is the handle's border, never its background:
   the browser rounds the width of a border to whole pixels of the screen,
   where a box of one CSS pixel would cover one and a half at a scale of 150%
   and be painted one or two pixels thick, according to its position. */
.split-handle {
  position: relative;
  z-index: 1;
  flex: none;
  border: 0 solid var(--color-line);
  outline: none;
  touch-action: none;

  /* Band of the hover and grip: each one a box that covers exactly the line
     (a border as wide as it, in a box of no width set against the handle's
     inner edge), whose outline draws the band or the grip on both sides. The
     browser rounds the width of an outline to whole pixels of the screen, as
     that of a border, and draws it around the pixels of the line: the band
     and the grip stay centered on the line wherever the handle falls. Boxes
     placed around the line by a calc() had their edges rounded on their own,
     and slid by a pixel to a side at some positions. */
  &::before,
  &::after {
    position: absolute;
    border: 0 solid transparent;
    content: '';
  }

  /* One step on each side of the line: a band of two steps. */
  &::before {
    outline: --spacing(1) solid transparent;
  }

  /* Half the grip on each side of the line, and at each of its ends. */
  &::after {
    border-color: var(--grip-color);
    border-radius: var(--grip-thickness);
    outline: calc(var(--grip-thickness) / 2) solid var(--grip-color);
  }

  /* A line without a grip (`grip`). */
  &[data-grip='none']::after {
    content: none;
  }

  /* Box of the zone grabbed, the same margin on each side of the line:
     transparent, above the neighbours as the handle is, it takes the pointer
     from them, which then show no hover nor tooltip of their own there, and
     the handle follows the pointer from the press. Across the line, the box
     is the margin before it, a transparent border as wide as the line, then
     the margin after it: the border is rounded to whole pixels of the screen
     as the line is, so the box follows the line at any scale. The handle
     clips only along its line: the box overflows across it. */
  .split-hit {
    --hit-margin: var(--split-hit-margin);

    position: absolute;
    border: 0 solid transparent;

    @media (pointer: coarse) {
      --hit-margin: var(--split-hit-margin-coarse);
    }
  }

  &[data-orientation='horizontal'] {
    border-inline-start-width: var(--line-width);
    overflow-block: clip;
    cursor: col-resize;

    &::before,
    &::after {
      inset-inline-end: 0;
      inline-size: 0;
      border-inline-start-width: var(--line-width);
    }

    &::before {
      inset-block: 0;
    }

    &::after {
      inset-block-start: calc(50% - (var(--grip-length) - var(--grip-thickness)) / 2);
      block-size: calc(var(--grip-length) - var(--grip-thickness));
    }

    .split-hit {
      inset-block: 0;
      inset-inline-end: calc(-1 * var(--hit-margin));
      border-inline-start-width: var(--line-width);
      padding-inline: var(--hit-margin);
    }
  }

  &[data-orientation='vertical'] {
    border-block-start-width: var(--line-width);
    overflow-inline: clip;
    cursor: row-resize;

    &::before,
    &::after {
      inset-block-end: 0;
      block-size: 0;
      border-block-start-width: var(--line-width);
    }

    &::before {
      inset-inline: 0;
    }

    &::after {
      inset-inline-start: calc(50% - (var(--grip-length) - var(--grip-thickness)) / 2);
      inline-size: calc(var(--grip-length) - var(--grip-thickness));
    }

    .split-hit {
      inset-inline: 0;
      inset-block-end: calc(-1 * var(--hit-margin));
      border-block-start-width: var(--line-width);
      padding-block: var(--hit-margin);
    }
  }

  &:hover,
  &[data-dragging],
  &:focus-visible {
    &::before {
      border-color: var(--resize-hover);
      outline-color: var(--resize-hover);
    }

    &::after {
      border-color: var(--color-primary);
      outline-color: var(--color-primary);
    }
  }
}
</style>
