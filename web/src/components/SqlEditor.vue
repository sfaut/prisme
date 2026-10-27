<!--
  SQL editor, based on CodeMirror 6.

  Highlighting according to the dialect and the version of the server (MySQL or
  MariaDB: keywords read from the server, see `sqlDialect.ts`), line numbers
  alone in their gutter, like the row numbers of the grid (`sqlSetup.ts`:
  no fold arrows, folding stays on Ctrl+Shift+[ and Ctrl+Shift+]),
  light or dark theme like the interface (`editorTheme.ts`), undo history,
  completion of names only: databases, tables, columns, functions and
  procedures (namespace built from the catalog, see `sqlCompletion.ts`).
  Keywords (SELECT, UPDATE...) are not offered, and an option must start
  with the typed text (or take its initials): no scattered matching. The
  query that Ctrl+Enter would run is highlighted (`currentStatement.ts`)
  and syntax errors are underlined, with their message on hover: the text
  is analysed by the server shortly after each keystroke. Ctrl+Enter
  (Cmd+Enter on Mac) emits `run` with the cursor position;
  Ctrl+Shift+Enter emits `run-all` with the selected passages (none: all
  the queries); `selection` tells whether text is selected, for "Exécuter
  précisément". Tab and Shift+Tab indent by stops of `indentSize` spaces (a
  setting of the user), like Backspace in the indentation and Ctrl+[ /
  Ctrl+], Enter goes to a new line at the indentation of the line it leaves
  (`editorIndent.ts`). Ctrl+Up and Ctrl+Down scroll the text by one line
  without moving the cursor (`editorScroll.ts`). The blanks at the end
  of a line go when the cursor leaves it, except inside a string
  (`editorTrim.ts`). The text binds with `v-model`.
-->
<script setup lang="ts">
import { autocompletion } from '@codemirror/autocomplete'
import { LanguageSupport } from '@codemirror/language'
import { type Diagnostic, setDiagnostics } from '@codemirror/lint'
import { MariaSQL, MySQL, schemaCompletionSource } from '@codemirror/lang-sql'
import { type ChangeSpec, Compartment } from '@codemirror/state'
import { keymap } from '@codemirror/view'
import { EditorView } from 'codemirror'
import { computed, onBeforeUnmount, onMounted, shallowRef, useTemplateRef, watch } from 'vue'

import { analyzeSql } from '@/api/analyze'
import type { DialectKeywords } from '@/api/generated/DialectKeywords'
import type { Flavor } from '@/api/generated/Flavor'
import type { SelectionRange } from '@/api/generated/SelectionRange'
import { fetchKeywords } from '@/api/keywords'
import { describe } from '@/api/messages'
import type { CatalogSchema } from '@/stores/catalog'

import { currentStatement, setStatements } from './currentStatement'
import {
  dedentLines,
  editorIndentation,
  indentOrInsertSpaces,
  insertNewlineKeepingIndentation,
} from './editorIndent'
import { scrollKeymap } from './editorScroll'
import { editorTheme } from './editorTheme'
import { trimTrailingBlanks } from './editorTrim'
import { completionSchema } from './sqlCompletion'
import { editorDialect } from './sqlDialect'
import { sqlSetup } from './sqlSetup'

const props = defineProps<{
  /**
   * Dialect of the target server, for highlighting and keywords.
   */
  flavor: Flavor
  /**
   * Version of the target server, for its keywords; `null` while unknown.
   */
  version: string | null
  /**
   * Is the `ANSI_QUOTES` mode active on the session? Double quotes then
   * delimit identifiers (highlighting, completion).
   */
  ansiQuotes: boolean
  /**
   * Databases of the connection's catalog: their tables and columns are
   * offered by completion.
   */
  schemas: CatalogSchema[]
  /**
   * Current database: its tables complete without a prefix. `null` if none.
   */
  defaultSchema: string | null
  /**
   * Gap between two tab stops, in spaces.
   */
  indentSize: number
}>()

/**
 * Text of the editor.
 */
const text = defineModel<string>({ required: true })

