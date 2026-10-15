<!--
  Grille de résultats compacte.

  - Tri : bouton ▲▼ de l'en-tête (croissant, décroissant, aucun).
  - Filtre : texte cherché dans une colonne ou dans toutes. Le formulaire est
    replié ; le bouton de la barre du bas le déplie (et le replie : le filtre
    reste appliqué, le bouton le signale).
  - Barre du bas : bouton du filtre, nombre de lignes chargées (« 120 /
    5 000 lignes » quand un filtre est actif), puis le contenu de
    l'emplacement `footer` (durée de la requête).
  - Sélection : clic sur une cellule, sur un numéro de ligne ou sur un nom de
    colonne ; Maj-clic étend, Ctrl-clic ajoute, glisser étend aussi (sur les
    numéros de ligne, glisser sélectionne un groupe de lignes). Un clic sur
    le coin supérieur gauche sélectionne tout.
  - Copie : Ctrl-C copie la sélection avec les noms des colonnes en première
    ligne (un réglage du profil pourra les retirer). Ctrl-A sélectionne tout,
    Échap vide la sélection.
  - Largeur : glisser le bord droit d'un en-tête change la largeur de la
    colonne ; un double clic sur ce bord l'ajuste à sa valeur la plus longue
    (avec un plafond), un second lui rend sa largeur estimée. Les largeurs
    choisies sont retrouvées quand la même requête est relancée
    (`gridWidths.ts`). Un texte trop long est tronqué (…) et s'affiche en
    entier au survol.
  - Ordre : glisser un nom de colonne la déplace. Une nouvelle exécution
    rétablit l'ordre du `SELECT`. La sélection et la copie suivent l'ordre
    affiché ; tri, filtre et largeurs restent attachés à la colonne.
  - Alignement : nombres, dates et heures à droite, d'après le type annoncé
    par le serveur.
  - Types : chaque en-tête montre l'icône de la famille du type de sa
    colonne (`typeIcons.ts`) ; le nom natif du type (`VARCHAR(20)`,
    `DECIMAL(10,2)`...) s'affiche au survol. Une colonne de texte qui
    contient du JSON ou du XML montre l'icône de ce format (`contentKind.ts`).
  - Liens : dans une colonne d'adresses web, chaque adresse est un lien,
    précédé d'une icône de lien externe et ouvert dans un nouvel onglet.

  Seules les lignes visibles existent dans la page (défilement virtuel) : la
  grille reste fluide avec des dizaines de milliers de lignes. La logique
  (tri, filtre, sélection, copie) est dans `resultGrid.ts`.
-->
<script setup lang="ts">
import { ExternalLink, ListFilter, Search, X } from 'lucide-vue-next'
import { useVirtualizer } from '@tanstack/vue-virtual'
import { computed, nextTick, reactive, ref, shallowRef, useTemplateRef, watch } from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { Cell } from '@/api/querySession'
import { cssPixels } from '@/cssLength'

import {
  type GridFilter,
  type SelectMode,
  type SortState,
  MIN_COLUMN_WIDTH,
  MOVE_THRESHOLD,
  CHAR_WIDTH,
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
  visibleRows,
} from './resultGrid'
import { rememberWidths, rememberedWidths } from './gridWidths'
import { type ContentKind, detectContent, isWebAddress } from './contentKind'
import { columnIcon } from './typeIcons'
import UiButton from './ui/UiButton.vue'
import UiInput from './ui/UiInput.vue'
import UiSelect from './ui/UiSelect.vue'
import { notify } from './ui/toasts'

