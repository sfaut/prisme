<!--
  Object navigator: under its title, "Navigateur d’objets" (`PanelHeader`),
  one collapsible root per source (saved connection), then its databases,
  tables and columns. Without any connection, "Aucune connexion." stands in
  place of the tree.

  Each editor has its own navigator, in its tab, which acts on it: its
  connection is the chosen source, its session's database is the current
  one. Each navigator keeps its open nodes and its scroll, and its layout
  (open or closed, its width), in the `navigator` store, part of the
  display of the editor saved on the server side (`editorDisplays` store),
  for a reload as for a reopening; the
  catalogs read, the list of the connections and the
  windows are common to all. A navigator builds its tree only while its
  editor is active: the others wait, hidden, without following the catalogs
  as they arrive.

  Several sources sit side by side in the same tree. The catalog of a source
  is read when it is opened (the source of the editor opens by itself the
  first time, and when its connection changes; the sources left open are
  read again when the editor becomes active) and arrives progressively: the databases first, then their objects (tables,
  views, procedures, functions), then the details of the tables (columns,
  indexes, foreign keys, triggers), each part shown as soon as it arrives.
  It then stays in memory. A click on a line opens or closes its level.
  Ctrl+click (Cmd+click on macOS), or Enter on the focused line, acts on it
  without opening or closing it: on a source, chooses it as the working
  connection (emits `select`), which stays in bold; on a database, makes
  it the current database, like `USE` (emits `use-database`); on a table or a
  view, emits `open-table`, to show its content; on an index, emits
  `index-operations`, to write the operations on it. A double click would open
  and close the line between its two clicks: it has no action. On hovering
  a database, a table or an index, a button on the right of its line does
  the same (plug: current database, eye: content, pencil and ruler:
  operations on the index). On hovering a source, and the current database
  instead of its plug, an unplugged plug closes the session of the editor
  (emits `disconnect`): it is enabled on the editor's source only, once its
  session is ready, and disabled while queries run.

  Each source carries as a badge the flavor of its server (`mysql`,
  `mariadb`), seen on the last connection or when reading the catalog. Each
  table shows after its name its row count estimated by the server, in short
  form (`345`, `1,3 k`); a view has none. Each "Tables" folder sorts its
  tables by name (by default) or by row count, the largest first, with the
  two buttons of its line; the choice is kept per folder with the tree.

  Each line carries the icon of its level: source (that of its kind, a
  server for a database), database, folder (open or closed), table, view,
  procedure, function, column, index, foreign key, trigger, and a spinning
  circle during a load, in place of the source's icon while its catalog
  arrives.

  The button of the title, always shown, reads again all the catalogs
  already read; connections are added in the connections manager (top
  bar). On hovering each source, a "⋮" menu opens the connection in the connections
  manager ("Modifier"), tests it, reads its catalog again or deletes it
  (after confirmation, `ConnectionRemoveDialog`, which warns of the
  transactions it rolls back); "Déconnecter" there closes the session of
  the editor, like the button of the line. The active navigator reads again
  the catalog of a connection changed in the manager.

  The tree is Reka UI's (`TreeRoot`, `TreeItem`: keyboard, opening of the
  levels, accessibility), with virtual scrolling (`TreeVirtualizer`); the
  rendering of the lines is Prisme's, in Tailwind classes. All the lines
  have the same height (`--row-height`), and each level is indented by
  three steps of the spacing scale (`main.css`).

  While the tree scrolls, each level leaves the top in full and stays pinned
  there as long as its section lasts (`stickyRows.ts`): the source, the
  database and the table keep showing over their columns. The pinned lines are
  the lines of the tree (`NavigatorLine.vue`), hidden from assistive
  technologies, which read the tree.

  A hidden tree loses its scroll: the navigator puts it back when its editor
  becomes active (and when the page opens), as the rows arrive, until the
  position is reached or the user moves in the tree.
-->
<script setup lang="ts">
import { errorMessage } from '@/api/errorMessage'
import { Pencil, PlugZap, RefreshCw, Trash2, Unplug } from '@lucide/vue'
import {
  TreeItem,
  type TreeItemSelectEvent,
  type TreeItemToggleEvent,
  TreeRoot,
  TreeVirtualizer,
} from 'reka-ui'
import { computed, nextTick, onMounted, ref, useTemplateRef, watch } from 'vue'

