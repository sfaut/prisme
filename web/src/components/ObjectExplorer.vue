<!--
  Explorateur d'objets : une racine par source de données (connexion
  enregistrée), puis ses bases, tables et colonnes.

  Plusieurs sources se côtoient dans le même arbre. Le catalogue d'une source
  se lit quand on l'ouvre (celle de la connexion choisie s'ouvre d'elle-même)
  et arrive au fil de l'eau : les bases d'abord, puis leurs objets (tables,
  vues, procédures, fonctions), puis le détail des tables (colonnes, index,
  clefs étrangères, déclencheurs), chaque partie affichée dès sa réception.
  Il reste ensuite en mémoire. « Rafraîchir » relit les catalogues déjà lus.
  Un double clic sur une table ou une vue émet `open-table`, pour en afficher
  le contenu.
-->
<script setup lang="ts">
import { NAlert, NButton, NTree, type TreeOption } from 'naive-ui'
import { computed, h, ref, watch } from 'vue'

import { useCatalogStore } from '@/stores/catalog'
import { useConnectionsStore } from '@/stores/connections'

import { hashIcon, keyIcon } from './columnIcons'
import { type ExplorerNode, sourceKey, sourceNodes } from './explorerNodes'

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
 * Dernière erreur de lecture de chaque connexion, par identifiant.
 */
const failures = ref<Record<number, string>>({})

/**
 * Clefs des nœuds ouverts.
 */
const expandedKeys = ref<string[]>([])

/**
 * Arbre complet, construit à partir des catalogues en mémoire.
 */
const nodes = computed(
  () =>
    sourceNodes(
      connections.connections.map((connection) => ({
        connectionId: connection.id,
        name: connection.name,
        catalog: catalog.catalogOf(connection.id),
        currentDatabase: connection.id === props.selectedId ? props.currentDatabase : null,
      })),
    ) as unknown as TreeOption[],
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
 * Un catalogue est-il encore en cours de réception ?
 */
const receiving = computed(() =>
  connections.connections.some((connection) => {
    const current = catalog.catalogOf(connection.id)
    return current !== undefined && !current.done
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
function refresh(): void {
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

// La connexion choisie ouvre sa source.
watch(
  () => props.selectedId,
  (connectionId) => {
    if (connectionId !== null && !expandedKeys.value.includes(sourceKey(connectionId))) {
      expand([...expandedKeys.value, sourceKey(connectionId)])
    }
  },
  { immediate: true },
)

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
    <div class="explorer-toolbar">
      <span class="explorer-title">Objets<template v-if="receiving"> · chargement…</template></span>
      <n-button size="tiny" :loading="loadingIds.length > 0" @click="refresh">Rafraîchir</n-button>
    </div>
    <n-alert v-for="message in messages" :key="message" type="warning" :show-icon="false">
      {{ message }}
    </n-alert>
    <n-tree
      :data="nodes"
      :expanded-keys="expandedKeys"
      :node-props="nodeProps"
      :render-label="renderLabel"
      :indent="8"
      :theme-overrides="TREE_THEME"
      block-line
      virtual-scroll
      class="object-tree"
      @update:expanded-keys="expand"
    />
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

.explorer-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.explorer-title {
  color: #666;
  font-size: 0.8rem;
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

.object-tree :deep(.n-tree-node-content) {
  padding: 0 2px 0 1px;
}
</style>