const props = defineProps<{
  /**
   * Noms des colonnes.
   */
  columns: string[]
  /**
   * Pour chaque colonne, ses valeurs s'alignent-elles à droite (nombres,
   * dates et heures) ?
   */
  rightAligned: boolean[]
  /**
   * Type de chaque colonne ; vide si inconnu (en-têtes sans type).
   */
  columnTypes: ColumnTypeInfo[]
  /**
   * Lignes du résultat ; le tableau peut grandir pendant la réception, sans
   * être réactif : c'est `rowCount` qui signale les nouvelles lignes.
   */
  rows: Cell[][]
  /**
   * Nombre de lignes reçues.
   */
  rowCount: number
  /**
   * Nombre de lignes affiché dans la barre du bas, rafraîchi moins souvent
   * que `rowCount` pendant la réception ; absent, `rowCount`.
   */
  shownCount?: number
  /**
   * Texte de la requête : les largeurs choisies sont retenues pour elle, et
   * retrouvées si elle est relancée.
   */
  sql: string
}>()

/**
 * Hauteur d'une ligne, en pixels, pour le défilement virtuel
 * (`--grid-row-height` du thème).
 */
const ROW_HEIGHT = cssPixels('--grid-row-height', 22)

const scroller = useTemplateRef<HTMLDivElement>('scroller')

const filter = reactive<GridFilter>({ text: '', column: null })

/**
 * Formulaire du filtre déplié ?
 */
const filterOpen = ref(false)

const filterBox = useTemplateRef<HTMLDivElement>('filterBox')

/**
 * Déplie ou replie le formulaire du filtre ; déplié, son champ prend le focus.
 */
async function toggleFilter(): Promise<void> {
  filterOpen.value = !filterOpen.value
  if (filterOpen.value) {
    await nextTick()
    filterBox.value?.querySelector('input')?.focus()
  }
}

/**
 * Nombre de lignes de la barre du bas : lignes chargées, précédées des
 * lignes retenues quand un filtre est actif.
 */
const countLabel = computed(() => {
  const loaded = props.shownCount ?? props.rowCount
  const total = `${loaded.toLocaleString('fr-FR')} ${loaded > 1 ? 'lignes' : 'ligne'}`
  return filter.text === '' ? total : `${view.value.length.toLocaleString('fr-FR')} / ${total}`
})
const sort = ref<SortState | null>(null)
const selection = shallowRef(emptySelection())

/**
 * Glissement de souris en cours : plage de cellules, ou groupe de lignes
 * (commencé sur un numéro de ligne) ; `null` sans glissement.
 */
let dragging: 'cells' | 'rows' | null = null

/**
 * Donne le focus clavier à la grille, pour que Ctrl-C, Ctrl-A et Échap s'y
 * appliquent. Les clics de sélection empêchent le comportement par défaut
 * (sélection du texte de la page), qui aurait aussi donné ce focus.
 */
function focusGrid(): void {
  scroller.value?.focus({ preventScroll: true })
}

/**
 * Commence un glissement, qui prend fin au relâchement du bouton.
 *
 * @param kind Ce que le glissement sélectionne.
 */
function startDragging(kind: 'cells' | 'rows'): void {
  dragging = kind
  window.addEventListener('mouseup', () => (dragging = null), { once: true })
}

/**
 * Vue : indices des lignes sources, filtrées et triées.
 */
const view = computed(() => {
  // Lu pour que la vue se recalcule à l'arrivée de nouvelles lignes.
  void props.rowCount
  return visibleRows(props.rows, filter, sort.value)
})

/**
 * Largeurs choisies à la souris, par indice de colonne ; les autres colonnes
 * gardent leur largeur estimée.
 */
const resized = ref<Record<number, number>>(rememberedWidths(props.sql, props.columns))

// Nouveau résultat : les largeurs retenues pour lui, s'il y en a.
watch(
  () => props.columns,
  (columns) => {
    resized.value = rememberedWidths(props.sql, columns)
  },
)

// Toute largeur choisie est retenue pour la requête.
watch(resized, (widths) => rememberWidths(props.sql, props.columns, widths))

/**
 * Ordre d'affichage : indice dans le `SELECT` de chaque colonne affichée.
 * Les positions affichées servent à la sélection ; les indices du `SELECT`
 * au tri, au filtre et aux largeurs.
 */
const order = ref(props.columns.map((_, index) => index))

// Nouveau résultat : l'ordre du SELECT.
watch(
  () => props.columns,
  (columns) => {
    order.value = columns.map((_, index) => index)
  },
)

