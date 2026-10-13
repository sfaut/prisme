<!--
  Explorateur d'objets : sous la racine repliable « Sources de données »,
  une branche par source (connexion enregistrée), puis ses bases, tables et
  colonnes.

  Plusieurs sources se côtoient dans le même arbre. Le catalogue d'une source
  se lit quand on l'ouvre (celle de la connexion choisie s'ouvre d'elle-même)
  et arrive au fil de l'eau : les bases d'abord, puis leurs objets (tables,
  vues, procédures, fonctions), puis le détail des tables (colonnes, index,
  clefs étrangères, déclencheurs), chaque partie affichée dès sa réception.
  Il reste ensuite en mémoire. Un clic sur une source la choisit comme
  connexion de travail (émet `select`) ; elle reste surlignée.

  Au survol de la racine, un menu « ⋮ » à droite ajoute une connexion ou relit tous les
  catalogues déjà lus ; au survol de chaque source, un autre modifie la
  connexion, relit son catalogue ou supprime la connexion (après
  confirmation). Un double clic sur une table ou une vue émet `open-table`,
  pour en afficher le contenu.
-->
<script setup lang="ts">
import { NAlert, NButton, NDropdown, NModal, NTree, type TreeOption } from 'naive-ui'
import { computed, h, onMounted, ref, watch } from 'vue'

import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import { useCatalogStore } from '@/stores/catalog'
import { useConnectionsStore } from '@/stores/connections'

import ConnectionDialog from './ConnectionDialog.vue'
import { hashIcon, keyIcon, menuIcon } from './columnIcons'
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
}>()

const catalog = useCatalogStore()
const connections = useConnectionsStore()

/**
 * Arbre compact : lignes de 20 pixels sans marge, retrait et espacement horizontal réduits.
 * `nodeHeight` sert aussi au défilement virtuel de l'arbre. La taille du
 * texte est fixée en CSS (`.object-tree`) : avec le défilement virtuel, Naive
 * UI ignore la variable `fontSize` de son thème.
 */
const TREE_THEME = { nodeHeight: '20px', nodeWrapperPadding: '0' }

/**
 * Connexions dont le catalogue est en cours de lecture.
 */
const loadingIds = ref<number[]>([])

/**
 * Erreur de lecture de la liste des connexions, `null` si aucune.
 */
const listError = ref<string | null>(null)

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
 * Clef du nœud dont le menu est ouvert, `null` si aucun : son bouton reste
 * affiché quand la souris quitte la ligne pour le menu.
 */
const menuKey = ref<string | null>(null)

/**
 * Choix du menu de la racine.
 */
const SOURCES_MENU = [
  { key: 'add', label: 'Ajouter une connexion' },
  { key: 'refresh-all', label: 'Tout rafraîchir' },
]

/**
 * Choix du menu d'une source.
 */
const SOURCE_MENU = [
  { key: 'edit', label: 'Modifier la connexion' },
  { key: 'refresh', label: 'Rafraîchir' },
  { key: 'remove', label: 'Supprimer la connexion' },
]

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
const nodes = computed(
  () =>
    explorerTree(
      connections.connections.map((connection) => ({
        connectionId: connection.id,
        name: connection.name,
        catalog: catalog.catalogOf(connection.id),
        currentDatabase: connection.id === props.selectedId ? props.currentDatabase : null,
      })),
      receiving.value,
    ) as unknown as TreeOption[],
)

/**
 * Nœud surligné : la source de la connexion choisie.
 */
