<!--
  Explorateur d'objets : sous la racine repliable « Sources de données »,
  une branche par source (connexion enregistrée), puis ses bases, tables et
  colonnes ; ensuite, la racine « Notebooks », vide pour l'instant.

  Plusieurs sources se côtoient dans le même arbre. Le catalogue d'une source
  se lit quand on l'ouvre (celle de la connexion choisie s'ouvre d'elle-même)
  et arrive au fil de l'eau : les bases d'abord, puis leurs objets (tables,
  vues, procédures, fonctions), puis le détail des tables (colonnes, index,
  clefs étrangères, déclencheurs), chaque partie affichée dès sa réception.
  Il reste ensuite en mémoire. Un clic sur une ligne ouvre ou ferme son
  niveau. Un double clic sur une source la choisit comme connexion de travail
  (émet `select`) ; elle reste surlignée. Un double clic sur une base en fait
  la base en cours, comme `USE` (émet `use-database`). Un double clic sur une
  table ou une vue émet `open-table`, pour en afficher le contenu.

  Au survol de la racine, un menu « ⋮ » à droite ajoute une connexion ou relit
  tous les catalogues déjà lus ; au survol de chaque source, un autre modifie
  ou teste la connexion, relit son catalogue ou supprime la connexion (après
  confirmation).

  L'arbre est celui de Reka UI (`TreeRoot`, `TreeItem` : clavier, ouverture
  des niveaux, accessibilité), en défilement virtuel (`TreeVirtualizer`) ;
  le rendu des lignes est celui de Prisme, en classes Tailwind. Toutes les
  lignes ont la même hauteur (`--row-height`), et chaque niveau est en
  retrait de `--tree-indent` (variables de `main.css`).
-->
<script setup lang="ts">
import {
  ChevronDown,
  ChevronRight,
  EllipsisVertical,
  Hash,
  KeyRound,
  NotebookPen,
  Pencil,
  PlugZap,
  Plus,
  RefreshCw,
  Trash2,
} from 'lucide-vue-next'
import { TreeItem, type TreeItemSelectEvent, TreeRoot, TreeVirtualizer } from 'reka-ui'
import { computed, onMounted, ref, watch } from 'vue'

import { probeConnection, serverLabel } from '@/api/connections'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import { cssPixels } from '@/cssLength'
import { useCatalogStore } from '@/stores/catalog'
import { useConnectionsStore } from '@/stores/connections'

import ConnectionDialog from './ConnectionDialog.vue'
import { type ExplorerNode, SOURCES_KEY, explorerTree, sourceKey } from './explorerNodes'
import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import UiDialog from './ui/UiDialog.vue'
import UiDropdownMenu from './ui/UiDropdownMenu.vue'
import type { MenuItem } from './ui/menuItem'

const props = defineProps<{
  /**
   * Identifiant de la connexion choisie, `null` si aucune.
   */
  selectedId: number | null
  /**
   * Base en cours de la session de requêtes de la connexion choisie, affichée
   * en gras ; `null` si aucune.
   */
  currentDatabase: string | null
}>()

const emit = defineEmits<{
  /**
   * L'utilisateur veut voir le contenu d'une table.
   */
  'open-table': [connectionId: number, schema: string, table: string]
  /**
   * L'utilisateur choisit une source comme connexion de travail.
   */
  select: [connectionId: number]
  /**
   * L'utilisateur choisit une base comme base en cours (`USE`).
   */
  'use-database': [connectionId: number, schema: string]
}>()

const catalog = useCatalogStore()
const connections = useConnectionsStore()

/**
 * Hauteur d'une ligne, en pixels, pour le défilement virtuel
 * (`--row-height` du thème).
 */
const ROW_HEIGHT = cssPixels('--row-height', 24)

/**
 * Connexions dont le catalogue est en cours de lecture.
 */
const loadingIds = ref<number[]>([])

/**
 * Erreur de lecture de la liste des connexions, `null` si aucune.
 */
const listError = ref<string | null>(null)

