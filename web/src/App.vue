<!--
  Disposition de l'écran : à gauche, les connexions et l'explorateur d'objets ;
  à droite, la cellule SQL et ses résultats.

  Les fournisseurs de Naive UI (configuration, messages) enveloppent toute
  l'application : textes des composants en français, messages éphémères
  (`useMessage`) disponibles partout.
-->
<script setup lang="ts">
import {
  NConfigProvider,
  NLayout,
  NLayoutContent,
  NLayoutSider,
  NEmpty,
  NMessageProvider,
  dateFrFR,
  frFR,
} from 'naive-ui'
import { nextTick, ref, useTemplateRef, watch } from 'vue'

import { useConnectionsStore } from './stores/connections'

import ConnectionPanel from './components/ConnectionPanel.vue'
import ObjectExplorer from './components/ObjectExplorer.vue'
import QueryCell from './components/QueryCell.vue'
import { selectAllQuery } from './components/explorerNodes'

const connections = useConnectionsStore()

/**
 * Texte de la cellule SQL.
 */
const sql = ref('')

/**
 * Base en cours de la session de requêtes (elle change avec `USE`), mise en
 * gras dans l'explorateur.
 */
const currentDatabase = ref<string | null>(null)

// Nouvelle connexion : sa base en cours n'est connue qu'à l'ouverture de sa session.
watch(
  () => connections.selectedId,
  () => {
    currentDatabase.value = null
  },
)

/**
 * Nombre de reconnexions demandées : changer la clef de la cellule rouvre sa session.
 */
const attempt = ref(0)

const cell = useTemplateRef<InstanceType<typeof QueryCell>>('cell')

/**
 * Affiche le contenu d'une table choisie dans l'explorateur.
 *
 * @param schema Nom de la base.
 * @param table Nom de la table.
 */
async function openTable(schema: string, table: string): Promise<void> {
  sql.value = selectAllQuery(schema, table)
  await nextTick()
  cell.value?.run(0)
}
</script>

<template>
  <n-config-provider :locale="frFR" :date-locale="dateFrFR">
    <n-message-provider>
      <n-layout has-sider position="absolute" class="app">
        <n-layout-sider bordered :width="300" content-class="app-sider">
          <h1 class="app-title">Prisme</h1>
          <connection-panel />
          <object-explorer
            v-if="connections.selectedId !== null"
            :key="connections.selectedId"
            :connection-id="connections.selectedId"
            :current-database="currentDatabase"
            @open-table="openTable"
          />
        </n-layout-sider>
        <n-layout-content content-class="app-content">
          <query-cell
            v-if="connections.selected"
            ref="cell"
            :key="`${connections.selected.id}-${attempt}`"
            v-model:sql="sql"
            :connection="connections.selected"
            @reconnect="attempt++"
            @database="currentDatabase = $event"
          />
          <n-empty v-else description="Choisissez ou créez une connexion" class="app-empty" />
        </n-layout-content>
      </n-layout>
    </n-message-provider>
  </n-config-provider>
</template>

<style>
* {
  box-sizing: border-box;
}

html,
body,
#app {
  height: 100%;
  margin: 0;
}

.app {
  height: 100%;
}

.app-sider {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 12px;
}

.app-content {
  display: flex;
  flex-direction: column;
  gap: 12px;
  height: 100%;
  padding: 12px;
  box-sizing: border-box;
}

.app-empty {
  margin: auto;
}

.app-title {
  margin: 0;
  font-size: 1.25rem;
}
</style>
