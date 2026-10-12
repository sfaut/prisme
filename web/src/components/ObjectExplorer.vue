<!--
  Explorateur d'objets de la connexion choisie : bases, tables et colonnes.

  Chaque niveau se charge quand l'utilisateur ouvre un nœud. Un double clic
  sur une table émet `open-table`, pour en afficher le contenu.
-->
<script setup lang="ts">
import { NAlert, NTree, type TreeOption } from 'naive-ui'
import { onMounted, ref } from 'vue'

import { listColumns, listSchemas, listTables } from '@/api/explorer'

import { type ExplorerNode, columnNodes, schemaNodes, tableNodes } from './explorerNodes'

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

/**
 * Nœuds de premier niveau : les bases.
 */
const nodes = ref<ExplorerNode[]>([])

/**
 * Dernière erreur de chargement.
 */
const error = ref<string | null>(null)

/**
 * Charge les enfants d'un nœud ouvert : tables d'une base, ou colonnes d'une table.
 *
 * @param option Nœud ouvert.
 */
async function loadChildren(option: TreeOption): Promise<void> {
  const node = option as unknown as ExplorerNode
  error.value = null
  try {
    node.children =
      node.table === undefined
        ? tableNodes(node.schema, await listTables(props.connectionId, node.schema))
        : columnNodes(
            node.schema,
            node.table,
            await listColumns(props.connectionId, node.schema, node.table),
          )
  } catch (failure) {
    node.children = []
    error.value = failure instanceof Error ? failure.message : String(failure)
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
      if (node.table !== undefined && !node.isLeaf) {
        emit('open-table', node.schema, node.table)
      }
    },
  }
}

onMounted(async () => {
  try {
    nodes.value = schemaNodes(await listSchemas(props.connectionId))
  } catch (failure) {
    error.value = failure instanceof Error ? failure.message : String(failure)
  }
})
</script>

<template>
  <div class="object-explorer">
    <n-alert v-if="error" type="error" :show-icon="false">{{ error }}</n-alert>
    <n-tree
      :data="nodes as unknown as TreeOption[]"
      :on-load="loadChildren"
      :node-props="nodeProps"
      block-line
      remote
      virtual-scroll
      class="object-tree"
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

.object-tree {
  flex: 1;
  min-height: 0;
  font-size: 0.85rem;
}
</style>