const emit = defineEmits<{
  /**
   * The user asks to run the query under the cursor (Ctrl+Enter).
   */
  run: [cursor: number]
  /**
   * The user asks to run the queries the selected passages touch, or all of
   * them if nothing is selected (Ctrl+Shift+Enter).
   */
  'run-all': [selection: SelectionRange[]]
  /**
   * Text becomes selected in the editor (`true`), or no longer is (`false`).
   */
  selection: [selected: boolean]
}>()

const host = useTemplateRef<HTMLDivElement>('host')

/**
 * Slot of the language in the configuration, to change it with the dialect,
 * the catalog or the current database.
 */
const language = new Compartment()

/**
 * Slot of the indentation in the configuration, to change it with the settings.
 */
const indentation = new Compartment()

let view: EditorView | null = null

/**
 * Was text selected in the editor, at the last change of the selection?
 */
let selected = false

/**
 * Delay between the last keystroke and the analysis of the text, in milliseconds.
 */
const ANALYSIS_DELAY = 250

/**
 * Pending deferred analysis, absent if none.
 */
let analysisTimer: ReturnType<typeof setTimeout> | undefined

/**
 * Schedules the analysis of the text, replacing the one that was waiting.
 */
function scheduleAnalysis(): void {
  clearTimeout(analysisTimer)
  analysisTimer = setTimeout(analyze, ANALYSIS_DELAY)
}

/**
 * Has the server analyse the text and passes the result on to the editor
 * (queries and syntax errors),
 * unless the text changed in the meantime (another analysis will follow). A
 * failure has no consequence: the editor keeps the previous analysis.
 */
async function analyze(): Promise<void> {
  const doc = view?.state.doc
  if (doc === undefined) {
    return
  }
  try {
    const analysis = await analyzeSql(doc.toString(), props.flavor)
    if (view?.state.doc === doc) {
      const diagnostics: Diagnostic[] = analysis.errors.map((error) => ({
        from: error.start,
        to: error.end,
        severity: 'error',
        message: describe(error.detail),
      }))
      view.dispatch(setDiagnostics(view.state, diagnostics), {
        effects: setStatements.of(analysis.statements),
      })
    }
  } catch {
    // Server unreachable: the analysis will come back on the next keystroke.
  }
}

/**
 * Keywords of the server's dialect and version, `null` as long as they are
 * not received: highlighting then uses the lists of `lang-sql`.
 */
let keywords: DialectKeywords | null = null

/**
 * Reserved words of the server's dialect and version, in capitals: a name of
 * the catalog that is one is quoted by completion. None as long as the
 * keywords are not received.
 */
const reserved = shallowRef<ReadonlySet<string>>(new Set())

/**
 * Databases, tables and columns offered by completion, quoted when needed.
 */
const completion = computed(() => completionSchema(props.schemas, props.ansiQuotes, reserved.value))

/**
 * Number of the last keywords request: an answer that arrives after a newer
 * request is ignored, as it may describe another dialect or version.
 */
let keywordsRequest = 0

/**
 * Reads the keywords of the server's dialect and version, then highlights
 * the text again. A failure keeps the highlighting in place.
 */
async function loadKeywords(): Promise<void> {
  const request = ++keywordsRequest
  try {
    const received = await fetchKeywords(props.flavor, props.version)
    if (request !== keywordsRequest) {
      return
    }
    keywords = received
    reserved.value = new Set(received.reserved)
    view?.dispatch({ effects: language.reconfigure(currentLanguage()) })
  } catch {
    // Server unreachable: lang-sql's highlighting stays in place.
  }
}

/**
 * CodeMirror language of the editor: dialect and completion, from the
 * properties and the keywords received.
 */
function currentLanguage(): LanguageSupport {
  const fallback = props.flavor === 'mariadb' ? MariaSQL : MySQL
  const dialect = keywords === null ? fallback : editorDialect(keywords, props.ansiQuotes)
  // Workaround: language assembled by hand, because `sql()` always adds the
  // completion of keywords; only the names of the catalog are offered
  // (`docs/DEVELOPER.md`, section "Workarounds").
  const names = schemaCompletionSource({
    dialect,
    schema: completion.value,
    defaultSchema: props.defaultSchema ?? undefined,
  })
  return new LanguageSupport(dialect.language, [dialect.language.data.of({ autocomplete: names })])
}

