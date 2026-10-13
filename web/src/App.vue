<!--
  Disposition de l'écran : à gauche, les connexions et l'explorateur d'objets,
  dans un panneau dont on règle la largeur en faisant glisser son bord droit ;
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
import SiderResizer from './components/SiderResizer.vue'
import { selectAllQuery } from './components/explorerNodes'
import { clampSiderWidth, loadSiderWidth } from './components/siderWidth'

const connections = useConnectionsStore()

/**
 * Largeur du panneau de gauche, en pixels.
 */
const siderWidth = ref(clampSiderWidth(loadSiderWidth(), window.innerWidth))

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
 * Nombre de reconnexions demandées : changer la clef de la cellule rouvre sa
 * session. Modifier la connexion la rouvre aussi.
 */
const attempt = ref(0)

const cell = useTemplateRef<InstanceType<typeof QueryCell>>('cell')

/**
 * Affiche le contenu d'une table choisie dans l'explorateur. Une table d'une
 * autre source choisit cette connexion et prépare la requête sans l'exécuter :
 * la session de la nouvelle connexion n'est pas encore ouverte.
 *
 * @param connectionId Identifiant de la connexion de la table.
 * @param schema Nom de la base.
 * @param table Nom de la table.
 */
async function openTable(connectionId: number, schema: string, table: string): Promise<void> {
  sql.value = selectAllQuery(schema, table)
  if (connectionId !== connections.selectedId) {
    connections.selectedId = connectionId
    return
  }
  await nextTick()
  cell.value?.run(0)
}
</script>

<template>
  <n-config-provider :locale="frFR" :date-locale="dateFrFR">
    <n-message-provider>
      <n-layout has-sider position="absolute" class="app">
        <n-layout-sider
          bordered
          :width="siderWidth"
          class="app-sider-panel"
          content-class="app-sider"
        >
          <h1 class="app-title">Prisme</h1>
          <connection-panel />
          <object-explorer
            :selected-id="connections.selectedId"
            :current-database="currentDatabase"
            @open-table="openTable"
          />
        </n-layout-sider>
        <sider-resizer v-model="siderWidth" />
        <n-layout-content content-class="app-content">
          <query-cell
            v-if="connections.selected"
            ref="cell"
            :key="`${connections.selected.id}-${connections.versions[connections.selected.id] ?? 0}-${attempt}`"
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

/* Naive UI anime la largeur de son panneau latéral (min-width et max-width,
   0,3 s), alors que son contenu prend aussitôt la nouvelle largeur et se cale
   à droite : pendant un redimensionnement à la souris, le contenu débordait
   à gauche et sur la partie droite, le temps de l'animation. Le panneau suit
   donc la poignée sans animation. */
.app-sider-panel {
  transition: none;
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
