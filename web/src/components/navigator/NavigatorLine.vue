<!--
  Contents of a line of the object navigator: the chevron of a level that can
  open, the icon of its level, its label (or the three columns of a column),
  the flavor badge of a source and its "⋮" menu, the button that acts on a
  database (makes it the current one), a table (shows its content) or an
  index (writes the operations on it), as Ctrl+click does (emits `act`). On
  a source, and on the current database instead of its plug, the button
  closes the session of the active editor (emits `disconnect`), enabled once
  that session is ready on this source. A "Tables" folder of two tables or
  more has two buttons instead, which sort its tables by name or by row
  count (emits `sort-tables`), the one pressed showing the order. Shared by
  the lines of the tree and by the lines pinned at the top of the tree while
  it scrolls (`NavigatorPanel.vue`), which only differ by their container.
-->
<script setup lang="ts">
import {
  ArrowDownWideNarrow,
  ArrowUpAZ,
  CalendarClock,
  ChevronDown,
  ChevronRight,
  Columns3,
  Database,
  DatabaseZap,
  EllipsisVertical,
  Eye,
  Folder,
  FolderOpen,
  KeyRound,
  Link,
  ListOrdered,
  LoaderCircle,
  PencilRuler,
  Rows3,
  ScrollText,
  Server,
  SquareFunction,
  Table,
  Unplug,
  Zap,
} from '@lucide/vue'
import { type Component, computed } from 'vue'

import type { SessionStatus } from '@/api/querySession'

import type { EntryKind, NavigatorNode } from './navigatorNodes'
import type { TableSort } from '@/stores/navigator'
import ConnectionFlavorBadge from '@/components/connections/ConnectionFlavorBadge.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiDropdownMenu from '@/components/ui/UiDropdownMenu.vue'
import type { MenuItem } from '@/components/ui/menuItem'
import { type Tooltip, vTooltip } from '@/components/ui/tooltip'

const props = defineProps<{
  /**
   * Node shown on the line.
   */
  node: NavigatorNode
  /**
   * Is the node open? Changes its chevron and the icon of a folder.
   */
  expanded: boolean
  /**
   * Choices of the "⋮" menu, `null` for a node without a menu.
   */
  menu: MenuItem[] | null
  /**
   * Stage of the active editor's session, when the node belongs to its
   * source; `null` otherwise.
   */
  session?: SessionStatus | null
}>()

const emit = defineEmits<{
  /**
   * The button of the line asks to act on its node, as Ctrl+click does.
   */
  act: []
  /**
   * The button of the line asks to close the session of the active editor.
   */
  disconnect: []
  /**
   * A button of a "Tables" folder asks to sort its tables.
   */
  'sort-tables': [sort: TableSort]
}>()

/**
 * Buttons of a "Tables" folder, one per order of its tables.
 */
const TABLE_SORTS = [
  { sort: 'name', icon: ArrowUpAZ, label: 'Trier par nom' },
  { sort: 'rows', icon: ArrowDownWideNarrow, label: 'Trier par nombre de lignes' },
] as const

/**
 * Tooltip of the disconnect button, by stage of the active editor's session.
 *
 * @param session Stage of the session on the line's source, `null` without one.
 */
function disconnectTip(session: SessionStatus | null): Tooltip {
  if (session === 'ready') {
    return {
      title: 'Déconnecter',
      description: 'Ferme la session de l’éditeur actif sur ce serveur',
    }
  }
  if (session === 'running') {
    return {
      title: 'Déconnecter',
      description: 'Arrêtez d’abord les requêtes en cours sur cette connexion',
    }
  }
  return {
    title: 'Déconnecter',
    description: 'Aucune session ouverte sur cette source par l’éditeur actif',
  }
}

/**
 * Button of the line, shown on hover: on a source or the current database,
 * closes the session of the active editor; on another database, makes it the
 * current database; on a table or a view, shows its content; on an index,
 * writes the operations on it. `null` for the other nodes.
 */
const action = computed(() => {
  if (props.node.role === 'source' || (props.node.role === 'database' && props.node.current)) {
    const session = props.session ?? null
    return {
      icon: Unplug,
      label: 'Déconnecter',
      tooltip: disconnectTip(session),
      disabled: session !== 'ready',
      event: 'disconnect' as const,
    }
  }
  if (props.node.role === 'database' && props.node.usable === true) {
    return {
      icon: DatabaseZap,
      label: 'Utiliser cette base',
      tooltip: { title: 'Utiliser cette base', description: 'Base courante de la session (USE)' },
    }
  }
  if (props.node.table !== undefined) {
    return {
      icon: Rows3,
      label: 'Afficher le contenu',
      tooltip: { title: 'Afficher le contenu', description: 'Par une requête SELECT' },
    }
  }
  if (props.node.index !== undefined) {
    return {
      icon: PencilRuler,
      label: 'Opérations sur l’index',
      tooltip: {
        title: 'Écrire les opérations sur l’index',
        description: 'ALTER TABLE, en commentaire',
      },
    }
  }
  return null
})

/**
 * Icon of the catalog nodes, according to their nature (folder apart: its
 * icon depends on its opening).
 */
const ENTRY_ICONS: Record<Exclude<EntryKind, 'folder'>, Component> = {
  table: Table,
  view: Eye,
  procedure: ScrollText,
  event: CalendarClock,
  function: SquareFunction,
  column: Columns3,
  index: ListOrdered,
  'foreign-key': Link,
  trigger: Zap,
  loading: LoaderCircle,
}