onMounted(() => {
  view = new EditorView({
    parent: host.value!,
    doc: text.value,
    extensions: [
      // Before sqlSetup, to take precedence over its own shortcuts.
      keymap.of([
        {
          key: 'Mod-Enter',
          run: (editor) => {
            emit('run', editor.state.selection.main.head)
            return true
          },
        },
        {
          key: 'Mod-Shift-Enter',
          run: (editor) => {
            emit('run-all', selectedPassages(editor))
            return true
          },
        },
        // Escape then Tab, or Ctrl+M, give Tab back to the browser (CodeMirror).
        { key: 'Tab', run: indentOrInsertSpaces, shift: dedentLines },
        // Indentation of the line left, without the SQL language's; completion,
        // which takes precedence, keeps Enter to accept.
        {
          key: 'Enter',
          run: insertNewlineKeepingIndentation,
          shift: insertNewlineKeepingIndentation,
        },
        ...scrollKeymap,
      ]),
      sqlSetup,
      indentation.of(editorIndentation(props.indentSize)),
      // Strict completion: start of the name or initials, never scattered letters.
      autocompletion({ filterStrict: true }),
      editorTheme(),
      language.of(currentLanguage()),
      currentStatement(),
      trimTrailingBlanks(),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          text.value = update.state.doc.toString()
          scheduleAnalysis()
        }
        const selecting = update.state.selection.ranges.some((range) => !range.empty)
        if (selecting !== selected) {
          selected = selecting
          emit('selection', selecting)
        }
      }),
    ],
  })
  scheduleAnalysis()
})

// Text changed from outside (and not by typing): the content is replaced.
watch(text, (value) => {
  if (view && value !== view.state.doc.toString()) {
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } })
  }
})

watch(
  () => [props.flavor, props.ansiQuotes, completion.value, props.defaultSchema],
  () => view?.dispatch({ effects: language.reconfigure(currentLanguage()) }),
)

watch(
  () => props.indentSize,
  (size) => view?.dispatch({ effects: indentation.reconfigure(editorIndentation(size)) }),
)

// The dialect changes when the session opens: the analysis must be redone.
watch(() => props.flavor, scheduleAnalysis)

// Dialect or version known or changed: their keywords must be read again.
watch(() => [props.flavor, props.version], loadKeywords, { immediate: true })

onBeforeUnmount(() => {
  clearTimeout(analysisTimer)
  view?.destroy()
  view = null
})

/**
 * Passages selected in the editor, in UTF-16 units; the cursors without a
 * selection are left out.
 *
 * @param editor View of the editor.
 */
function selectedPassages(editor: EditorView): SelectionRange[] {
  return editor.state.selection.ranges
    .filter((range) => !range.empty)
    .map((range) => ({ start: range.from, end: range.to }))
}

/**
 * Position of the cursor in the text, in UTF-16 units (0 before mounting).
 */
function cursor(): number {
  return view?.state.selection.main.head ?? 0
}

/**
 * Passages selected in the editor, for the "Exécuter tout" and "Exécuter
 * précisément" buttons: the same as Ctrl+Shift+Enter. None before mounting.
 */
function selection(): SelectionRange[] {
  return view ? selectedPassages(view) : []
}

/**
 * Changes the text of the editor (a query generated by the navigator), then
 * gives the focus to the editor, cursor at the given place, scrolled into
 * view, ready to edit. Ctrl+Z undoes the change. No effect before mounting.
 *
 * @param changes Changes, in positions of the current text.
 * @param cursor Position of the cursor in the changed text, in UTF-16 units.
 */
function changeText(changes: ChangeSpec, cursor: number): void {
  if (!view) {
    return
  }
  view.dispatch({ changes, selection: { anchor: cursor }, scrollIntoView: true })
  view.focus()
}

defineExpose({ cursor, selection, changeText })
</script>

<template>
  <div ref="host" class="sql-editor" />
</template>

<style scoped>
.sql-editor {
  block-size: 100%;
  border: var(--line-width) solid var(--color-line);
  border-radius: var(--radius-lg);
  overflow: hidden;
}

.sql-editor :deep(.cm-editor) {
  block-size: 100%;
  font-size: var(--editor-font-size);
}

.sql-editor :deep(.cm-scroller) {
  font-family: var(--font-mono);
}
</style>
