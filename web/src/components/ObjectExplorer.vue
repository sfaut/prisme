<!--
  Object explorer: under the collapsible "Sources de données" root, one
  branch per source (saved connection), then its databases, tables and
  columns; next, the "Notebooks" root, empty for now.

  Several sources sit side by side in the same tree. The catalog of a source
  is read when it is opened (that of the chosen connection opens by itself)
  and arrives progressively: the databases first, then their objects (tables,
  views, procedures, functions), then the details of the tables (columns,
  indexes, foreign keys, triggers), each part shown as soon as it arrives.
  It then stays in memory. A click on a line opens or closes its level. A
  double click on a source chooses it as the working connection (emits
  `select`); it stays highlighted. A double click on a database makes it the
  current database, like `USE` (emits `use-database`). A double click on a
  table or a view emits `open-table`, to show its content.

  Each source carries as a badge the flavor of its server (`mysql`,
  `mariadb`), seen on the last connection or when reading the catalog.

  Each line carries the icon of its level: roots, source, database, folder
  (open or closed), table, view, procedure, function, column, index, foreign
  key, trigger, and a spinning circle during a load.

  On hovering the root, a "⋮" menu on the right adds a connection or reads
  again all the catalogs already read; on hovering each source, another one
  changes or tests the connection, reads its catalog again or deletes the
  connection (after confirmation).

  The tree is Reka UI's (`TreeRoot`, `TreeItem`: keyboard, opening of the
  levels, accessibility), with virtual scrolling (`TreeVirtualizer`); the
  rendering of the lines is Prisme's, in Tailwind classes. All the lines
  have the same height (`--row-height`), and each level is indented by
  `--tree-indent` (variables of `main.css`).

  While the tree scrolls, the ancestors of the first line under the top stay
  pinned there (`stickyRows.ts`): the source, the database and the table keep
  showing over their columns. The pinned lines are the lines of the tree
  (`ExplorerLine.vue`), hidden from assistive technologies, which read the tree.
-->
<script setup lang="ts">
import { NotebookPen, Pencil, PlugZap, Plus, RefreshCw, Trash2 } from 'lucide-vue-next'
import { TreeItem, type TreeItemSelectEvent, TreeRoot, TreeVirtualizer } from 'reka-ui'
import { computed, onMounted, ref, watch } from 'vue'

import { probeConnection, serverLabel } from '@/api/connections'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import { cssPixels } from '@/cssLength'
import { useCatalogStore } from '@/stores/catalog'
import { useConnectionsStore } from '@/stores/connections'

import ConnectionDialog from './ConnectionDialog.vue'
import ExplorerLine from './ExplorerLine.vue'
import { type ExplorerNode, SOURCES_KEY, explorerTree, sourceKey } from './explorerNodes'
import { pinnedRows, visibleRows } from './stickyRows'
import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import UiDialog from './ui/UiDialog.vue'
import type { MenuItem } from './ui/menuItem'

const props = defineProps<{
  /**
   * Id of the chosen connection, `null` if none.
   */
  selectedId: number | null
  /**
   * Current database of the chosen connection's query session, shown in
   * bold; `null` if none.
   */
  currentDatabase: string | null
}>()

const emit = defineEmits<{
  /**
   * The user wants to see the content of a table.
   */
  'open-table': [connectionId: number, schema: string, table: string]
  /**
   * The user chooses a source as the working connection.
   */
  select: [connectionId: number]
  /**
   * The user chooses a database as the current database (`USE`).
   */
  'use-database': [connectionId: number, schema: string]
}>()

const catalog = useCatalogStore()
const connections = useConnectionsStore()

/**
 * Height of a line, in pixels, for virtual scrolling
 * (`--row-height` of the theme).
 */
const ROW_HEIGHT = cssPixels('--row-height', 24)

/**
 * Connections whose catalog is being read.
 */
const loadingIds = ref<number[]>([])

/**
 * Error reading the list of connections, `null` if none.
 */
const listError = ref<string | null>(null)

/**
 * Result of the last successful test of a source (name, flavor and version
 * of the server), `null` if none.
 */
const probed = ref<string | null>(null)

/**
 * Last reading error of each connection, by id.
 */
const failures = ref<Record<number, string>>({})

