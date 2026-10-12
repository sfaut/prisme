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

import { type ExplorerNode, catalogNodes } from './explorerNodes'

const props = defineProps<{
  /**
   * Identifiant de la connexion explorée.
   */
  connectionId: number
}>()

const emit = defineEmits<{
  /**
   * L'utilisateur veut voir le contenu d'une table.
   */
  'open-table': [schema: string, table: string]
}>()

const catalog = useCatalogStore()

/**
 * Arbre compact : lignes de 20 pixels sans marge, retrait réduit.
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
  const current = catalog.catalogs[props.connectionId]
  return (current ? catalogNodes(current.schemas) : []) as unknown as TreeOption[]
})

/**
 * Erreurs de lecture signalées par le serveur pendant la réception.
 */
const readErrors = computed(() => catalog.catalogs[props.connectionId]?.errors ?? [])

/**
 * Le catalogue est-il encore en cours de réception ?
 */
const receiving = computed(() => {
  const current = catalog.catalogs[props.connectionId]
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
 * séparés : le nom d'une colonne de clef primaire est en gras, celui d'une
 * colonne de clef étrangère souligné (et préfixé par « # »).
 *
 * @param info Nœud affiché.
 */
function renderLabel({ option }: { option: TreeOption }) {
  const node = option as unknown as ExplorerNode
  if (node.column === undefined) {
    return node.label
  }
  return [
    h(
      'span',
      { class: { 'primary-key': node.column.primaryKey, 'foreign-key': node.column.foreignKey } },
      node.column.name,
    ),
    h('span', { class: 'column-details' }, ` · ${node.column.details}`),
  ]
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
        :indent="12"
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
  font-size: 10px;
}

.object-tree :deep(.n-tree-node-switcher) {
  width: 14px;
  height: 20px;
}

.object-tree :deep(.primary-key) {
  font-weight: 700;
}

.object-tree :deep(.foreign-key) {
  text-decoration: underline;
}

.object-tree :deep(.n-tree-node-content) {
  padding: 0 4px 0 2px;
}
</style>
