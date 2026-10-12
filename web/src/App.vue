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
  NMessageProvider,
  dateFrFR,
  frFR,
} from 'naive-ui'

import ConnectionPanel from './components/ConnectionPanel.vue'
</script>

<template>
  <n-config-provider :locale="frFR" :date-locale="dateFrFR">
    <n-message-provider>
      <n-layout has-sider class="app">
        <n-layout-sider bordered :width="300" content-class="app-sider">
          <h1 class="app-title">Prisme</h1>
          <connection-panel />
        </n-layout-sider>
        <n-layout-content content-class="app-content" />
      </n-layout>
    </n-message-provider>
  </n-config-provider>
</template>

<style>
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

.app-title {
  margin: 0;
  font-size: 1.25rem;
}
</style>