/**
 * Is the creation or change window open?
 */
const dialogOpen = ref(false)

/**
 * Connection changed by the window, `null` to create one.
 */
const editing = ref<ConnectionSummary | null>(null)

/**
 * Connection whose deletion waits for confirmation, `null` if none.
 */
const removing = ref<ConnectionSummary | null>(null)

/**
 * Is the deletion confirmation window open?
 */
const removeOpen = computed({
  get: () => removing.value !== null,
  set: (open) => {
    if (!open) {
      removing.value = null
    }
  },
})

/**
 * Keys of the open nodes.
 */
const expandedKeys = ref<string[]>([SOURCES_KEY])

/**
 * Is a catalog still being received?
 */
const receiving = computed(() =>
  connections.connections.some((connection) => {
    const current = catalog.catalogOf(connection.id)
    return current !== undefined && !current.done
  }),
)

/**
 * Whole tree, built from the catalogs in memory.
 */
const nodes = computed(() =>
  explorerTree(
    connections.connections.map((connection) => ({
      connectionId: connection.id,
      name: connection.name,
      catalog: catalog.catalogOf(connection.id),
      currentDatabase: connection.id === props.selectedId ? props.currentDatabase : null,
      flavor: catalog.catalogOf(connection.id)?.flavor ?? connection.server_flavor,
      showSystemSchemas: connection.show_system_schemas,
    })),
    receiving.value,
  ),
)

/**
 * Rows displayed by the tree, in display order.
 */
const rows = computed(() => visibleRows(nodes.value, expandedKeys.value))

/**
 * Position of the tree's scroll, in pixels.
 */
const scrollTop = ref(0)

/**
 * Ancestors of the rows under the top of the tree, pinned there while it
 * scrolls, so that the source, the database and the table stay visible.
 */
const pinned = computed(() => pinnedRows(rows.value, scrollTop.value, ROW_HEIGHT))

/**
 * Classes of a line of the tree, and of a pinned line, which look the same.
 */
const ROW_CLASSES =
  'tree-row group flex h-row w-full cursor-pointer items-center gap-1 rounded-control pe-1 text-start outline-none select-none hover:bg-surface-muted focus-visible:bg-surface-muted'

/**
 * Classes of the line of a node: the chosen source is highlighted.
 *
 * @param node Node shown on the line.
 */
function rowClass(node: ExplorerNode): (string | Record<string, boolean>)[] {
  return [
    ROW_CLASSES,
    { 'bg-surface-elevated': node.role === 'source' && node.connectionId === props.selectedId },
  ]
}

/**
 * Errors to show, preceded by the name of their source: reading failures,
 * then errors reported by the server during the reception.
 */
const messages = computed(() =>
  connections.connections.flatMap((connection) => {
    const failure = failures.value[connection.id]
    const errors = catalog.catalogOf(connection.id)?.errors ?? []
    return [...(failure === undefined ? [] : [failure]), ...errors].map(
      (message) => `${connection.name} : ${message}`,
    )
  }),
)

/**
 * Reads the catalog of a connection, from memory or from the server.
 *
 * @param connectionId Id of the connection.
 * @param refresh Reads the catalog again from the server even if it is in memory.
 */
async function load(connectionId: number, refresh: boolean): Promise<void> {
  if (loadingIds.value.includes(connectionId)) {
    return
  }
  loadingIds.value.push(connectionId)
  delete failures.value[connectionId]
  try {
    await catalog.load(connectionId, refresh)
  } catch (failure) {
    failures.value[connectionId] = failure instanceof Error ? failure.message : String(failure)
  } finally {
    loadingIds.value = loadingIds.value.filter((id) => id !== connectionId)
  }
}

/**
 * Reads again the catalogs already read.
 */
function refreshAll(): void {
  for (const connection of connections.connections) {
    if (catalog.catalogs[connection.id] !== undefined) {
      void load(connection.id, true)
    }
  }
}

/**
 * Follows the opening and closing of the nodes: opening a source reads its catalog.
 *
 * @param keys Keys of the nodes now open.
 */
function expand(keys: string[]): void {
  expandedKeys.value = keys
  for (const connection of connections.connections) {
    if (keys.includes(sourceKey(connection.id))) {
      void load(connection.id, false)
    }
  }
}

