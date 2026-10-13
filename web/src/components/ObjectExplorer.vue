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

  Composants de Nuxt UI (`UTree`, `UDropdownMenu`, `UModal`, `UAlert`),
  importés automatiquement par son plugin Vite, et classes Tailwind.
-->
<script setup lang="ts">
import type { DropdownMenuItem, TreeItem } from '@nuxt/ui'
import type { TreeItemSelectEvent } from 'reka-ui'
import { computed, onMounted, ref, watch } from 'vue'

import { probeConnection, serverLabel } from '@/api/connections'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import { useCatalogStore } from '@/stores/catalog'
import { useConnectionsStore } from '@/stores/connections'

import ConnectionDialog from './ConnectionDialog.vue'
import { type ExplorerNode, SOURCES_KEY, explorerTree, sourceKey } from './explorerNodes'

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
 * Arbre compact : lignes basses, peu d'espace autour du texte, libellés
 * alignés à gauche (chaque ligne est un bouton, centré par défaut). `group`
 * permet d'afficher le bouton « ⋮ » au survol de la ligne.
 */
const TREE_UI = {
  link: 'group px-1 py-0.5 gap-1 text-xs text-start',
  linkLabel: 'min-w-0 flex-1 truncate text-trim',
  linkTrailing: 'gap-0',
}

/**
 * Hauteur estimée d'une ligne, en pixels, pour le défilement virtuel.
 */
const ROW_HEIGHT = 22

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
 * Arbre complet, construit à partir des catalogues en mémoire. La source de
 * la connexion choisie est surlignée.
 */