/**
 * Résultat du dernier test réussi d'une source (nom, variante et version du
 * serveur), `null` si aucun.
 */
const probed = ref<string | null>(null)

/**
 * Dernière erreur de lecture de chaque connexion, par identifiant.
 */
const failures = ref<Record<number, string>>({})

/**
 * Fenêtre de création ou de modification ouverte ?
 */
const dialogOpen = ref(false)

/**
 * Connexion modifiée par la fenêtre, `null` pour en créer une.
 */
const editing = ref<ConnectionSummary | null>(null)

/**
 * Connexion dont la suppression attend confirmation, `null` si aucune.
 */
const removing = ref<ConnectionSummary | null>(null)

/**
 * Fenêtre de confirmation de la suppression ouverte ?
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
 * Clefs des nœuds ouverts.
 */
const expandedKeys = ref<string[]>([SOURCES_KEY])

/**
 * Un catalogue est-il encore en cours de réception ?
 */
const receiving = computed(() =>
  connections.connections.some((connection) => {
    const current = catalog.catalogOf(connection.id)
    return current !== undefined && !current.done
  }),
)

/**
 * Arbre complet, construit à partir des catalogues en mémoire.
 */
const nodes = computed(() =>
  explorerTree(
    connections.connections.map((connection) => ({
      connectionId: connection.id,
      name: connection.name,
      catalog: catalog.catalogOf(connection.id),
      currentDatabase: connection.id === props.selectedId ? props.currentDatabase : null,
    })),
    receiving.value,
  ),
)

/**
 * Erreurs à afficher, précédées du nom de leur source : échecs de lecture,
 * puis erreurs signalées par le serveur pendant la réception.
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
 * Lit le catalogue d'une connexion, depuis la mémoire ou depuis le serveur.
 *
 * @param connectionId Identifiant de la connexion.
 * @param refresh Relit le catalogue sur le serveur même s'il est en mémoire.
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
 * Relit les catalogues déjà lus.
 */
function refreshAll(): void {
  for (const connection of connections.connections) {
    if (catalog.catalogs[connection.id] !== undefined) {
      void load(connection.id, true)
    }
  }
}

/**
 * Suit l'ouverture et la fermeture des nœuds : ouvrir une source lit son catalogue.
 *
 * @param keys Clefs des nœuds désormais ouverts.
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
 * Choix du menu « ⋮ » d'un nœud, `null` pour un nœud sans menu.
 *
 * @param node Nœud de l'arbre.
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
      // Les notebooks ne sont pas encore enregistrés (étape 3 de la v1).
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
 * Ouvre la fenêtre de connexion.
 *
 * @param connection Connexion à modifier, `null` pour en créer une.
 */
function openDialog(connection: ConnectionSummary | null): void {
  editing.value = connection
  dialogOpen.value = true
}

/**
 * Se connecte au serveur d'une source et affiche sa variante et sa version.
 * Un échec s'affiche comme une erreur de lecture de la source.
 *
 * @param connectionId Identifiant de la connexion.
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
 * Relit le catalogue d'une connexion modifiée, s'il était déjà lu : la
 * connexion peut désormais viser un autre serveur.
 *
 * @param connectionId Identifiant de la connexion modifiée.
 */
function onUpdated(connectionId: number): void {
  if (catalog.catalogs[connectionId] !== undefined) {
    void load(connectionId, true)
  }
}

/**
 * Supprime la connexion qui attendait confirmation.
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
 * Suit les clics sur les lignes. L'arbre ne garde aucune sélection (la
 * source surlignée est celle de la connexion choisie) ; le clic ouvre ou
 * ferme le niveau. Le second clic d'un double clic choisit la source, change
 * de base ou ouvre la table.
 *
 * @param event Sélection demandée par l'arbre, avec le clic d'origine.
 * @param node Nœud cliqué.
 */