/**
 * Largeurs estimées des colonnes, sur l'en-tête et les premières lignes.
 */
const estimated = computed(() => {
  // Lu pour que les largeurs se recalculent à l'arrivée de nouvelles lignes.
  void props.rowCount
  return props.columns.map((_, column) => estimatedWidth(headerText(column), props.rows, column))
})

/**
 * Contenu reconnu de chaque colonne de texte (JSON ou XML), d'après ses
 * premières valeurs ; `null` pour les autres colonnes.
 */
const contentKinds = computed<(ContentKind | null)[]>(() => {
  // Lu pour que la détection se refasse à l'arrivée de nouvelles lignes.
  void props.rowCount
  return props.columns.map((_, column) =>
    props.columnTypes[column]?.family === 'text' ? detectContent(props.rows, column) : null,
  )
})

/**
 * Pour chaque colonne, ses valeurs s'affichent-elles en chasse fixe (`CHAR`) ?
 */
const monospaced = computed(() =>
  props.columns.map((_, column) => isFixedLengthText(props.columnTypes[column])),
)

/**
 * Marque de chaque colonne dans son en-tête (icône ou texte), absente sans type.
 */
const columnMarks = computed(() =>
  props.columns.map((_, column) => {
    const type = props.columnTypes[column]
    return type ? columnIcon(type, contentKinds.value[column] ?? null) : undefined
  }),
)

/**
 * Libellé du contenu reconnu d'une colonne, pour le survol de son en-tête.
 */
const CONTENT_LABELS: Record<ContentKind, string> = {
  json: 'contenu JSON',
  xml: 'contenu XML',
  url: 'adresses web',
}

/**
 * Texte de survol de l'en-tête d'une colonne : nom, type natif et contenu reconnu.
 *
 * @param column Indice de la colonne.
 */
function headerTitle(column: number): string {
  const type = props.columnTypes[column]
  const content = contentKinds.value[column]
  const details = [type?.name, content ? CONTENT_LABELS[content] : undefined].filter(Boolean)
  const name = props.columns[column] ?? ''
  return `${details.length > 0 ? `${name} ${details.join(', ')}` : name} (glisser pour déplacer)`
}

/**
 * Texte de l'en-tête d'une colonne, pour estimer sa largeur : son nom, plus
 * la place de la marque du type (icône, ou texte comme « 123 »).
 *
 * @param column Indice de la colonne.
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
 * Gabarit des colonnes de la grille : numéros de ligne, puis chaque colonne,
 * à sa largeur choisie ou estimée.
 */
const template = computed(() => {
  const widths = order.value.map(
    (column) => resized.value[column] ?? estimated.value[column] ?? MIN_COLUMN_WIDTH,
  )
  const numberWidth = String(props.rowCount).length * CHAR_WIDTH + 16
  return [numberWidth, ...widths].map((width) => `${width}px`).join(' ')
})

/**
 * Redimensionnement en cours : colonne, position de départ du pointeur et
 * largeur de départ.
 */
let resizing: { column: number; x: number; width: number } | null = null

/**
 * Commence le redimensionnement d'une colonne ; le pointeur reste capturé
 * même hors de la poignée.
 *
 * @param event Appui sur la poignée.
 * @param column Indice de la colonne.
 */
function onResizeStart(event: PointerEvent, column: number): void {
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  const width = resized.value[column] ?? estimated.value[column] ?? 0
  resizing = { column, x: event.clientX, width }
}

/**
 * Suit le pointeur pendant le redimensionnement.
 *
 * @param event Déplacement du pointeur.
 */
function onResizeMove(event: PointerEvent): void {
  if (resizing !== null) {
    resized.value = {
      ...resized.value,
      [resizing.column]: resizedWidth(resizing.width, event.clientX - resizing.x),
    }
  }
}

/**
 * Termine le redimensionnement.
 */
function onResizeEnd(): void {
  resizing = null
}