/**
 * Icon of a node, before its label: each level has its own (source,
 * database, open or closed folder, table, view, column...), a spinning
 * circle while a source's catalog is being received. A column has a
 * single icon, which tells its key: a key for the primary key, a link for a
 * foreign key, the column icon otherwise.
 *
 * @param node Node of the tree.
 * @param expanded Is the node open?
 */
function nodeIcon(node: NavigatorNode, expanded: boolean): Component {
  switch (node.role) {
    case 'source':
      // The icon of its kind of source: every connection is a database
      // server for now; another kind (CSV) will get its own here. A spinning
      // circle replaces it while its catalog is being received.
      return node.receiving ? LoaderCircle : Server
    case 'database':
      return Database
  }
  if (node.column?.primaryKey) {
    return KeyRound
  }
  if (node.column?.foreignKey) {
    return Link
  }
  if (node.entry === undefined || node.entry === 'folder') {
    return expanded ? FolderOpen : Folder
  }
  return ENTRY_ICONS[node.entry]
}

/**
 * Name of a column's key icon and of a source's spinning circle, for screen
 * readers; none for the other icons, which repeat the label.
 *
 * @param node Node of the tree.
 */
function iconLabel(node: NavigatorNode): string | undefined {
  if (node.receiving) {
    return 'Chargement'
  }
  if (node.column?.primaryKey) {
    return 'Clef primaire'
  }
  return node.column?.foreignKey ? 'Clef étrangère' : undefined
}
</script>

<template>
  <component
    :is="expanded ? ChevronDown : ChevronRight"
    v-if="node.children?.length"
    class="size-4 shrink-0 text-fg-faint"
  />
  <span v-else class="size-4 shrink-0" />
  <component
    :is="nodeIcon(node, expanded)"
    class="size-5 shrink-0 text-fg-secondary"
    :aria-label="iconLabel(node)"
    :class="{ 'animate-spin': node.entry === 'loading' || node.receiving }"
  />

  <!-- Column: name (bold if indexed), type and nullability. -->
  <span v-if="node.column" class="tree-label flex min-w-0 flex-1 items-center">
    <span class="flex min-w-0 flex-1 items-center gap-4">
      <span
        class="flex-none font-bold text-trim"
        :style="{ minWidth: `${node.column.nameWidth}ch` }"
      >
        <span :class="{ 'font-normal': !node.column.indexed }">
          {{ node.column.name }}
        </span>
      </span>
      <span class="truncate text-sm text-fg-faint text-trim" v-tooltip="node.column.details">
        {{ node.column.details }}
      </span>
    </span>
  </span>
  <!--
    Other nodes: label, then the note if any (UNIQUE, row count of a table,
    its digits of equal width), and for a source the badge of the server's
    flavor, right after the name, whose tooltip gives the name and version
    of the server. The whole takes the room left, which pushes the menu to
    the end of the line.
  -->
  <span v-else class="flex min-w-0 flex-1 items-center gap-3">
    <span
      class="tree-label flex min-w-0 items-center gap-4"
      :class="{
        'current-database': node.current,
        'chosen-source': node.chosen,
        'font-bold': node.current || node.chosen,
      }"
    >
      <span class="truncate text-trim">{{ node.label }}</span>
      <span
        v-if="node.note"
        class="shrink-0 text-sm font-normal text-fg-faint tabular-nums text-trim"
      >
        {{ node.note }}
      </span>
    </span>
    <connection-flavor-badge
      v-if="node.badge"
      :flavor="node.badge"
      :version="node.badgeVersion ?? null"
    />
  </span>

  <!--
    "Tables" folder: its two orders, shown on hovering; the order by row
    count stays shown when chosen, as it explains the order of the list.
  -->
  <span
    v-if="node.tableSort"
    role="group"
    aria-label="Trier les tables"
    class="flex shrink-0 items-center"
  >
    <ui-button
      v-for="choice in TABLE_SORTS"
      :key="choice.sort"
      :icon="choice.icon"
      variant="ghost"
      small
      :tone="node.tableSort === choice.sort ? 'primary' : 'neutral'"
      :aria-pressed="node.tableSort === choice.sort"
      :aria-label="choice.label"
      :class="{
        'invisible group-hover:visible group-focus-visible:visible':
          choice.sort === 'name' || node.tableSort !== 'rows',
      }"
      @click.stop="node.tableSort !== choice.sort && emit('sort-tables', choice.sort)"
      @keydown.stop
    />
  </span>

  <!--
    On hovering a database, a table or an index: acts on it, like Ctrl+click;
    on a source or the current database: closes the active editor's session.
  -->
  <ui-button
    v-if="action"
    v-tooltip="action.tooltip"
    :icon="action.icon"
    variant="ghost"
    small
    :aria-label="action.label"
    :disabled="'disabled' in action && action.disabled"
    class="invisible group-hover:visible group-focus-visible:visible"
    @click.stop="'event' in action ? emit('disconnect') : emit('act')"
    @keydown.stop
  />

  <ui-dropdown-menu v-if="menu" :items="menu">
    <ui-button
      :icon="EllipsisVertical"
      variant="ghost"
      small
      aria-label="Actions"
      class="node-menu invisible group-hover:visible group-focus-visible:visible data-[state=open]:visible"
      @click.stop
      @keydown.stop
    />
  </ui-dropdown-menu>
</template>