const selectedKeys = computed(() =>
  props.selectedId === null ? [] : [sourceKey(props.selectedId)],
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
 * Exécute un choix du menu d'un nœud.
 *
 * @param node Nœud dont le menu est ouvert.
 * @param choice Clef du choix.
 */
function onMenu(node: ExplorerNode, choice: string): void {
  const connection = connections.connections.find((item) => item.id === node.connectionId) ?? null
  switch (choice) {
    case 'add':
      editing.value = null
      dialogOpen.value = true
      break
    case 'refresh-all':
      refreshAll()
      break
    case 'edit':
      editing.value = connection
      dialogOpen.value = true
      break
    case 'refresh':
      void load(node.connectionId, true)
      break
    case 'remove':
      removing.value = connection
      break
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
 * Menu « ⋮ » à droite de la racine et de chaque source, affiché au survol de
 * la ligne (règles CSS `.node-menu`). Le clic sur le bouton ne se propage
 * pas : il n'ouvre ni ne ferme le nœud.
 *
 * @param info Nœud affiché.
 */
function renderSuffix({ option }: { option: TreeOption }) {
  const node = option as unknown as ExplorerNode
  if (node.role === undefined) {
    return null
  }
  return h(
    NDropdown,
    {
      trigger: 'click',
      size: 'small',
      options: node.role === 'sources' ? SOURCES_MENU : SOURCE_MENU,
      onSelect: (choice: string) => onMenu(node, choice),
      onUpdateShow: (show: boolean) => {
        menuKey.value = show ? node.key : null
      },
    },
    {
      default: () =>
        h(
          NButton,
          {
            text: true,
            size: 'tiny',
            class: { 'node-menu': true, 'node-menu-open': menuKey.value === node.key },
            title: 'Actions',
            onClick: (event: MouseEvent) => event.stopPropagation(),
          },
          { default: menuIcon },
        ),
    },
  )
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

/**
 * Suit les clics de l'arbre : un clic sur une source la choisit. Les autres
 * nœuds ne se surlignent pas, la source choisie reste surlignée.
 *
 * @param _keys Clefs que l'arbre voudrait surligner.
 * @param options Nœuds correspondants (vide quand on reclique la source surlignée).
 */
function onSelect(_keys: string[], options: (TreeOption | null)[]): void {
  const node = options[0] as unknown as ExplorerNode | undefined
  if (node?.role === 'source') {
    emit('select', node.connectionId)
  }
}

/**
 * Propriétés HTML de chaque nœud : le double clic sur une table l'ouvre.
 *
 * @param info Nœud affiché.
 */
function nodeProps({ option }: { option: TreeOption }) {
  const node = option as unknown as ExplorerNode
  return {
    onDblclick: () => {
      if (node.table !== undefined) {
        emit('open-table', node.connectionId, node.schema, node.table)
      }
    },
  }
}

/**
 * Affiche le libellé d'un nœud. Une colonne s'affiche en trois colonnes,
 * alignées sur toutes les colonnes de la table : symboles (clef pour la clef
 * primaire, croisillon pour une clef étrangère), nom (en gras si la colonne
 * est indexée, clef primaire comprise), puis type et nullabilité, en
 * capitales, plus petits et plus clairs. La largeur des noms se compte en
 * caractères (`ch`), toujours en gras : la même pour tous les noms, gras ou
 * non. Approchée pour une police proportionnelle, elle aligne les types sans
 * mesurer le texte.
 *
 * @param info Nœud affiché.
 */
function renderLabel({ option }: { option: TreeOption }) {
  const node = option as unknown as ExplorerNode
  // Hors colonnes : le libellé, suivi de sa mention éventuelle (UNIQUE).
  if (node.column === undefined) {
    // Toujours un élément, en gras ou non : passer d'un texte nu à un élément
    // (ou l'inverse) fait échouer la mise à jour de l'arbre par Vue.
    return h('span', { class: { 'current-database': node.current } }, [
      node.label,
      node.note === undefined ? null : h('span', { class: 'node-note' }, node.note),
    ])
  }
  const column = node.column
  return h('span', { class: 'column-label' }, [
    column.markSlots > 0
      ? h('span', { class: 'column-marks', style: { width: `${column.markSlots}em` } }, [
          column.primaryKey ? keyIcon() : null,
          column.foreignKey ? hashIcon() : null,
        ])
      : null,
    h('span', { class: 'column-name', style: { minWidth: `${column.nameWidth}ch` } }, [
      h('span', { class: { unindexed: !column.indexed } }, column.name),
    ]),
    // Texte complet au survol : le détail est tronqué (…) s'il déborde.
    h('span', { class: 'column-details', title: column.details }, column.details),
  ])
}
</script>

<template>
  <div class="object-explorer">
    <n-alert v-if="listError" type="error" :show-icon="false">{{ listError }}</n-alert>
    <n-alert v-for="message in messages" :key="message" type="warning" :show-icon="false">
      {{ message }}
    </n-alert>
    <n-tree
      :data="nodes"
      :expanded-keys="expandedKeys"
      :selected-keys="selectedKeys"
      :node-props="nodeProps"
      :render-label="renderLabel"
      :render-suffix="renderSuffix"
      :indent="8"
      :theme-overrides="TREE_THEME"
      block-line
      virtual-scroll
      class="object-tree"
      @update:expanded-keys="expand"
      @update:selected-keys="onSelect"
    />
    <connection-dialog v-model:open="dialogOpen" :connection="editing" @updated="onUpdated" />
    <n-modal
      :show="removing !== null"
      preset="dialog"
      type="warning"
      title="Supprimer la connexion"
      positive-text="Supprimer"
      negative-text="Annuler"
      @positive-click="confirmRemove"
      @negative-click="removing = null"
      @close="removing = null"
      @mask-click="removing = null"
    >
      Supprimer la connexion « {{ removing?.name }} » ?
    </n-modal>
  </div>
</template>

<style scoped>
.object-explorer {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 8px;
  min-height: 0;
}

.object-tree {
  flex: 1;
  min-height: 0;
  font-size: 11px;
}

.object-tree :deep(.n-tree-node-switcher) {
  width: 12px;
  height: 20px;
}

.object-tree :deep(.current-database) {
  font-weight: 700;
}

.object-tree :deep(.column-label) {
  display: inline-flex;
  max-width: 100%;
  align-items: center;
  /* Aligné en haut : une colonne des symboles vide ne rehausse pas la ligne. */
  vertical-align: top;
}

.object-tree :deep(.column-marks) {
  display: inline-flex;
  flex: none;
}

.object-tree :deep(.column-name) {
  flex: none;
  margin-right: 0.8em;
  font-weight: 700;
}

.object-tree :deep(.unindexed) {
  font-weight: 400;
}

.object-tree :deep(.node-note) {
  margin-left: 0.8em;
}

.object-tree :deep(.column-details),
.object-tree :deep(.node-note) {
  overflow: hidden;
  text-overflow: ellipsis;
  color: #999;
  font-size: 0.85em;
  white-space: nowrap;
}

.object-tree :deep(.n-tree-node-content__text) {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.object-tree :deep(.node-menu) {
  padding: 0 4px;
  color: #999;
  font-size: 13px;
  line-height: 1;
}

.object-tree :deep(.node-menu:hover) {
  color: #333;
}

/* Bouton visible au survol de la ligne, ou tant que son menu est ouvert. */
.object-tree :deep(.n-tree-node:not(:hover) .node-menu:not(.node-menu-open)) {
  visibility: hidden;
}

.object-tree :deep(.n-tree-node-content) {
  padding: 0 2px 0 1px;
}
</style>