import { probeConnection, probeMessage } from '@/api/connections'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import type { SessionStatus } from '@/api/querySession'
import { cssPixels } from '@/utils/cssLength'
import { useCatalogStore } from '@/stores/catalog'
import { useConnectionsStore } from '@/stores/connections'
import { useNavigatorStore } from '@/stores/navigator'

import NavigatorLine from './NavigatorLine.vue'
import ConnectionRemoveDialog from '@/components/connections/ConnectionRemoveDialog.vue'
import PanelHeader from '@/components/panel/PanelHeader.vue'
import { type NavigatorNode, sourceKey, sourceNodes } from './navigatorNodes'
import { pinnedRows, visibleRows } from './stickyRows'
import UiAlert from '@/components/ui/UiAlert.vue'
import UiButton from '@/components/ui/UiButton.vue'
import { vTooltip } from '@/components/ui/tooltip'
import type { MenuItem } from '@/components/ui/menuItem'

const props = defineProps<{
  /**
   * Id of the editor of the navigator.
   */
  editorId: number
  /**
   * Is the editor active, its navigator on screen?
   */
  active: boolean
  /**
   * Id of the editor's connection, the chosen source; `null` if none.
   */
  selectedId: number | null
  /**
   * Current database of the editor's session, shown in bold; `null` if
   * none.
   */
  currentDatabase: string | null
  /**
   * Stage of the editor's session, on its connection; `null` without a
   * session.
   */
  sessionStatus: SessionStatus | null
}>()

const emit = defineEmits<{
  /**
   * The user wants to see the content of a table.
   */
  'open-table': [connectionId: number, schema: string, table: string]
  /**
   * The user wants to write the operations on an index.
   */
  'index-operations': [connectionId: number, schema: string, table: string, index: string]
  /**
   * The user chooses a source as the working connection.
   */
  select: [connectionId: number]
  /**
   * The user chooses a database as the current database (`USE`).
   */
  'use-database': [connectionId: number, schema: string]
  /**
   * The user closes the session of the editor.
   */
  disconnect: []
}>()

const catalog = useCatalogStore()
const connections = useConnectionsStore()
const navigatorStore = useNavigatorStore()

/**
 * Height of a line, in pixels, for virtual scrolling
 * (`--row-height` of the theme).
 *
 * Fixed size: the `estimate-size` of Reka UI's `TreeVirtualizer` only takes
 * pixels, so this theme length is read once (`docs/DEVELOPER.md`, section "Lengths in pixels").
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
 * Was the list of connections received? Until then, an empty list does not
 * mean that there is no connection.
 */
const listed = ref(false)

/**
 * Message of the last successful test of a source (name, flavor and version
 * of the server, and the warning of an unsupported version), `null` if none.
 */
const probed = ref<ReturnType<typeof probeMessage> | null>(null)

/**
 * Last reading error of each connection, by id.
 */
const failures = ref<Record<number, string>>({})

/**
 * Connection whose deletion waits for confirmation, `null` if none.
 */
const removing = ref<ConnectionSummary | null>(null)

/**
 * Forgets the reading error of a deleted connection.
 *
 * @param connectionId Id of the deleted connection.
 */
function forgetFailure(connectionId: number): void {
  delete failures.value[connectionId]
}

/**
 * Tree of the editor's navigator: open nodes and scroll.
 */
const tree = navigatorStore.treeOf(props.editorId)

/**
 * Keys of the open nodes.
 */
const expandedKeys = computed({
  get: () => tree.expanded,
  set: (keys: string[]) => {
    tree.expanded = keys
  },
})

/**
 * Last tree built, kept while the editor is not active.
 */
let built: NavigatorNode[] = []

/**
 * Whole tree, built from the catalogs in memory while the editor is active;
 * hidden, the last one built, which follows nothing.
 */
const nodes = computed(() => {
  if (props.active) {
    built = sourceNodes(
      connections.connections.map((connection) => {
        const read = catalog.catalogOf(connection.id)
        return {
          connectionId: connection.id,
          name: connection.name,
          catalog: read,
          receiving: read !== undefined && !read.done,
          chosen: connection.id === props.selectedId,
          currentDatabase: connection.id === props.selectedId ? props.currentDatabase : null,
          flavor: read?.source?.flavor ?? connection.server_flavor,
          version: read?.source?.version ?? connection.server_version,
          showSystemSchemas: connection.show_system_schemas,
        }
      }),
      new Set(tree.tables_by_rows),
    )
  }
  return built
})