const nodes = computed(() => {
  const tree = explorerTree(
    connections.connections.map((connection) => ({
      connectionId: connection.id,
      name: connection.name,
      catalog: catalog.catalogOf(connection.id),
      currentDatabase: connection.id === props.selectedId ? props.currentDatabase : null,
    })),
    receiving.value,
  )
  for (const source of tree[0]?.children ?? []) {
    if (source.connectionId === props.selectedId) {
      Object.assign(source, { class: 'before:bg-elevated' })
    }
  }
  return tree as (ExplorerNode & TreeItem)[]
})

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
function menuItems(node: ExplorerNode): DropdownMenuItem[] | null {
  const connection = connections.connections.find((item) => item.id === node.connectionId) ?? null
  switch (node.role) {
    case 'sources':
      return [
        { label: 'Ajouter une connexion', icon: 'i-lucide-plus', onSelect: () => openDialog(null) },
        { label: 'Tout rafraîchir', icon: 'i-lucide-refresh-cw', onSelect: refreshAll },
      ]
    case 'notebooks':
      // Les notebooks ne sont pas encore enregistrés (étape 3 de la v1).
      return [{ label: 'Créer un notebook', icon: 'i-lucide-notebook-pen', disabled: true }]
    case 'source':
      return [
        { label: 'Modifier', icon: 'i-lucide-pencil', onSelect: () => openDialog(connection) },
        {
          label: 'Rafraîchir',
          icon: 'i-lucide-refresh-cw',
          onSelect: () => void load(node.connectionId, true),
        },
        {
          label: 'Tester',
          icon: 'i-lucide-plug-zap',
          onSelect: () => void probe(node.connectionId),
        },
        {
          label: 'Supprimer',
          icon: 'i-lucide-trash-2',
          color: 'error',
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
function onSelect(event: TreeItemSelectEvent<TreeItem>, node: TreeItem): void {
  event.preventDefault()
  const original = event.detail.originalEvent
  if (!(original instanceof MouseEvent) || original.detail !== 2) {
    return
  }
  const clicked = node as ExplorerNode
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
  <div class="flex min-h-0 flex-1 flex-col gap-2">
    <u-alert
      v-if="probed"
      color="success"
      variant="subtle"
      :title="probed"
      close
      @update:open="probed = null"
    />
    <u-alert v-if="listError" color="error" variant="subtle" :title="listError" />
    <u-alert
      v-for="message in messages"
      :key="message"
      color="warning"
      variant="subtle"
      :title="message"
    />
    <!-- Enveloppe : la portée des styles de ce composant ne passe pas jusqu'à UTree. -->
    <div class="object-tree flex min-h-0 flex-1 flex-col">
      <u-tree
        :items="nodes"
        :get-key="(node: TreeItem) => (node as ExplorerNode).key"
        :expanded="expandedKeys"
        :virtualize="{ estimateSize: ROW_HEIGHT }"
        size="xs"
        color="neutral"
        :ui="TREE_UI"
        class="min-h-0 flex-1"
        @update:expanded="expand"
        @select="onSelect"
      >
        <template #item-leading="{ item, expanded }">
          <u-icon
            v-if="item.children?.length"
            :name="expanded ? 'i-lucide-chevron-down' : 'i-lucide-chevron-right'"
            class="size-3 shrink-0 text-dimmed"
          />
          <span v-else class="size-3 shrink-0" />
        </template>

        <template #item-label="{ item }">
          <!-- Colonne : symboles, nom (gras si indexée), type et nullabilité. -->
          <span v-if="item.column" class="inline-flex max-w-full items-center align-top">
            <span
              v-if="item.column.markSlots > 0"
              class="inline-flex flex-none text-muted"
              :style="{ width: `${item.column.markSlots}em` }"
            >
              <u-icon
                v-if="item.column.primaryKey"
                name="i-lucide-key-round"
                title="Clef primaire"
              />
              <u-icon v-if="item.column.foreignKey" name="i-lucide-hash" title="Clef étrangère" />
            </span>
            <span
              class="me-3 flex-none font-bold"
              :style="{ minWidth: `${item.column.nameWidth}ch` }"
            >
              <span :class="{ 'font-normal': !item.column.indexed }">{{ item.column.name }}</span>
            </span>
            <span class="truncate text-[0.85em] text-dimmed" :title="item.column.details">
              {{ item.column.details }}
            </span>
          </span>
          <!-- Autres nœuds : libellé, puis mention éventuelle (UNIQUE, chargement…). -->
          <span
            v-else
            :class="{
              'current-database': item.current,
              'font-bold': item.current || item.role === 'sources' || item.role === 'notebooks',
            }"
          >
            {{ item.label }}
            <span v-if="item.note" class="ms-3 text-[0.85em] font-normal text-dimmed">
              {{ item.note }}
            </span>
          </span>
        </template>

        <template #item-trailing="{ item }">
          <!-- Jamais vide : un emplacement vide afficherait le chevron par défaut de Nuxt UI. -->
          <u-dropdown-menu v-if="menuItems(item)" :items="menuItems(item) ?? []" size="sm">
            <u-button
              icon="i-lucide-ellipsis-vertical"
              color="neutral"
              variant="ghost"
              size="xs"
              aria-label="Actions"
              class="node-menu invisible p-0.5 group-hover:visible data-[state=open]:visible"
              @click.stop
              @keydown.stop
            />
          </u-dropdown-menu>
          <span v-else />
        </template>
      </u-tree>
    </div>

    <connection-dialog v-model:open="dialogOpen" :connection="editing" @updated="onUpdated" />

    <u-modal
      v-model:open="removeOpen"
      title="Supprimer la connexion"
      :description="`Supprimer la connexion « ${removing?.name} » ?`"
    >
      <template #body>
        <p class="text-sm text-muted">
          Aucune donnée n'est perdue : seuls les paramètres de connexion enregistrés dans Prisme
          sont retirés, le serveur et ses bases ne sont pas touchés. La connexion pourra être
          recréée ultérieurement.
        </p>
      </template>
      <template #footer>
        <div class="flex w-full justify-end gap-2">
          <u-button color="neutral" variant="outline" label="Annuler" @click="removing = null" />
          <u-button color="error" label="Supprimer" @click="confirmRemove" />
        </div>
      </template>
    </u-modal>
  </div>
</template>

<style scoped>
/* Retrait de 12 pixels par niveau. Nuxt UI en met 22 (style en ligne, d'où
   !important), trop pour un arbre profond dans un panneau étroit. Reka UI
   pose le niveau de chaque ligne dans `aria-level`, lu ici comme un nombre
   par `attr()`. Un navigateur qui ne sait pas encore lire un attribut typé
   ignore la règle et garde le retrait de Nuxt UI. */
.object-tree :deep([data-slot='link'][aria-level]) {
  padding-inline-start: calc(4px + 12px * (attr(aria-level type(<integer>), 1) - 1)) !important;
}
</style>
