/**
 * Folding of the data explorer (`DataExplorer.vue`) by level: everything at
 * once, or the level of the cursor.
 *
 * The foldable nodes are those the language marks (`foldNodeProp`): objects
 * and arrays of JSON, elements of XML, when they span several lines. The
 * level of a node is the number of foldable nodes around it (0: the root);
 * that of the cursor, the number of foldable nodes whose content holds its
 * line. On the line `"address": {`, the cursor is at the level of the
 * `address` object: folding this level folds it, with all the nodes of the
 * same depth. A leaf deeper than every foldable node (`"zip": 1` in the
 * innermost object) has a level without anything to fold.
 */

import {
  ensureSyntaxTree,
  foldEffect,
  foldNodeProp,
  foldedRanges,
  unfoldEffect,
} from '@codemirror/language'
import type { EditorState, StateEffect } from '@codemirror/state'

/**
 * Syntax tree of a document.
 */
type Tree = NonNullable<ReturnType<typeof ensureSyntaxTree>>

/**
 * Time given to the parser to read the whole document, in milliseconds.
 */
const PARSE_TIMEOUT = 2000

/**
 * Foldable node: its folded range, and its level.
 */
export interface FoldableNode {
  from: number
  to: number
  level: number
}

/**
 * Foldable nodes already found, by syntax tree of a whole document: the
 * explorer is read-only, its tree only changes with its content.
 */
const found = new WeakMap<Tree, FoldableNode[]>()

/**
 * Foldable nodes of the document, spanning several lines, in the order of the
 * text; none if the document cannot be parsed in time.
 *
 * @param state State of the editor.
 */
export function foldableNodes(state: EditorState): FoldableNode[] {
  const tree = ensureSyntaxTree(state, state.doc.length, PARSE_TIMEOUT)
  if (tree === null) {
    return []
  }
  const known = found.get(tree)
  if (known !== undefined) {
    return known
  }
  const nodes: FoldableNode[] = []
  // For each open node, is it foldable?
  const open: boolean[] = []
  tree.iterate({
    enter: (node) => {
      const range = node.type.prop(foldNodeProp)?.(node.node, state) ?? null
      const foldable = range !== null && range.to > state.doc.lineAt(range.from).to
      if (foldable) {
        nodes.push({ from: range.from, to: range.to, level: open.filter(Boolean).length })
      }
      open.push(foldable)
    },
    leave: () => {
      open.pop()
    },
  })
  found.set(tree, nodes)
  return nodes
}

/**
 * Level of the cursor: number of foldable nodes whose content holds its line.
 *
 * @param state State of the editor.
 * @param nodes Foldable nodes of the document.
 */
export function cursorLevel(state: EditorState, nodes: FoldableNode[]): number {
  const line = state.doc.lineAt(state.selection.main.head)
  return nodes.filter((node) => node.from <= line.from && line.to <= node.to).length
}

/**
 * Effects that fold (or unfold) the nodes of a level, or of all the levels;
 * none for a node already in the wanted state.
 *
 * @param state State of the editor.
 * @param fold Folds if `true`, unfolds otherwise.
 * @param level Level of the nodes, `null` for all the levels.
 */
export function foldingEffects(
  state: EditorState,
  fold: boolean,
  level: number | null,
): StateEffect<{ from: number; to: number }>[] {
  const folded = new Set<string>()
  foldedRanges(state).between(0, state.doc.length, (from, to) => {
    folded.add(`${from}-${to}`)
  })
  return foldableNodes(state)
    .filter((node) => level === null || node.level === level)
    .filter((node) => folded.has(`${node.from}-${node.to}`) !== fold)
    .map((node) => (fold ? foldEffect : unfoldEffect).of({ from: node.from, to: node.to }))
}
