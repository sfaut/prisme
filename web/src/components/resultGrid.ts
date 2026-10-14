/**
 * Logique de la grille de résultats : tri, filtre, sélection et copie.
 *
 * Ces fonctions ne dépendent pas de l'affichage (voir `ResultGrid.vue`).
 * Les lignes du résultat ne sont jamais recopiées : la vue triée et filtrée
 * est une liste d'indices dans le tableau d'origine.
 *
 * Vocabulaire : une « ligne de la vue » est une position dans la grille
 * affichée (après tri et filtre) ; une « ligne source » est une position dans
 * le résultat reçu du serveur.
 */

import type { Cell } from '@/api/querySession'

/**
 * Sens du tri.
 */
export type SortDirection = 'asc' | 'desc'

/**
 * Tri appliqué à la grille.
 */
export interface SortState {
  /**
   * Indice de la colonne triée.
   */
  column: number
  /**
   * Sens du tri.
   */
  direction: SortDirection
}

/**
 * Filtre appliqué à la grille.
 */
export interface GridFilter {
  /**
   * Texte cherché, sans distinction de casse. Vide : pas de filtre.
   */
  text: string
  /**
   * Colonne filtrée, ou `null` pour chercher dans toutes les colonnes.
   */
  column: number | null
}

/**
 * Plage rectangulaire de cellules, en lignes de la vue, bornes comprises.
 */
export interface CellRange {
  top: number
  bottom: number
  left: number
  right: number
}

/**
 * Position d'une cellule : ligne de la vue et colonne.
 */
export interface CellPosition {
  row: number
  column: number
}

/**
 * Sélection : une ou plusieurs plages, et la cellule de départ d'une extension (Maj-clic).
 */
export interface GridSelection {
  ranges: CellRange[]
  anchor: CellPosition | null
}

/**
 * Manière dont un clic modifie la sélection.
 *
 * - `replace` : clic simple, la sélection est remplacée ;
 * - `extend` : Maj-clic, la dernière plage s'étend jusqu'au clic ;
 * - `add` : Ctrl-clic, une plage s'ajoute aux précédentes.
 */
export type SelectMode = 'replace' | 'extend' | 'add'

/**
 * Texte affiché pour NULL dans la grille.
 */
export const NULL_TEXT = 'NULL'

/**
 * Largeur minimale d'une colonne redimensionnée, en pixels : de quoi garder
 * le bouton de tri et un début de nom.
 */
export const MIN_COLUMN_WIDTH = 40

/**
 * Ordre d'affichage des colonnes après un déplacement.
 *
 * @param order Ordre actuel : indice dans le `SELECT` de chaque colonne affichée.
 * @param from Position affichée de la colonne déplacée.
 * @param to
 *     Position d'insertion, comptée avant le retrait de la colonne
 *     (0 : tout à gauche, `order.length` : tout à droite).
 * @returns Le nouvel ordre, et la nouvelle position de la colonne déplacée.
 */
export function moveColumn(
  order: number[],
  from: number,
  to: number,
): { order: number[]; position: number } {
  const moved = order[from]
  if (moved === undefined) {
    return { order, position: from }
  }
  const position = to > from ? to - 1 : to
  const result = order.filter((_, index) => index !== from)
  result.splice(position, 0, moved)
  return { order: result, position }
}

/**
 * Distance à parcourir avant qu'un appui sur un nom de colonne (ou sur un
 * onglet de résultat) devienne un déplacement, en pixels : un clic un peu
 * tremblé reste un clic.
 */
export const MOVE_THRESHOLD = 5

/**
 * Position d'insertion d'une colonne lâchée à l'abscisse `x` : avant la
 * première colonne dont le milieu est à droite du pointeur.
 *
 * @param centers Abscisse du milieu de chaque en-tête affiché, de gauche à droite.
 * @param x Abscisse du pointeur.
 */
export function dropPosition(centers: number[], x: number): number {
  const index = centers.findIndex((center) => x < center)
  return index < 0 ? centers.length : index
}

