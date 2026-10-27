<!--
  Contents of a line of the object navigator: the chevron of a level that can
  open, the icon of its level, its label (or the three columns of a column),
  the flavor badge of a source and its "⋮" menu, the button that acts on a
  database (makes it the current one), a table (shows its content) or an
  index (writes the operations on it), as Ctrl+click does (emits `act`). On
  a source, and on the current database instead of its plug, the button
  closes the session of the active editor (emits `disconnect`), enabled once
  that session is ready on this source. Shared by the lines of the
  tree and by the lines pinned at the top of the tree while it scrolls
  (`ObjectNavigator.vue`), which only differ by their container.
-->
<script setup lang="ts">
import {
  Cable,
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
  Notebook,
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
import UiButton from './ui/UiButton.vue'
import UiDropdownMenu from './ui/UiDropdownMenu.vue'
import type { MenuItem } from './ui/menuItem'
import { vTooltip } from './ui/tooltip'

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
}>()

/**
 * Tooltip of the disconnect button, by stage of the active editor's session.
 *
 * @param session Stage of the session on the line's source, `null` without one.
 */
function disconnectTip(session: SessionStatus | null): string {
  if (session === 'ready') {
    return 'Ferme la session de l’éditeur actif sur ce serveur'
  }
  if (session === 'running') {
    return 'Déconnexion impossible pour le moment : arrêtez d’abord les requêtes en cours sur cette connexion'
  }
  return 'Aucune session ouverte sur cette source par l’éditeur actif'
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
  if (props.node.role === 'database') {
    return {
      icon: DatabaseZap,
      label: 'Utiliser cette base',
      tooltip: 'Utiliser cette base (USE)',
    }
  }
  if (props.node.table !== undefined) {
    return { icon: Rows3, label: 'Afficher le contenu', tooltip: 'Afficher le contenu (SELECT)' }
  }
  if (props.node.index !== undefined) {
    return {
      icon: PencilRuler,
      label: 'Opérations sur l’index',
      tooltip: 'Écrire les opérations sur l’index (ALTER TABLE), en commentaire',
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
 * Icon of a node, before its label: each level has its own (roots, source,
 * database, open or closed folder, table, view, column...).
 *
 * @param node Node of the tree.
 * @param expanded Is the node open?
 */
function nodeIcon(node: NavigatorNode, expanded: boolean): Component {
  switch (node.role) {
    case 'sources':
      return Server
    case 'source':
      return Cable
    case 'database':
      return Database
    case 'notebooks':
      return Notebook
  }
  if (node.entry === undefined || node.entry === 'folder') {
    return expanded ? FolderOpen : Folder
  }
  return ENTRY_ICONS[node.entry]
}
</script>

<template>
  <component
    :is="expanded ? ChevronDown : ChevronRight"
    v-if="node.children?.length"
    class="size-icon-sm shrink-0 text-fg-dimmed"
  />
  <span v-else class="size-icon-sm shrink-0" />
  <component
    :is="nodeIcon(node, expanded)"
    class="size-icon shrink-0 text-fg-muted"
    :class="{ 'animate-spin': node.entry === 'loading' }"
  />

  <!-- Column: symbols, name (bold if indexed), type and nullability. -->
  <span v-if="node.column" class="tree-label flex min-w-0 flex-1 items-center">
    <span
      v-if="node.column.markSlots > 0"
      class="inline-flex flex-none text-fg-muted"
      :style="{ width: `${node.column.markSlots}em` }"
    >
      <KeyRound v-if="node.column.primaryKey" class="size-[1em]" aria-label="Clef primaire" />
      <Link v-if="node.column.foreignKey" class="size-[1em]" aria-label="Clef étrangère" />
    </span>
    <span class="flex min-w-0 flex-1 items-center gap-3">
      <span
        class="flex-none font-bold text-trim"
        :style="{ minWidth: `${node.column.nameWidth}ch` }"
      >
        <span :class="{ 'font-normal': !node.column.indexed }">
          {{ node.column.name }}
        </span>
      </span>
      <span class="truncate text-small text-fg-dimmed text-trim" v-tooltip="node.column.details">
        {{ node.column.details }}
      </span>
    </span>
  </span>
  <!--
    Other nodes: label, then the note if any (UNIQUE, chargement…), and for a
    source the badge of the server's flavor, right after the name, whose
    tooltip gives the name and version of the server. The whole takes the
    room left, which pushes the menu to the end of the line.
  -->
  <span v-else class="flex min-w-0 flex-1 items-center gap-2">
    <span
      class="tree-label flex min-w-0 items-center gap-3"
      :class="{
        'current-database': node.current,
        'font-bold': node.current || node.role === 'sources' || node.role === 'notebooks',
      }"
    >
      <span class="truncate text-trim">{{ node.label }}</span>
      <span v-if="node.note" class="shrink-0 text-small font-normal text-fg-dimmed text-trim">
        {{ node.note }}
      </span>
    </span>
    <span
      v-if="node.badge"
      v-tooltip="node.badgeTooltip"
      class="flavor-badge inline-flex h-4 shrink-0 items-center rounded-full px-2 text-small font-medium"
      :data-flavor="node.badge"
    >
      <span class="text-trim">{{ node.badge }}</span>
    </span>
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

<style scoped>
/* Badge of the server's flavor: pill tinted in the engine's colors. */
.flavor-badge {
  &[data-flavor='mysql'] {
    background: var(--badge-mysql-bg);
    color: var(--badge-mysql);
  }

  &[data-flavor='mariadb'] {
    background: var(--badge-mariadb-bg);
    color: var(--badge-mariadb);
  }
}
</style>
