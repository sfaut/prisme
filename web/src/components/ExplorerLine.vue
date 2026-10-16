<!--
  Contents of a line of the object explorer: the chevron of a level that can
  open, the icon of its level, its label (or the three columns of a column),
  the flavor badge of a source and its "⋮" menu. Shared by the lines of the
  tree and by the lines pinned at the top of the tree while it scrolls
  (`ObjectExplorer.vue`), which only differ by their container.
-->
<script setup lang="ts">
import {
  Cable,
  ChevronDown,
  ChevronRight,
  Columns3,
  Database,
  EllipsisVertical,
  Eye,
  Folder,
  FolderOpen,
  Hash,
  KeyRound,
  Link,
  ListOrdered,
  LoaderCircle,
  Notebook,
  ScrollText,
  Server,
  SquareFunction,
  Table,
  Zap,
} from 'lucide-vue-next'
import type { Component } from 'vue'

import type { EntryKind, ExplorerNode } from './explorerNodes'
import UiButton from './ui/UiButton.vue'
import UiDropdownMenu from './ui/UiDropdownMenu.vue'
import type { MenuItem } from './ui/menuItem'

defineProps<{
  /**
   * Node shown on the line.
   */
  node: ExplorerNode
  /**
   * Is the node open? Changes its chevron and the icon of a folder.
   */
  expanded: boolean
  /**
   * Choices of the "⋮" menu, `null` for a node without a menu.
   */
  menu: MenuItem[] | null
}>()

/**
 * Icon of the catalog nodes, according to their nature (folder apart: its
 * icon depends on its opening).
 */
const ENTRY_ICONS: Record<Exclude<EntryKind, 'folder'>, Component> = {
  table: Table,
  view: Eye,
  procedure: ScrollText,
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
function nodeIcon(node: ExplorerNode, expanded: boolean): Component {
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
      <Hash v-if="node.column.foreignKey" class="size-[1em]" aria-label="Clef étrangère" />
    </span>
    <span
      class="me-3 flex-none font-bold text-trim"
      :style="{ minWidth: `${node.column.nameWidth}ch` }"
    >
      <span :class="{ 'font-normal': !node.column.indexed }">
        {{ node.column.name }}
      </span>
    </span>
    <span class="truncate text-small text-fg-dimmed text-trim" :title="node.column.details">
      {{ node.column.details }}
    </span>
  </span>
  <!-- Other nodes: label, then the note if any (UNIQUE, chargement…). -->
  <span
    v-else
    class="tree-label min-w-0 truncate text-trim"
    :class="{
      'flex-1': !node.badge,
      'current-database': node.current,
      'font-bold': node.current || node.role === 'sources' || node.role === 'notebooks',
    }"
  >
    {{ node.label }}
    <span v-if="node.note" class="ms-3 text-small font-normal text-fg-dimmed">
      {{ node.note }}
    </span>
  </span>
  <!-- Source: badge of the server's flavor, right after the name. -->
  <span
    v-if="node.badge"
    class="flavor-badge ms-0.5 me-auto shrink-0 text-trim"
    :data-flavor="node.badge"
  >
    {{ node.badge }}
  </span>

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
  padding: var(--badge-padding);
  border-radius: var(--badge-radius);
  font-size: var(--badge-font-size);
  font-weight: 500;
  letter-spacing: 0.03em;

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