/**
 * Ajuste une colonne à sa valeur la plus longue, dans la limite d'un plafond ;
 * si elle l'est déjà, lui rend sa largeur estimée.
 *
 * @param column Indice de la colonne.
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

const filterOptions = computed(() => [
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

// Tri ou filtre modifié : les positions changent, la sélection n'a plus de sens.
watch([filter, sort], () => {
  selection.value = emptySelection()
})

/**
 * Mode de sélection d'après les touches tenues pendant le clic.
 *
 * @param event Clic.
 */
function modeOf(event: MouseEvent): SelectMode {
  if (event.shiftKey) {
    return 'extend'
  }
  return event.ctrlKey || event.metaKey ? 'add' : 'replace'
}

/**
 * Début d'une sélection de cellule (et d'un éventuel glissement).
 */
function onCellDown(event: MouseEvent, row: number, column: number): void {
  selection.value = selectCell(selection.value, row, column, modeOf(event))
  focusGrid()
  startDragging('cells')
}

/**
 * Glissement sur une cellule : la plage s'étend jusqu'à elle, ou le groupe
 * de lignes jusqu'à sa ligne.
 */
function onCellEnter(row: number, column: number): void {
  if (dragging === 'cells') {
    selection.value = selectCell(selection.value, row, column, 'extend')
  } else if (dragging === 'rows') {
    onRowEnter(row)
  }
}

/**
 * Clic sur un numéro de ligne : sélection de la ligne entière, et début
 * d'un éventuel glissement sur d'autres lignes.
 */
function onRowHeader(event: MouseEvent, row: number): void {
  selection.value = selectRow(selection.value, row, props.columns.length, modeOf(event))
  focusGrid()
  startDragging('rows')
}

/**
 * Glissement sur un numéro de ligne : le groupe de lignes s'étend jusqu'à elle.
 */
function onRowEnter(row: number): void {
  if (dragging === 'rows') {
    selection.value = selectRow(selection.value, row, props.columns.length, 'extend')
  }
}

/**
 * Déplacement de colonne en cours : position affichée de la colonne prise,
 * abscisse de départ du pointeur, et le glissement a-t-il dépassé le seuil ?
 */
let moving: { from: number; x: number; started: boolean } | null = null

/**
 * Position où la colonne déplacée serait insérée, `null` hors déplacement.
 */
const dropAt = ref<number | null>(null)

/**
 * Appui sur un nom de colonne : sélection de la colonne entière, et début
 * d'un éventuel déplacement.
 *
 * @param event Appui sur le nom.
 * @param position Position affichée de la colonne.
 */
function onColumnHeader(event: PointerEvent, position: number): void {
  selection.value = selectColumn(selection.value, position, view.value.length, modeOf(event))
  focusGrid()
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  moving = { from: position, x: event.clientX, started: false }
}

/**
 * Suit le pointeur pendant un déplacement de colonne.
 *
 * @param event Déplacement du pointeur.
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
 * Lâcher : la colonne prend sa nouvelle place et reste sélectionnée.
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
 * Abscisse du milieu de chaque en-tête de colonne affiché, de gauche à droite.
 */
function headerCenters(): number[] {
  const headers = scroller.value?.querySelectorAll('.grid-column-header') ?? []
  return Array.from(headers, (header) => {
    const rect = header.getBoundingClientRect()
    return rect.left + rect.width / 2
  })
}

/**
 * Cellule d'une ligne de la vue.
 *
 * @param row Position de la ligne dans la vue.
 * @param column Indice de la colonne dans le `SELECT`.
 */
function cellAt(row: number, column: number): Cell {
  const source = view.value[row]
  return source === undefined ? null : (props.rows[source]?.[column] ?? null)
}

/**
 * Change le tri d'une colonne : croissant, décroissant, aucun.
 */
function toggleSort(column: number): void {
  sort.value = nextSort(sort.value, column)
}

/**
 * Copie la sélection dans le presse-papiers.
 *
 * @param withHeaders Ajoute les noms des colonnes en première ligne.
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
    notify('Sélection copiée')
  } catch {
    notify('Copie refusée par le navigateur', 'error')
  }
}

/**
 * Sélectionne toutes les lignes et toutes les colonnes (Ctrl-A, clic sur le coin).
 */