function onSelect(event: TreeItemSelectEvent<ExplorerNode>, clicked: ExplorerNode): void {
  event.preventDefault()
  const original = event.detail.originalEvent
  if (!(original instanceof MouseEvent) || original.detail !== 2) {
    return
  }
  if (clicked.role === 'source') {
    emit('select', clicked.connectionId)
  } else if (clicked.role === 'database') {
    emit('use-database', clicked.connectionId, clicked.schema)
  } else if (clicked.table !== undefined) {
    emit('open-table', clicked.connectionId, clicked.schema, clicked.table)
  }
}

onMounted(async () => {
  try {
    await connections.load()
  } catch (failure) {
    listError.value = `Connexions illisibles : ${failure instanceof Error ? failure.message : String(failure)}`
  }
})

// La connexion choisie ouvre sa source.
watch(
  () => props.selectedId,
  (connectionId) => {
    if (connectionId === null) {
      return
    }
    if (!expandedKeys.value.includes(sourceKey(connectionId))) {
      expandedKeys.value = [...expandedKeys.value, sourceKey(connectionId)]
    }
    // Directement : la liste des connexions n'est peut-être pas encore reçue.
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

    <tree-root
      :items="nodes"
      :get-key="(node: ExplorerNode) => node.key"
      :expanded="expandedKeys"
      class="min-h-0 flex-1 overflow-y-auto"
      @update:expanded="expand"
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
          class="tree-row group flex h-row w-full cursor-pointer items-center gap-1 rounded-control pe-1 text-start outline-none select-none hover:bg-surface-muted focus-visible:bg-surface-muted"
          :class="{
            'bg-surface-elevated':
              item.value.role === 'source' && item.value.connectionId === selectedId,
          }"
          :style="{ '--level': item.level }"
          @select="
            onSelect($event as TreeItemSelectEvent<ExplorerNode>, item.value as ExplorerNode)
          "
        >
          <component
            :is="isExpanded ? ChevronDown : ChevronRight"
            v-if="item.value.children?.length"
            class="size-icon-sm shrink-0 text-fg-dimmed"
          />
          <span v-else class="size-icon-sm shrink-0" />

          <!-- Colonne : symboles, nom (gras si indexée), type et nullabilité. -->
          <span v-if="item.value.column" class="tree-label flex min-w-0 flex-1 items-center">
            <span
              v-if="item.value.column.markSlots > 0"
              class="inline-flex flex-none text-fg-muted"
              :style="{ width: `${item.value.column.markSlots}em` }"
            >
              <KeyRound
                v-if="item.value.column.primaryKey"
                class="size-[1em]"
                aria-label="Clef primaire"
              />
              <Hash
                v-if="item.value.column.foreignKey"
                class="size-[1em]"
                aria-label="Clef étrangère"
              />
            </span>
            <span
              class="me-3 flex-none font-bold text-trim"
              :style="{ minWidth: `${item.value.column.nameWidth}ch` }"
            >
              <span :class="{ 'font-normal': !item.value.column.indexed }">
                {{ item.value.column.name }}
              </span>
            </span>
            <span
              class="truncate text-small text-fg-dimmed text-trim"
              :title="item.value.column.details"
            >
              {{ item.value.column.details }}
            </span>
          </span>
          <!-- Autres nœuds : libellé, puis mention éventuelle (UNIQUE, chargement…). -->
          <span
            v-else
            class="tree-label min-w-0 flex-1 truncate text-trim"
            :class="{
              'current-database': item.value.current,
              'font-bold':
                item.value.current ||
                item.value.role === 'sources' ||
                item.value.role === 'notebooks',
            }"
          >
            {{ item.value.label }}
            <span v-if="item.value.note" class="ms-3 text-small font-normal text-fg-dimmed">
              {{ item.value.note }}
            </span>
          </span>

          <ui-dropdown-menu
            v-if="menuItems(item.value as ExplorerNode)"
            :items="menuItems(item.value as ExplorerNode) ?? []"
          >
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
        </tree-item>
      </tree-virtualizer>
    </tree-root>

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
/* Retrait de chaque niveau : `--level` est posé sur la ligne (1 pour les racines). */
.tree-row {
  padding-inline-start: calc(var(--space-unit) + var(--tree-indent) * (var(--level) - 1));
}
</style>
