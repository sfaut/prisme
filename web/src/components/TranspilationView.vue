<!--
  Report of the transpilation of a query written in a dialect other than the
  native SQL of the server: a summary line (dialects, number of steps and of
  errors), which unfolds the transpiled query, then each step and each error,
  with its kind, its message and the passage of the query it concerns.

  Folded by default; unfolded when the query cannot be transpiled, or with
  `open`. The messages come from the server, in English, like its errors.
-->
<script setup lang="ts">
import { ChevronRight, Languages } from 'lucide-vue-next'
import { computed } from 'vue'

import type { ReportStepKind } from '@/api/generated/ReportStepKind'
import type { TextSpan } from '@/api/generated/TextSpan'
import type { TranspilationReport } from '@/api/generated/TranspilationReport'

import { choiceLabel } from './dialectChoice'

const props = defineProps<{
  /**
   * Report received from the server.
   */
  report: TranspilationReport
  /**
   * Text of the query as written, which the positions of the report refer to.
   */
  sql: string
  /**
   * Unfolded from the start.
   */
  open?: boolean
}>()

/**
 * Longest passage quoted, in characters.
 */
const PASSAGE_MAX = 60

/**
 * Names of the kinds of steps.
 */
const KINDS: Record<ReportStepKind, string> = {
  rewrite: 'Réécriture',
  emulation: 'Émulation',
  loss_of_precision: 'Perte de précision',
  omission: 'Omission',
}

/**
 * Summary: dialects, then the number of steps and of errors.
 */
const summary = computed(() => {
  const { steps, errors } = props.report
  const counts = [
    steps.length === 0 ? 'sans différence' : `${steps.length} étape${steps.length > 1 ? 's' : ''}`,
    errors.length === 0 ? null : `${errors.length} erreur${errors.length > 1 ? 's' : ''}`,
  ].filter((part) => part !== null)
  return `${choiceLabel(props.report.input)} → ${choiceLabel(props.report.output)} · ${counts.join(' · ')}`
})

/**
 * Passage of the query designated by a position, shortened; `null` without
 * a position (the whole query).
 *
 * @param span Position in the query, in UTF-16 units.
 */
function passage(span: TextSpan | null): string | null {
  if (span === null) {
    return null
  }
  const text = props.sql.slice(span.start, span.end).replace(/\s+/g, ' ')
  return text.length > PASSAGE_MAX ? `${text.slice(0, PASSAGE_MAX)}…` : text
}
</script>

<template>
  <details
    class="group rounded-control border border-line text-ui"
    :open="open || report.errors.length > 0"
  >
    <summary
      class="flex h-control cursor-pointer items-center gap-1.5 px-control-x text-fg-muted select-none hover:text-fg-strong"
      title="Requête transpilée vers le dialecte du serveur"
    >
      <ChevronRight class="size-icon-sm shrink-0 transition-transform group-open:rotate-90" />
      <Languages class="size-icon shrink-0" />
      <span class="truncate text-trim">Transpilée : {{ summary }}</span>
    </summary>
    <div
      class="flex max-h-(--transpilation-max-height) flex-col gap-gap overflow-auto border-t border-line p-control-x"
    >
      <pre v-if="report.sql" class="font-mono break-words whitespace-pre-wrap text-fg-strong">{{
        report.sql
      }}</pre>
      <ul v-if="report.steps.length > 0 || report.errors.length > 0" class="flex flex-col gap-1">
        <li
          v-for="(error, index) in report.errors"
          :key="`error-${index}`"
          class="flex flex-wrap items-baseline gap-x-1.5 text-error"
        >
          <span class="font-medium">Erreur</span>
          <span v-if="error.rule" class="font-mono text-fg-dimmed">{{ error.rule }}</span>
          <span>— {{ error.message }}</span>
          <code v-if="passage(error.span)" class="font-mono">« {{ passage(error.span) }} »</code>
        </li>
        <li
          v-for="(step, index) in report.steps"
          :key="`step-${index}`"
          class="flex flex-wrap items-baseline gap-x-1.5 text-fg"
        >
          <span class="font-medium">{{ KINDS[step.kind] }}</span>
          <span class="font-mono text-fg-dimmed">{{ step.rule }}</span>
          <span>— {{ step.message }}</span>
          <code v-if="passage(step.span)" class="font-mono text-fg-muted">
            « {{ passage(step.span) }} »
          </code>
        </li>
      </ul>
    </div>
  </details>
</template>