function selectEverything(): void {
  selection.value = selectAll(view.value.length, props.columns.length)
  focusGrid()
}

/**
 * Raccourcis clavier de la grille.
 */
function onKeydown(event: KeyboardEvent): void {
  const command = event.ctrlKey || event.metaKey
  if (command && event.key.toLowerCase() === 'c') {
    event.preventDefault()
    // Avec les en-têtes, toujours pour l'instant : un réglage du profil
    // de l'utilisateur pourra les retirer.
    void copy(true)
  } else if (command && event.key.toLowerCase() === 'a') {
    event.preventDefault()
    selectEverything()
  } else if (event.key === 'Escape') {
    selection.value = emptySelection()
  }
}

/**
 * Valeur de la liste « colonne filtrée » : -1 pour toutes les colonnes.
 */
const filterColumn = computed({
  get: () => filter.column ?? -1,
  set: (value: number | undefined) => {
    filter.column = value === undefined || value < 0 ? null : value
  },
})
</script>

<template>
  <div class="result-grid">
    <div v-if="filterOpen" ref="filterBox" class="grid-toolbar">
      <ui-input
        v-model="filter.text"
        :icon="Search"
        placeholder="Filtrer"
        aria-label="Texte cherché"
        class="grid-filter"
      >
        <template v-if="filter.text !== ''" #trailing>
          <ui-button
            :icon="X"
            variant="ghost"
            small
            aria-label="Vider le filtre"
            @click="filter.text = ''"
          />
        </template>
      </ui-input>
      <ui-select
        v-model="filterColumn"
        :items="filterOptions"
        label="Colonne filtrée"
        class="grid-filter-column"
      />
    </div>

    <div ref="scroller" class="grid-scroller" tabindex="0" @keydown="onKeydown">
      <div class="grid-row grid-header" :style="{ gridTemplateColumns: template }">
        <div class="grid-corner" title="Tout sélectionner" @mousedown.prevent="selectEverything" />
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
            :title="headerTitle(column)"
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
            :title="`Trier par ${columns[column]}`"
            @click="toggleSort(column)"
          >
            {{ sort?.column === column ? (sort.direction === 'asc' ? '▲' : '▼') : '⇅' }}
          </button>
          <div
            class="grid-resizer"
            title="Glisser pour changer la largeur, double clic pour l’ajuster au contenu ou revenir à la largeur estimée"
            @pointerdown.stop.prevent="onResizeStart($event, column)"
            @pointermove="onResizeMove"
            @pointerup="onResizeEnd"
            @pointercancel="onResizeEnd"
            @dblclick="fitWidth(column)"
          />
        </div>
      </div>

      <div class="grid-body" :style="{ height: `${virtualizer.getTotalSize()}px` }">
        <div
          v-for="item in virtualizer.getVirtualItems()"
          :key="item.index"
          class="grid-row"
          :style="{ gridTemplateColumns: template, transform: `translateY(${item.start}px)` }"
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
            :title="cellAt(item.index, column) ?? NULL_TEXT"
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
              ><ExternalLink class="grid-link-icon" aria-hidden="true" />{{
                cellAt(item.index, column)
              }}</a
            >
            <template v-else>{{ cellAt(item.index, column) ?? NULL_TEXT }}</template>
          </div>
        </div>
      </div>
    </div>

    <div class="grid-footer">
      <ui-button
        :icon="ListFilter"
        variant="ghost"
        :tone="filter.text === '' ? 'neutral' : 'accent'"
        small
        :aria-pressed="filterOpen"
        :aria-label="filterOpen ? 'Replier le filtre' : 'Filtrer les lignes'"
        :title="filterOpen ? 'Replier le filtre' : 'Filtrer les lignes'"
        class="grid-filter-toggle"
        @click="toggleFilter"
      />
      <!-- L'emplacement suit le nombre dans le même texte, sans l'écart du conteneur. -->
      <span class="grid-count">{{ countLabel }}<slot name="footer" /></span>
    </div>
  </div>
