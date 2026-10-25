/**
 * Extensions of the SQL editor: CodeMirror's `basicSetup`, copied as its
 * documentation advises for an adjusted setup, without the fold gutter.
 * The line numbers stand alone in their gutter, like the row numbers of the
 * grid; folding stays on the keyboard (`foldKeymap`: Ctrl+Shift+[ and
 * Ctrl+Shift+]). The data explorer keeps `basicSetup`, whose fold arrows
 * fold its JSON and XML nodes.
 *
 * Workaround: `basicSetup` takes no options; this copy follows it when it
 * changes (`docs/DEVELOPER.md`, section "Workarounds").
 */
import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
} from '@codemirror/autocomplete'
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands'
import {
  bracketMatching,
  defaultHighlightStyle,
  foldKeymap,
  indentOnInput,
  syntaxHighlighting,
} from '@codemirror/language'
import { lintKeymap } from '@codemirror/lint'
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search'
import { EditorState, type Extension } from '@codemirror/state'
import {
  crosshairCursor,
  drawSelection,
  dropCursor,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
} from '@codemirror/view'

/**
 * `basicSetup` of CodeMirror, in its order, without `foldGutter()`.
 */
export const sqlSetup: Extension = [
  lineNumbers(),
  highlightActiveLineGutter(),
  highlightSpecialChars(),
  history(),
  drawSelection(),
  dropCursor(),
  EditorState.allowMultipleSelections.of(true),
  indentOnInput(),
  syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
  bracketMatching(),
  closeBrackets(),
  autocompletion(),
  rectangularSelection(),
  crosshairCursor(),
  highlightActiveLine(),
  highlightSelectionMatches(),
  keymap.of([
    ...closeBracketsKeymap,
    ...defaultKeymap,
    ...searchKeymap,
    ...historyKeymap,
    ...foldKeymap,
    ...completionKeymap,
    ...lintKeymap,
  ]),
]
