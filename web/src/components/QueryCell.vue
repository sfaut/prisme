<!--
  Cellule SQL : éditeur, exécution, limite de lignes, état et résultats.

  La cellule ouvre une session de requêtes sur la connexion choisie et la
  garde ouverte tant qu'elle est affichée. Ctrl-Entrée ou « Exécuter » lance
  la requête sous le curseur ; Ctrl-Maj-Entrée ou « Tout exécuter » lance
  toutes les requêtes en parallèle. « Stopper » les arrête toutes.
  « Enjoliver » réécrit le texte en « beau SQL », une fois le catalogue de la
  connexion chargé (il donne la casse des fonctions de l'utilisateur).
  Chaque requête a son onglet de résultat (`ResultTab.vue`), avec sa barre
  d'avancement. Un onglet se renomme, s'épingle (il reste quand la requête
  est relancée) ou se ferme (sa requête est arrêtée si elle tourne).
  La limite de lignes choisie ici devient la limite par défaut de l'utilisateur.
-->
<script setup lang="ts">
import { Play, Square, WandSparkles } from 'lucide-vue-next'
import { TabsContent, TabsList, TabsRoot, TabsTrigger } from 'reka-ui'
import { computed, onMounted, ref, useTemplateRef, watch } from 'vue'

import { beautify } from '@/api/beautify'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import { type QueryTab, useQuerySession } from '@/api/querySession'
import { useCatalogStore } from '@/stores/catalog'
import { useSettingsStore } from '@/stores/settings'

import QueryTabView from './QueryTabView.vue'
import ResultTab from './ResultTab.vue'
import SqlEditor from './SqlEditor.vue'
import { completionSchema } from './sqlCompletion'
import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import UiSelect from './ui/UiSelect.vue'

const props = defineProps<{
  /**
   * Connexion sur laquelle s'exécutent les requêtes.
   */
  connection: ConnectionSummary
}>()

const emit = defineEmits<{
  /**
   * La session est fermée et l'utilisateur demande à la rouvrir.
   */
  reconnect: []
  /**
   * La base en cours a changé (connexion, `USE`) ; `null` si aucune.
   */
  database: [database: string | null]
}>()

/**
 * Texte SQL de la cellule.
 */
const sql = defineModel<string>('sql', { required: true })

const session = useQuerySession(props.connection.id)
const catalog = useCatalogStore()
const state = session.state
const settings = useSettingsStore()

/**
 * Choix de la limite de lignes, formatés pour la liste.
 */
const limitOptions = computed(() =>
  (settings.settings?.row_limit_choices ?? []).map((choice) => ({
    label: `${choice.toLocaleString('fr-FR')} lignes`,
    value: choice,
  })),
)

/**
 * Variante et version du serveur, pour l'en-tête de la cellule.
 */
const serverLabel = computed(() => {
  const server = state.server
  if (!server) {
    return 'Connexion…'
  }
  return `${server.flavor === 'mariadb' ? 'MariaDB' : 'MySQL'} ${server.version}`
})

const editor = useTemplateRef<InstanceType<typeof SqlEditor>>('editor')

/**
 * Onglet affiché : son identifiant.
 */
const selectedTab = ref<string | number>(0)

// Les onglets d'un nouveau lot arrivent : le premier s'affiche. Sinon,
// l'onglet affiché a disparu : le premier qui reste prend sa place.
watch(
  () => state.tabs,
  (tabs, previous) => {
    const fresh = tabs.find((tab) => !previous?.some((old) => old.id === tab.id))
    if (fresh) {
      selectedTab.value = fresh.id
    } else if (!tabs.some((tab) => tab.id === selectedTab.value)) {
      selectedTab.value = tabs[0]?.id ?? 0
    }
  },
)

/**
 * Libellé par défaut d'un onglet : numéro et début de la requête.
 *
 * @param tab Onglet de la requête.
 */
function tabLabel(tab: QueryTab): string {
  const short = tab.sql.replace(/\s+/g, ' ')
  return `${tab.query + 1} · ${short.length > 40 ? `${short.slice(0, 40)}…` : short}`
}

/**
 * Ferme un onglet ; s'il était affiché, son voisin de droite (ou de gauche)
 * prend sa place. Sa requête est arrêtée si elle tourne.
 *
 * @param tab Onglet à fermer.
 */
function closeTab(tab: QueryTab): void {
  if (selectedTab.value === tab.id) {
    const index = state.tabs.indexOf(tab)
    const neighbour = state.tabs[index + 1] ?? state.tabs[index - 1]
    selectedTab.value = neighbour?.id ?? 0
  }
  session.closeTab(tab.id)
}

/**
 * Exécute la requête sous le curseur.
 *
 * @param cursor Position du curseur ; absente, celle de l'éditeur.
 */
function run(cursor?: number): void {
  if (sql.value.trim() !== '') {
    session.run(sql.value, cursor ?? editor.value?.cursor() ?? 0)
  }
}

/**
 * Lance une action dès que la session est prête : tout de suite si elle
 * l'est, sinon à la fin de son ouverture. Rien si elle se ferme avant.
 *
 * @param action Action à lancer.
 */
function whenReady(action: () => void): void {
  if (state.status === 'ready') {
    action()
    return
  }
  const stop = watch(
    () => state.status,
    (status) => {
      if (status === 'ready') {
        action()
      }
      if (status !== 'connecting') {
        stop()
      }
    },
  )
}

/**
 * Exécute la requête sous le curseur dès que la session est prête.
 *
 * @param cursor Position du curseur ; absente, celle de l'éditeur.
 */
function runWhenReady(cursor?: number): void {
  whenReady(() => run(cursor))
}

/**
 * Change la base en cours de la session (comme `USE`) dès qu'elle est prête.
 *
 * @param database Nom de la base.
 */
function changeDatabaseWhenReady(database: string): void {
  whenReady(() => session.changeDatabase(database))
}

/**
 * Bases, tables et colonnes proposées par la complétion de l'éditeur,
 * d'après le catalogue de la connexion (chargé par l'explorateur).
 */
const completion = computed(() =>
  completionSchema(catalog.catalogOf(props.connection.id)?.schemas ?? []),
)

/**
 * Le catalogue de la connexion est-il entièrement chargé ? Sans lui,
 * l'enjoliveur ne connaît pas les fonctions de l'utilisateur et les
 * mettrait en capitales : « Enjoliver » attend donc la fin du chargement.
 */
const catalogReady = computed(() => catalog.catalogOf(props.connection.id)?.done === true)

/**
 * Enjolive le texte de l'éditeur (mots-clefs en capitales...). Ctrl-Z annule.
 * Les fonctions et procédures du catalogue gardent la casse de leur définition.
 */
async function prettify(): Promise<void> {
  try {
    sql.value = await beautify(sql.value, catalog.routineNames(props.connection.id))
  } catch (failure) {
    state.error = failure instanceof Error ? failure.message : String(failure)
  }
}

/**
 * Exécute toutes les requêtes de l'éditeur, en parallèle.
 */
function runAll(): void {
  if (sql.value.trim() !== '') {
    session.run(sql.value)
  }
}

/**
 * Change la limite de lignes par défaut de l'utilisateur.
 *
 * @param rowLimit Nouvelle limite.
 */
async function changeLimit(rowLimit: number): Promise<void> {
  try {
    await settings.setRowLimit(rowLimit)
  } catch (failure) {
    state.error = failure instanceof Error ? failure.message : String(failure)
  }
}

onMounted(async () => {
  if (settings.settings === null) {
    try {
      await settings.load()
    } catch (failure) {
      state.error = failure instanceof Error ? failure.message : String(failure)
    }
  }
})

watch(
  () => state.database,
  (database) => emit('database', database),
)

defineExpose({ run, runWhenReady, changeDatabaseWhenReady })
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col gap-gap">
    <sql-editor
      ref="editor"
      v-model="sql"
      :flavor="state.server?.flavor ?? 'mysql'"
      :schema="completion"
      :default-schema="state.database"
      @run="run"
      @run-all="runAll"
    />

    <div class="flex items-center gap-gap">
      <ui-button
        label="Exécuter"
        :icon="Play"
        variant="solid"
        tone="accent"
        :disabled="state.status !== 'ready' && state.status !== 'running'"
        :loading="state.status === 'running'"
        @click="run()"
      />
      <ui-button
        label="Tout exécuter"
        title="Toutes les requêtes, en parallèle (Ctrl-Maj-Entrée)"
        :disabled="state.status !== 'ready'"
        @click="runAll"
      />
      <ui-button
        label="Stopper"
        :icon="Square"
        tone="error"
        title="Arrête toutes les requêtes et tous les chargements en cours"
        :disabled="state.status !== 'running'"
        @click="session.cancel()"
      />
      <ui-button
        label="Enjoliver"
        :icon="WandSparkles"
        :disabled="sql.trim() === '' || !catalogReady"
        :title="
          catalogReady
            ? 'Réécrit les requêtes en « beau SQL » : mots-clefs en capitales (Ctrl-Z annule)'
            : 'Disponible une fois le catalogue de la connexion chargé'
        "
        @click="prettify"
      />
      <ui-select
        :model-value="settings.settings?.row_limit"
        :items="limitOptions"
        label="Limite de lignes"
        class="w-32"
        @update:model-value="(limit) => limit !== undefined && changeLimit(limit)"
      />
      <span class="text-trim text-fg-muted">{{ connection.name }} · {{ serverLabel }}</span>
    </div>

    <ui-alert v-if="state.status === 'closed'" tone="warning" title="Session fermée.">
      <ui-button label="Reconnecter" @click="emit('reconnect')" />
    </ui-alert>
    <ui-alert v-if="state.error" tone="error" :title="state.error" />

    <!--
      La grille de résultats doit avoir une hauteur bornée : sinon sa zone de
      défilement grandit avec le résultat, toutes les lignes sont dessinées et
      l'interface se fige. Chaque niveau, jusqu'à la grille, occupe donc la
      place restante sans pouvoir la dépasser (flex-1 et min-h-0). Les
      onglets cachés restent montés (`unmount-on-hide` à faux) : la grille
      garde son défilement.
    -->
    <tabs-root
      v-if="state.tabs.length > 0"
      v-model="selectedTab"
      :unmount-on-hide="false"
      class="flex min-h-0 flex-1 flex-col gap-1"
    >
      <tabs-list class="flex overflow-x-auto border-b border-line">
        <tabs-trigger
          v-for="tab in state.tabs"
          :key="tab.id"
          :value="tab.id"
          as="div"
          class="group relative flex h-control max-w-tab min-w-0 shrink-0 cursor-pointer items-center rounded-t-control ps-control-x pe-1 text-ui font-medium text-fg-muted outline-none select-none hover:text-fg-strong focus-visible:bg-surface-muted data-[state=active]:bg-surface-elevated data-[state=active]:text-fg-strong"
        >
          <result-tab
            :tab="tab"
            :default-label="tabLabel(tab)"
            @close="closeTab(tab)"
            @toggle-pin="tab.pinned = !tab.pinned"
            @rename="(name) => (tab.name = name)"
          />
        </tabs-trigger>
      </tabs-list>
      <tabs-content
        v-for="tab in state.tabs"
        :key="tab.id"
        :value="tab.id"
        class="flex min-h-0 flex-1 outline-none"
      >
        <query-tab-view :tab="tab" />
      </tabs-content>
    </tabs-root>
  </div>
</template>