</template>

<style scoped>
.result-grid {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: var(--gap);
  min-height: 0;
}

.grid-toolbar {
  display: flex;
  align-items: center;
  gap: var(--gap);
}

.grid-filter {
  max-inline-size: var(--grid-filter-width);
}

.grid-filter-column {
  max-inline-size: var(--grid-filter-column-width);
}

.grid-footer {
  display: flex;
  align-items: center;
  gap: var(--gap);
  color: var(--fg-muted);
  font-size: var(--font-size-ui);

  & > span {
    text-box: trim-both cap alphabetic;
  }
}

.grid-scroller {
  position: relative;
  flex: 1;
  min-block-size: var(--grid-min-height);
  /* Garde-fou : une zone de défilement plus haute que la fenêtre dessinerait
     toutes les lignes et figerait l'interface. */
  max-block-size: 100vh;
  overflow: auto;
  border: var(--line-width) solid var(--line);
  font-size: var(--font-size-ui);
  outline: none;
  user-select: none;
}

.grid-row {
  display: grid;
  block-size: var(--grid-row-height);
}

/*
 * Cellules, en-têtes et numéros de ligne : le texte est centré sur ses
 * capitales (`text-box`) au milieu de la ligne (`align-content` sur un bloc).
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
  font-weight: 600;
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

.grid-corner,
.grid-row-header {
  background: var(--surface-elevated);
  color: var(--fg-dimmed);
  text-align: end;
}

.grid-row-header {
  padding-inline: var(--grid-cell-padding-x);
  cursor: e-resize;
}

.grid-corner {
  cursor: se-resize;
}

.grid-column-header {
  position: relative;
  display: flex;
  min-inline-size: 0;
  border-inline-start: var(--line-width) solid var(--line);

  /* Seule la dernière colonne porte une bordure droite : les autres sont
     fermées par la bordure gauche de leur voisine. */
  &:last-child {
    border-inline-end: var(--line-width) solid var(--line);
  }

  &.drop-before {
    box-shadow: inset var(--focus-width) 0 0 var(--accent);
  }

  &.drop-after {
    box-shadow: inset calc(-1 * var(--focus-width)) 0 0 var(--accent);
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
  gap: var(--grid-type-gap);
  min-inline-size: 0;
  padding-inline: var(--grid-cell-padding-x);
  white-space: nowrap;
  cursor: grab;
}

/* Icône neutre de la famille du type, devant le nom. */
.grid-type-icon {
  flex: none;
  align-self: center;
  inline-size: var(--icon-size-sm);
  block-size: var(--icon-size-sm);
  color: var(--fg-dimmed);
}

/* Marque en texte (123, .00), de la hauteur des icônes. */
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
 * Le nom occupe toute la hauteur de l'en-tête et y centre ses capitales
 * (`align-content`) : rogné à sa hauteur de capitales, il perdrait ses
 * jambages (« q », « _ »).
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
  padding-inline: var(--grid-cell-padding-x);
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
  padding-inline: var(--grid-cell-padding-x);
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

  /* Texte de longueur fixe (CHAR) : codes et références, en chasse fixe. */
  &.mono {
    font-family: var(--font-code);
  }

  /* NULL en petites capitales : plus discret qu'une valeur de la colonne. */
  &.null {
    color: var(--fg-dimmed);
    font-style: italic;
    font-variant-caps: all-small-caps;
  }
}

.selected {
  background: var(--selection);
}

/* Adresse web d'une colonne de liens. */
.grid-link {
  color: var(--accent);
  text-decoration: none;
  cursor: pointer;

  &:hover {
    text-decoration: underline;
  }
}

/* Icône de lien externe, devant l'adresse. */
.grid-link-icon {
  display: inline-block;
  inline-size: var(--icon-size-sm);
  block-size: var(--icon-size-sm);
  margin-inline-end: 0.25em;
  vertical-align: -0.1em;
}
</style>