/**
 * Largeur d'un caractère, en pixels, pour estimer la largeur des colonnes.
 */
export const CHAR_WIDTH = 7

/**
 * Largeur maximale estimée d'une colonne, en pixels.
 */
export const MAX_ESTIMATED_WIDTH = 360

/**
 * Largeur maximale d'une colonne ajustée à son contenu (double clic sur le
 * bord de son en-tête), en pixels.
 */
export const MAX_FITTED_WIDTH = 720

/**
 * Nombre de lignes lues pour estimer la largeur des colonnes.
 */
const WIDTH_SAMPLE = 200

/**
 * Largeur d'une colonne d'après son texte le plus long : en-tête (plus la
 * place du bouton de tri) et lignes données.
 *
 * @param name Nom de la colonne.
 * @param rows Lignes à mesurer.
 * @param column Indice de la colonne.
 * @param max Largeur maximale, en pixels.
 */
function contentWidth(name: string, rows: Cell[][], column: number, max: number): number {
  let longest = name.length + 2
  for (const row of rows) {
    longest = Math.max(longest, (row[column] ?? NULL_TEXT).length)
  }
  return Math.min(max, Math.max(48, longest * CHAR_WIDTH + 16))
}

/**
 * Largeur estimée d'une colonne, sur l'en-tête et les premières lignes,
 * plafonnée pour qu'une seule colonne ne prenne pas tout l'écran.
 *
 * @param name Nom de la colonne.
 * @param rows Lignes du résultat.
 * @param column Indice de la colonne.
 */
export function estimatedWidth(name: string, rows: Cell[][], column: number): number {
  return contentWidth(name, rows.slice(0, WIDTH_SAMPLE), column, MAX_ESTIMATED_WIDTH)
}

/**
 * Largeur ajustée au contenu d'une colonne : sa valeur la plus longue sur
 * toutes les lignes reçues, avec un plafond plus haut que l'estimation.
 *
 * @param name Nom de la colonne.
 * @param rows Lignes du résultat.
 * @param column Indice de la colonne.
 */
export function fittedWidth(name: string, rows: Cell[][], column: number): number {
  return contentWidth(name, rows, column, MAX_FITTED_WIDTH)
}

/**
 * Largeur d'une colonne après un glissement de son bord droit.
 *
 * @param start Largeur au début du glissement, en pixels.
 * @param delta Déplacement horizontal du pointeur, en pixels.
 */
export function resizedWidth(start: number, delta: number): number {
  return Math.max(MIN_COLUMN_WIDTH, Math.round(start + delta))
}

/**
 * Expression d'un nombre décimal, pour trier les nombres par valeur.
 */
const NUMBER = /^-?\d+(\.\d+)?([eE][-+]?\d+)?$/

/**
 * Compare deux cellules pour le tri croissant.
 *
 * NULL vient en premier, comme dans MySQL et MariaDB. Deux nombres se
 * comparent par valeur, le reste comme du texte, en tenant compte des
 * chiffres (« a2 » avant « a10 »).
 *
 * @param a Première cellule.
 * @param b Seconde cellule.
 */
export function compareCells(a: Cell, b: Cell): number {
  if (a === null || b === null) {
    return a === b ? 0 : a === null ? -1 : 1
  }
  if (NUMBER.test(a) && NUMBER.test(b)) {
    return Number(a) - Number(b)
  }
  return a.localeCompare(b, undefined, { numeric: true })
}

/**
 * Calcule la vue : indices des lignes sources qui passent le filtre, dans l'ordre du tri.
 *
 * Sans tri, l'ordre reçu du serveur est conservé. Le tri est stable : à
 * valeur égale, l'ordre du serveur est conservé aussi.
 *
 * @param rows Lignes du résultat.
 * @param filter Filtre.
 * @param sort Tri, `null` pour aucun.
 */