/**
 * Rows displayed by the tree, with the rows of each level.
 */
const visible = computed(() => visibleRows(nodes.value, expandedKeys.value))

/**
 * Position of the tree's scroll, in pixels.
 */
const scrollTop = ref(0)

/**
 * Levels pinned at the top of the tree while it scrolls, from the root down.
 */
const pinned = computed(() => pinnedRows(visible.value, scrollTop.value, ROW_HEIGHT))

/**
 * Classes of a line of the tree, and of a pinned line, which look the same.
 */
const ROW_CLASSES =
  'tree-row group flex h-(--row-height) w-full cursor-pointer items-center gap-1 rounded-control pe-1 text-start select-none hover:bg-surface-sunken focus-visible:bg-surface-sunken'

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
    await catalog.load(connectionId, refresh, props.editorId)
  } catch (failure) {
    failures.value[connectionId] = errorMessage(failure)
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
 * Stage of the editor's session for a source: that of the chosen
 * connection, `null` for the other sources (no session of the editor).
 *
 * @param connectionId Id of the source's connection.
 */
function sessionOf(connectionId: number): SessionStatus | null {
  return connectionId === props.selectedId ? props.sessionStatus : null
}

/**
 * Choices of a node's "⋮" menu, `null` for a node without a menu.
 *
 * @param node Node of the tree.
 */
function menuItems(node: NavigatorNode): MenuItem[] | null {
  const connection = connections.connections.find((item) => item.id === node.connectionId) ?? null
  switch (node.role) {
    case 'source':
      // The connection may have been deleted since the tree was built: no menu then.
      if (connection === null) {
        return null
      }
      return [
        {
          label: 'Modifier',
          icon: Pencil,
          onSelect: () => connections.openManager(connection.id),
        },
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
          label: 'Déconnecter',
          icon: Unplug,
          disabled: sessionOf(node.connectionId) !== 'ready',
          onSelect: () => emit('disconnect'),
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
    const message = probeMessage(await probeConnection(connectionId))
    probed.value = { ...message, title: `${name} : ${message.title}` }
  } catch (failure) {
    failures.value[connectionId] = errorMessage(failure)
  }
}

// A changed connection may now target another server: the active navigator
// reads its catalog again, if it was already read (the catalogs are common to
// all the navigators).
watch(
  () => ({ ...connections.versions }),
  (versions, before) => {
    if (!props.active) {
      return
    }
    for (const [id, version] of Object.entries(versions)) {
      const connectionId = Number(id)
      if (version !== before[connectionId] && catalog.catalogs[connectionId] !== undefined) {
        void load(connectionId, true)
      }
    }
  },
)

/**
 * Does an event ask to act on a line, without opening or closing it? A click
 * with Ctrl (Cmd on macOS, where Ctrl+click opens the context menu), or
 * Enter.
 *
 * @param event Original event of the tree, or of a pinned line.
 */
function asksAction(event: Event): boolean {
  if (event instanceof MouseEvent) {
    return event.ctrlKey || event.metaKey
  }
  return event instanceof KeyboardEvent && event.key === 'Enter'
}

/**
 * Follows the selections that the tree requests, on a click, Enter or Space.
 * The tree keeps no selection (the source in bold is that of the chosen
 * connection); a Ctrl+click or Enter acts on the line.
 *
 * @param event Selection requested by the tree, with the original event.
 * @param node Node of the line.
 */
function onSelect(event: TreeItemSelectEvent<NavigatorNode>, node: NavigatorNode): void {
  event.preventDefault()
  if (asksAction(event.detail.originalEvent)) {
    activate(node)
  }
}

/**
 * Follows the openings and closings that the tree requests: a Ctrl+click
 * acts on the line, which stays as it is.
 *
 * @param event Opening or closing requested by the tree, with the original event.
 */
function onToggle(event: TreeItemToggleEvent<NavigatorNode>): void {
  if (asksAction(event.detail.originalEvent)) {
    event.preventDefault()
  }
}

/**
 * Acts on a node, after a Ctrl+click or Enter: chooses a source, changes the
 * current database, opens the table or writes the operations on the index.
 *
 * @param node Node of the line.
 */
function activate(node: NavigatorNode): void {
  if (node.role === 'source') {
    emit('select', node.connectionId)
  } else if (node.role === 'database') {
    if (node.usable === true) {
      emit('use-database', node.connectionId, node.schema)
    }
  } else if (node.table !== undefined) {
    emit('open-table', node.connectionId, node.schema, node.table)
  } else if (node.index !== undefined) {
    emit('index-operations', node.connectionId, node.schema, node.index.table, node.index.name)
  }
}

/**
 * Follows the clicks on a pinned line, as the tree does: a click opens or
 * closes its level, a Ctrl+click acts on it.
 *
 * @param event Original click.
 * @param clicked Node of the pinned line.
 */
function onPinnedClick(event: MouseEvent, clicked: NavigatorNode): void {
  if (asksAction(event)) {
    activate(clicked)
  } else if (clicked.children?.length) {
    expand(
      expandedKeys.value.includes(clicked.key)
        ? expandedKeys.value.filter((key) => key !== clicked.key)
        : [...expandedKeys.value, clicked.key],
    )
  }
}

/**
 * Box of the tree, which holds its scrolling element.
 */
const frame = useTemplateRef<HTMLElement>('frame')

/**
 * Scroll of the tree still to put back, in pixels; `null` once reached, or
 * once the user moves in the tree.
 */
let pendingScroll: number | null = null

/**
 * Scrolling element of the tree, absent before mounting.
 */
function treeElement(): HTMLElement | null {
  return frame.value?.querySelector<HTMLElement>('[role="tree"]') ?? null
}

/**
 * Puts the kept scroll back, as far as the rows already there allow; once
 * reached, nothing is left to put back.
 */
function restoreScroll(): void {
  const element = treeElement()
  if (pendingScroll === null || element === null || !props.active) {
    return
  }
  element.scrollTop = pendingScroll
  scrollTop.value = element.scrollTop
  if (Math.abs(element.scrollTop - pendingScroll) < 1) {
    pendingScroll = null
  }
}

/**
 * Tries again to put the scroll back once the tree has drawn its new rows.
 */
function retryScroll(): void {
  if (pendingScroll !== null) {
    void nextTick(() => requestAnimationFrame(restoreScroll))
  }
}

/**
 * The user moves in the tree: the scroll is theirs, nothing is put back.
 */
function stopRestoring(): void {
  pendingScroll = null
}

/**
 * Follows the scroll of the tree, to pin the rows of its hierarchy, and
 * keeps it for the editor.
 *
 * @param event Scroll event of the tree.
 */
function onScroll(event: Event): void {
  scrollTop.value = (event.target as HTMLElement).scrollTop
  if (props.active && pendingScroll === null) {
    navigatorStore.scrollTo(props.editorId, scrollTop.value)
  }
}

onMounted(async () => {
  try {
    await connections.ensureLoaded()
    listed.value = true
  } catch (failure) {
    listError.value = `Connexions illisibles.

${errorMessage(failure)}`
  }
})

/**
 * Connection of the editor seen while it was last active; `undefined`
 * before its first activation.
 */
let seenConnection: number | null | undefined

/**
 * Opens the source of the editor, when its tree is new or when its
 * connection changed, then reads the catalogs of the open sources.
 *
 * @param connectionId Id of the editor's connection, `null` if none.
 */
function openSources(connectionId: number | null): void {
  const changed =
    seenConnection === undefined
      ? navigatorStore.isNew(props.editorId)
      : connectionId !== seenConnection
  seenConnection = connectionId
  if (changed && connectionId !== null && !expandedKeys.value.includes(sourceKey(connectionId))) {
    expandedKeys.value = [...expandedKeys.value, sourceKey(connectionId)]
  }
  if (connectionId !== null) {
    // Directly: the list of connections may not be received yet.
    void load(connectionId, false)
  }
  for (const connection of connections.connections) {
    if (expandedKeys.value.includes(sourceKey(connection.id))) {
      void load(connection.id, false)
    }
  }
}

// Scroll changed by the reset of the editor: the tree follows.
watch(
  () => tree.scroll_top,
  (top) => {
    const element = treeElement()
    if (element !== null && Math.abs(element.scrollTop - top) >= 1) {
      pendingScroll = top
      retryScroll()
    }
  },
)

// While the editor is active: its source opens, its open sources are read,
// and so are they again when the list of connections arrives; its kept
// scroll comes back.
watch(
  () => props.active,
  (active) => {
    if (active) {
      openSources(props.selectedId)
      pendingScroll = tree.scroll_top
      retryScroll()
    }
  },
  { immediate: true },
)

watch(
  () => props.selectedId,
  (connectionId) => {
    if (props.active) {
      openSources(connectionId)
    }
  },
)

watch(
  () => connections.connections,
  () => {
    if (props.active) {
      openSources(props.selectedId)
    }
  },
)

watch(() => visible.value.rows.length, retryScroll)
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col gap-3">
    <panel-header title="Navigateur d’objets">
      <template #actions>
        <ui-button
          :icon="RefreshCw"
          variant="ghost"
          small
          aria-label="Rafraîchir"
          v-tooltip="{ title: 'Rafraîchir', description: 'Relit les catalogues déjà lus' }"
          @click="refreshAll"
        />
      </template>
    </panel-header>
    <ui-alert v-if="probed" :tone="probed.tone" :title="probed.title" @close="probed = null" />
    <ui-alert v-if="listError" tone="error" :title="listError" />
    <!-- Index as key: the same message can come twice, from two errors of one source. -->
    <ui-alert v-for="(message, index) in messages" :key="index" tone="warning" :title="message" />

    <p v-if="listed && connections.connections.length === 0" class="text-fg-secondary">
      Aucune connexion.
    </p>
    <div
      v-else
      ref="frame"
      class="relative flex min-h-0 flex-1 flex-col"
      @wheel.passive="stopRestoring"
      @pointerdown="stopRestoring"
      @keydown="stopRestoring"
    >
      <tree-root
        :items="nodes"
        :get-key="(node: NavigatorNode) => node.key"
        :expanded="expandedKeys"
        class="min-h-0 flex-1 overflow-y-auto"
        @update:expanded="expand"
        @scroll="onScroll"
      >
        <tree-virtualizer
          v-slot="{ item }"
          :estimate-size="ROW_HEIGHT"
          :text-content="(node) => node.label"
        >
          <tree-item
            v-slot="{ isExpanded }"
            :value="item.value"
            :level="item.level"
            :class="ROW_CLASSES"
            :style="{ '--level': item.level }"
            @select="
              onSelect($event as TreeItemSelectEvent<NavigatorNode>, item.value as NavigatorNode)
            "
            @toggle="onToggle($event as TreeItemToggleEvent<NavigatorNode>)"
          >
            <navigator-line
              :node="item.value as NavigatorNode"
              :expanded="isExpanded"
              :menu="menuItems(item.value as NavigatorNode)"
              :session="sessionOf((item.value as NavigatorNode).connectionId)"
              @act="activate(item.value as NavigatorNode)"
              @disconnect="emit('disconnect')"
              @sort-tables="
                navigatorStore.sortTables(editorId, (item.value as NavigatorNode).key, $event)
              "
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
          :class="ROW_CLASSES"
          :style="{ '--level': row.level }"
          @click="onPinnedClick($event, row.node)"
        >
          <navigator-line
            :node="row.node"
            :expanded="expandedKeys.includes(row.node.key)"
            :menu="menuItems(row.node)"
            :session="sessionOf(row.node.connectionId)"
            @act="activate(row.node)"
            @disconnect="emit('disconnect')"
            @sort-tables="navigatorStore.sortTables(editorId, row.node.key, $event)"
          />
        </div>
      </div>
    </div>

    <connection-remove-dialog
      v-model:connection="removing"
      @removed="forgetFailure"
      @failed="(id, message) => (failures[id] = message)"
    />
  </div>
</template>

<style scoped>
@reference "@/assets/main.css";

/* Indentation of each level: `--level` is set on the line (1 for the roots). */
.tree-row {
  padding-inline-start: calc(--spacing(1) + --spacing(4) * (var(--level) - 1));
}
</style>