/**
 * Choices of a node's "⋮" menu, `null` for a node without a menu.
 *
 * @param node Node of the tree.
 */
function menuItems(node: ExplorerNode): MenuItem[] | null {
  const connection = connections.connections.find((item) => item.id === node.connectionId) ?? null
  switch (node.role) {
    case 'sources':
      return [
        { label: 'Ajouter une connexion', icon: Plus, onSelect: () => openDialog(null) },
        { label: 'Tout rafraîchir', icon: RefreshCw, onSelect: refreshAll },
      ]
    case 'notebooks':
      // Notebooks are not saved yet (stage 3 of the v1).
      return [{ label: 'Créer un notebook', icon: NotebookPen, disabled: true }]
    case 'source':
      return [
        { label: 'Modifier', icon: Pencil, onSelect: () => openDialog(connection) },
        {
          label: 'Rafraîchir',
          icon: RefreshCw,
          onSelect: () => void load(node.connectionId, true),
        },
        {
          label: 'Tester',
          icon: PlugZap,
          onSelect: () => void probe(node.connectionId),
        },
        {
          label: 'Supprimer',
          icon: Trash2,
          tone: 'error',
          onSelect: () => {
            removing.value = connection
          },
        },
      ]
    default:
      return null
  }
}

/**
 * Opens the connection window.
 *
 * @param connection Connection to change, `null` to create one.
 */
function openDialog(connection: ConnectionSummary | null): void {
  editing.value = connection
  dialogOpen.value = true
}

/**
 * Connects to the server of a source and shows its flavor and version.
 * A failure shows as a reading error of the source.
 *
 * @param connectionId Id of the connection.
 */
async function probe(connectionId: number): Promise<void> {
  const name = connections.connections.find((item) => item.id === connectionId)?.name
  probed.value = null
  delete failures.value[connectionId]
  try {
    probed.value = `${name} : ${serverLabel(await probeConnection(connectionId))}`
  } catch (failure) {
    failures.value[connectionId] = failure instanceof Error ? failure.message : String(failure)
  }
}

/**
 * Reads again the catalog of a changed connection, if it was already read:
 * the connection may now target another server.
 *
 * @param connectionId Id of the changed connection.
 */
function onUpdated(connectionId: number): void {
  if (catalog.catalogs[connectionId] !== undefined) {
    void load(connectionId, true)
  }
}

/**
 * Deletes the connection that was waiting for confirmation.
 */
async function confirmRemove(): Promise<void> {
  const connection = removing.value
  removing.value = null
  if (connection === null) {
    return
  }
  try {
    await connections.remove(connection.id)
    expandedKeys.value = expandedKeys.value.filter((key) => key !== sourceKey(connection.id))
    delete failures.value[connection.id]
  } catch (failure) {
    failures.value[connection.id] = failure instanceof Error ? failure.message : String(failure)
  }
}

/**
 * Follows the clicks on the lines. The tree keeps no selection (the
 * highlighted source is that of the chosen connection); the click opens or
 * closes the level. The second click of a double click chooses the source,
 * changes the database or opens the table.
 *
 * @param event Selection requested by the tree, with the original click.
 * @param node Clicked node.
 */
function onSelect(event: TreeItemSelectEvent<ExplorerNode>, clicked: ExplorerNode): void {
  event.preventDefault()
  const original = event.detail.originalEvent
  if (!(original instanceof MouseEvent) || original.detail !== 2) {
    return
  }
  activate(clicked)
}

/**
 * Acts on a node after a double click: chooses a source, changes the current
 * database or opens the table.
 *
 * @param node Double-clicked node.
 */
function activate(node: ExplorerNode): void {
  if (node.role === 'source') {
    emit('select', node.connectionId)
  } else if (node.role === 'database') {
    emit('use-database', node.connectionId, node.schema)
  } else if (node.table !== undefined) {
    emit('open-table', node.connectionId, node.schema, node.table)
  }
}

/**
 * Follows the clicks on a pinned line, as the tree does: a click opens or
 * closes its level, and a double click acts on it.
 *
 * @param event Original click.
 * @param clicked Node of the pinned line.
 */