export function visibleRows(rows: Cell[][], filter: GridFilter, sort: SortState | null): number[] {
  const needle = filter.text.trim().toLowerCase()
  const view: number[] = []
  rows.forEach((row, index) => {
    if (needle === '' || matches(row, needle, filter.column)) {
      view.push(index)
    }
  })
  if (sort) {
    const sign = sort.direction === 'asc' ? 1 : -1
    view.sort(
      (a, b) => sign * compareCells(rows[a]![sort.column] ?? null, rows[b]![sort.column] ?? null),
    )
  }
  return view
}

/**
 * Indique si une ligne contient le texte cherché, dans une colonne ou dans toutes.
 *
 * @param row Ligne.
 * @param needle Texte cherché, en minuscules.
 * @param column Colonne, ou `null` pour toutes.
 */
function matches(row: Cell[], needle: string, column: number | null): boolean {
  const cells = column === null ? row : [row[column] ?? null]
  return cells.some((cell) => (cell ?? NULL_TEXT).toLowerCase().includes(needle))
}

/**
 * Tri suivant après un clic sur l'en-tête d'une colonne : croissant, puis décroissant, puis aucun.
 *
 * @param current Tri actuel.
 * @param column Colonne cliquée.
 */
export function nextSort(current: SortState | null, column: number): SortState | null {
  if (current?.column !== column) {
    return { column, direction: 'asc' }
  }
  return current.direction === 'asc' ? { column, direction: 'desc' } : null
}

/**
 * Sélection vide.
 */
export function emptySelection(): GridSelection {
  return { ranges: [], anchor: null }
}

/**
 * Applique un clic sur une plage cible, selon le mode.
 *
 * @param selection Sélection actuelle.
 * @param target Plage visée par le clic (une cellule, une ligne entière ou une colonne entière).
 * @param position Cellule cliquée, nouveau point de départ.
 * @param mode Clic simple, Maj-clic ou Ctrl-clic.
 * @param stretch Étend la plage depuis le point de départ jusqu'à la cible.
 */
function applySelection(
  selection: GridSelection,
  target: CellRange,
  position: CellPosition,
  mode: SelectMode,
  stretch: (anchor: CellPosition) => CellRange,
): GridSelection {
  if (mode === 'extend' && selection.anchor) {
    const ranges = selection.ranges.slice(0, -1)
    return { ranges: [...ranges, stretch(selection.anchor)], anchor: selection.anchor }
  }
  const ranges = mode === 'add' ? [...selection.ranges, target] : [target]
  return { ranges, anchor: position }
}

/**
 * Sélectionne une cellule.
 *
 * @param selection Sélection actuelle.
 * @param row Ligne de la vue.
 * @param column Colonne.
 * @param mode Clic simple, Maj-clic ou Ctrl-clic.
 */
export function selectCell(
  selection: GridSelection,
  row: number,
  column: number,
  mode: SelectMode,
): GridSelection {
  return applySelection(
    selection,
    { top: row, bottom: row, left: column, right: column },
    { row, column },
    mode,
    (anchor) => ({
      top: Math.min(anchor.row, row),
      bottom: Math.max(anchor.row, row),
      left: Math.min(anchor.column, column),
      right: Math.max(anchor.column, column),
    }),
  )
}

/**
 * Sélectionne une ligne entière.
 *
 * @param selection Sélection actuelle.
 * @param row Ligne de la vue.
 * @param columnCount Nombre de colonnes.
 * @param mode Clic simple, Maj-clic ou Ctrl-clic.
 */
export function selectRow(
  selection: GridSelection,
  row: number,
  columnCount: number,
  mode: SelectMode,
): GridSelection {
  const right = columnCount - 1
  return applySelection(
    selection,
    { top: row, bottom: row, left: 0, right },
    { row, column: 0 },
    mode,
    (anchor) => ({
      top: Math.min(anchor.row, row),
      bottom: Math.max(anchor.row, row),
      left: 0,
      right,
    }),
  )
}

/**
 * Sélectionne une colonne entière.
 *
 * @param selection Sélection actuelle.
 * @param column Colonne.
 * @param rowCount Nombre de lignes de la vue.
 * @param mode Clic simple, Maj-clic ou Ctrl-clic.
 */
