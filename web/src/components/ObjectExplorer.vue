<!--
  Explorateur d'objets de la connexion choisie : bases, tables et colonnes.

  Le catalogue arrive au fil de l'eau dès que la connexion est choisie : les
  bases d'abord, puis leurs objets (tables, vues, procédures, fonctions), puis
  le détail des tables (colonnes, index, clefs étrangères, déclencheurs),
  chaque partie affichée dès sa réception. Il reste ensuite en mémoire : revenir sur une connexion déjà
  explorée est immédiat. « Rafraîchir » relit le catalogue. Un double clic
  sur une table ou une vue émet `open-table`, pour en afficher le contenu.
-->
<script setup lang="ts">
import { NAlert, NButton, NSpin, NTree, type TreeOption } from 'naive-ui'
import { computed, h, onMounted, ref } from 'vue'

import { useCatalogStore } from '@/stores/catalog'

import { hashIcon, keyIcon } from './columnIcons'
import { type ExplorerNode, catalogNodes } from './explorerNodes'

const props = defineProps<{
  /**
   * Identifiant de la connexion explorée.
   */
  connectionId: number
  /**
   * Base en cours de la session de requêtes, affichée en gras ; `null` si aucune.
   */
  currentDatabase: string | null
}>()

const emit = defineEmits<{
  /**
   * L'utilisateur veut voir le contenu d'une table.
   */
  'open-table': [schema: string, table: string]
}>()

const catalog = useCatalogStore()

/**
 * Arbre compact : lignes de 20 pixels sans marge, retrait et espacement horizontal réduits.
 * `nodeHeight` sert aussi au défilement virtuel de l'arbre. La taille du
 * texte est fixée en CSS (`.object-tree`) : avec le défilement virtuel, Naive
 * UI ignore la variable `fontSize` de son thème.
 */
const TREE_THEME = { nodeHeight: '20px', nodeWrapperPadding: '0' }

/**
 * Lecture du catalogue en cours ?
 */
const loading = ref(false)

/**
 * Dernière erreur de lecture.
 */
const error = ref<string | null>(null)

/**
 * Arbre complet, construit à partir du catalogue en mémoire.
 */
const nodes = computed(() => {
  const current = catalog.catalogOf(props.connectionId)
  return (current
    ? catalogNodes(current.schemas, props.currentDatabase)
    : []) as unknown as TreeOption[]
})

/**
 * Erreurs de lecture signalées par le serveur pendant la réception.
 */
// Copie : le tableau du catalogue est brut, il garde la même référence en grandissant.
const readErrors = computed(() => [...(catalog.catalogOf(props.connectionId)?.errors ?? [])])

/**
 * Le catalogue est-il encore en cours de réception ?
 */
const receiving = computed(() => {
  const current = catalog.catalogOf(props.connectionId)
  return current !== undefined && !current.done
})

/**
 * Lit le catalogue, depuis la mémoire ou depuis le serveur.
 *
 * @param refresh Relit le catalogue sur le serveur même s'il est en mémoire.
 */
async function load(refresh: boolean): Promise<void> {
  loading.value = true
  error.value = null
  try {
    await catalog.load(props.connectionId, refresh)
  } catch (failure) {
    error.value = failure instanceof Error ? failure.message : String(failure)
  } finally {
    loading.value = false
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
        emit('open-table', node.schema, node.table)
      }
    },
  }
}

/**
 * Affiche le libellé d'un nœud. Pour une colonne, le nom et le détail sont
 * séparés : le nom d'une colonne indexée (clef primaire comprise) est en
 * gras ; celui d'une colonne de clef primaire est précédé d'une clef, celui
 * d'une colonne de clef étrangère d'un croisillon.
 *
 * @param info Nœud affiché.
 */
function renderLabel({ option }: { option: TreeOption }) {
  const node = option as unknown as ExplorerNode
  if (node.column === undefined) {
    // Toujours un élément, en gras ou non : passer d'un texte nu à un élément
    // (ou l'inverse) fait échouer la mise à jour de l'arbre par Vue.
    return h('span', { class: { 'current-database': node.current } }, node.label)
  }
  // Les symboles sont placés dans la marge, à gauche du nom, comme des puces :
  // les noms de toutes les colonnes restent alignés.
  return h('span', { class: 'column-label' }, [
    h('span', { class: 'column-marks' }, [
      node.column.primaryKey ? keyIcon() : null,
      node.column.foreignKey ? hashIcon() : null,
    ]),
    h('span', { class: { indexed: node.column.indexed } }, node.column.name),
    h('span', { class: 'column-details' }, ` · ${node.column.details}`),
  ])
}

onMounted(() => load(false))
</script>

<template>
  <div class="object-explorer">
    <div class="explorer-toolbar">
      <span class="explorer-title">Objets<template v-if="receiving"> · chargement…</template></span>
      <n-button size="tiny" :loading="loading" @click="load(true)">Rafraîchir</n-button>
    </div>
    <n-alert v-if="error" type="error" :show-icon="false">{{ error }}</n-alert>
    <n-alert v-for="message in readErrors" :key="message" type="warning" :show-icon="false">
      {{ message }}
    </n-alert>
    <n-spin :show="loading && nodes.length === 0" class="explorer-spin">
      <n-tree
        :data="nodes"
        :node-props="nodeProps"
        :render-label="renderLabel"
        :indent="8"
        :theme-overrides="TREE_THEME"
        block-line
        virtual-scroll
        class="object-tree"
      />
    </n-spin>
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

.explorer-spin,
.explorer-spin :deep(.n-spin-content) {
  display: flex;
  flex: 1;
  flex-direction: column;
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
  position: relative;
}

.object-tree :deep(.column-marks) {
  position: absolute;
  top: 50%;
  right: 100%;
  display: flex;
  gap: 1px;
  margin-right: 3px;
  transform: translateY(-50%);
}

.object-tree :deep(.indexed) {
  font-weight: 700;
}

.object-tree :deep(.n-tree-node-content) {
  padding: 0 2px 0 1px;
}
</style>