function onPinnedClick(event: MouseEvent, clicked: ExplorerNode): void {
  if (clicked.children?.length) {
    expand(
      expandedKeys.value.includes(clicked.key)
        ? expandedKeys.value.filter((key) => key !== clicked.key)
        : [...expandedKeys.value, clicked.key],
    )
  }
  if (event.detail === 2) {
    activate(clicked)
  }
}

/**
 * Follows the scroll of the tree, to pin the rows of its hierarchy.
 *
 * @param event Scroll event of the tree.
 */
function onScroll(event: Event): void {
  scrollTop.value = (event.target as HTMLElement).scrollTop
}

onMounted(async () => {
  try {
    await connections.load()
  } catch (failure) {
    listError.value = `Connexions illisibles : ${failure instanceof Error ? failure.message : String(failure)}`
  }
})

// The chosen connection opens its source.
watch(
  () => props.selectedId,
  (connectionId) => {
    if (connectionId === null) {
      return
    }
    if (!expandedKeys.value.includes(sourceKey(connectionId))) {
      expandedKeys.value = [...expandedKeys.value, sourceKey(connectionId)]
    }
    // Directly: the list of connections may not be received yet.
    void load(connectionId, false)
  },
  { immediate: true },
)
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col gap-gap">
    <ui-alert v-if="probed" tone="success" :title="probed" closable @close="probed = null" />
    <ui-alert v-if="listError" tone="error" :title="listError" />
    <ui-alert v-for="message in messages" :key="message" tone="warning" :title="message" />

    <div class="relative flex min-h-0 flex-1 flex-col">
      <tree-root
        :items="nodes"
        :get-key="(node: ExplorerNode) => node.key"
        :expanded="expandedKeys"
        class="min-h-0 flex-1 overflow-y-auto"
        @update:expanded="expand"
        @scroll="onScroll"
      >
        <tree-virtualizer
          v-slot="{ item }"
          :estimate-size="ROW_HEIGHT"
          :text-content="(flat) => flat.value.label"
        >
          <tree-item
            v-slot="{ isExpanded }"
            :value="item.value"
            :level="item.level"
            :class="rowClass(item.value as ExplorerNode)"
            :style="{ '--level': item.level }"
            @select="
              onSelect($event as TreeItemSelectEvent<ExplorerNode>, item.value as ExplorerNode)
            "
          >
            <explorer-line
              :node="item.value as ExplorerNode"
              :expanded="isExpanded"
              :menu="menuItems(item.value as ExplorerNode)"
            />
          </tree-item>
        </tree-virtualizer>
      </tree-root>

      <!-- Pinned lines repeat lines of the tree: hidden from assistive technologies. -->
      <div
        v-if="pinned.length"
        aria-hidden="true"
        class="absolute inset-x-0 top-0 z-10 border-b border-line bg-surface"
      >
        <div
          v-for="row in pinned"
          :key="row.node.key"
          :class="rowClass(row.node)"
          :style="{ '--level': row.level }"
          @click="onPinnedClick($event, row.node)"
        >
          <explorer-line
            :node="row.node"
            :expanded="expandedKeys.includes(row.node.key)"
            :menu="menuItems(row.node)"
          />
        </div>
      </div>
    </div>

    <connection-dialog v-model:open="dialogOpen" :connection="editing" @updated="onUpdated" />

    <ui-dialog
      v-model:open="removeOpen"
      title="Supprimer la connexion"
      :description="`Supprimer la connexion « ${removing?.name} » ?`"
    >
      <p class="text-fg-muted">
        Aucune donnée n'est perdue : seuls les paramètres de connexion enregistrés dans Prisme sont
        retirés, le serveur et ses bases ne sont pas touchés. La connexion pourra être recréée
        ultérieurement.
      </p>
      <template #footer>
        <div class="flex w-full justify-end gap-gap">
          <ui-button label="Annuler" @click="removing = null" />
          <ui-button label="Supprimer" variant="solid" tone="error" @click="confirmRemove" />
        </div>
      </template>
    </ui-dialog>
  </div>
</template>

<style scoped>
/* Indentation of each level: `--level` is set on the line (1 for the roots). */
.tree-row {
  padding-inline-start: calc(var(--space-unit) + var(--tree-indent) * (var(--level) - 1));
}
</style>