export function selectColumn(
  selection: GridSelection,
  column: number,
  rowCount: number,
  mode: SelectMode,
): GridSelection {
  const bottom = rowCount - 1
  return applySelection(
    selection,
    { top: 0, bottom, left: column, right: column },
    { row: 0, column },
    mode,
    (anchor) => ({
      top: 0,
      bottom,
      left: Math.min(anchor.column, column),
      right: Math.max(anchor.column, column),
    }),
  )
}

/**
 * Sélectionne toute la grille.
 *
 * @param rowCount Nombre de lignes de la vue.
 * @param columnCount Nombre de colonnes.
 */
export function selectAll(rowCount: number, columnCount: number): GridSelection {
  if (rowCount === 0 || columnCount === 0) {
    return emptySelection()
  }
  return {
    ranges: [{ top: 0, bottom: rowCount - 1, left: 0, right: columnCount - 1 }],
    anchor: { row: 0, column: 0 },
  }
}

/**
 * Indique si une cellule est sélectionnée.
 *
 * @param selection Sélection.
 * @param row Ligne de la vue.
 * @param column Colonne.
 */
export function isSelected(selection: GridSelection, row: number, column: number): boolean {
  return selection.ranges.some(
    (range) =>
      row >= range.top && row <= range.bottom && column >= range.left && column <= range.right,
  )
}

/**
 * Copie la sélection en texte tabulé, collable dans un tableur.
 *
 * Le texte couvre les lignes et les colonnes touchées par la sélection ; une
 * cellule non sélectionnée à l'intérieur de ce cadre est copiée vide. NULL
 * est copié comme une cellule vide. Une valeur qui contient une tabulation,
 * un retour à la ligne ou un guillemet est mise entre guillemets, avec les
 * guillemets internes doublés (format des tableurs).
 *
 * @param selection Sélection.
 * @param view Vue : indices des lignes sources.
 * @param rows Lignes du résultat.
 * @param columns Noms des colonnes.
 * @param withHeaders Ajoute une première ligne avec les noms des colonnes copiées.
 * @param order
 *     Indice dans le `SELECT` de chaque colonne affichée, quand les colonnes
 *     ont été déplacées : la sélection porte sur les positions affichées.
 */
export function selectionToText(
  selection: GridSelection,
  view: number[],
  rows: Cell[][],
  columns: string[],
  withHeaders: boolean,
  order: number[] = columns.map((_, index) => index),
): string {
  const rowIndexes = coveredIndexes(selection.ranges, 'top', 'bottom')
  const columnIndexes = coveredIndexes(selection.ranges, 'left', 'right')
  const lines = rowIndexes.map((row) =>
    columnIndexes
      .map((column) => {
        const source = view[row]
        const field = order[column] ?? column
        const cell = source === undefined ? null : (rows[source]?.[field] ?? null)
        return isSelected(selection, row, column) ? escapeCell(cell ?? '') : ''
      })
      .join('\t'),
  )
  if (withHeaders && columnIndexes.length > 0) {
    lines.unshift(
      columnIndexes.map((column) => escapeCell(columns[order[column] ?? column] ?? '')).join('\t'),
    )
  }
  return lines.join('\n')
}

/**
 * Indices couverts par au moins une plage, triés, entre les bornes données.
 *
 * @param ranges Plages.
 * @param start Borne de début (`top` ou `left`).
 * @param end Borne de fin (`bottom` ou `right`).
 */
function coveredIndexes(
  ranges: CellRange[],
  start: 'top' | 'left',
  end: 'bottom' | 'right',
): number[] {
  const indexes = new Set<number>()
  for (const range of ranges) {
    for (let index = range[start]; index <= range[end]; index++) {
      indexes.add(index)
    }
  }
  return [...indexes].sort((a, b) => a - b)
}

/**
 * Met une valeur entre guillemets si elle contient un séparateur ou un guillemet.
 *
 * @param value Valeur à copier.
 */
function escapeCell(value: string): string {
  return /[\t\n\r"]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value
}
