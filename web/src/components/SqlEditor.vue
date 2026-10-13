<!--
  Éditeur SQL, basé sur CodeMirror 6.

  Coloration selon le dialecte du serveur (MySQL ou MariaDB), numéros de
  ligne, historique d'annulation, complétion des mots-clefs, des bases, des
  tables et des colonnes (espace de noms construit à partir du catalogue,
  voir `sqlCompletion.ts`). La requête que Ctrl-Entrée exécuterait est
  surlignée (`currentStatement.ts`) : le texte est analysé par le serveur
  peu après chaque frappe. Ctrl-Entrée (Cmd-Entrée sur Mac) émet `run`
  avec la position du curseur ; Ctrl-Maj-Entrée émet `run-all`. Le texte se
  lie avec `v-model`.
-->
<script setup lang="ts">
import { MariaSQL, MySQL, type SQLNamespace, sql } from '@codemirror/lang-sql'
import { Compartment } from '@codemirror/state'
import { keymap } from '@codemirror/view'
import { EditorView, basicSetup } from 'codemirror'
import { onBeforeUnmount, onMounted, useTemplateRef, watch } from 'vue'

import { analyzeSql } from '@/api/analyze'
import type { Flavor } from '@/api/generated/Flavor'

import { currentStatement, setStatements } from './currentStatement'

const props = defineProps<{
  /**
   * Dialecte du serveur cible, pour la coloration et les mots-clefs.
   */
  flavor: Flavor
  /**
   * Bases, tables et colonnes proposées par la complétion.
   */
  schema: SQLNamespace
  /**
   * Base en cours : ses tables se complètent sans préfixe. `null` si aucune.
   */
  defaultSchema: string | null
}>()

/**
 * Texte de l'éditeur.
 */
const text = defineModel<string>({ required: true })

const emit = defineEmits<{
  /**
   * L'utilisateur demande l'exécution de la requête sous le curseur (Ctrl-Entrée).
   */
  run: [cursor: number]
  /**
   * L'utilisateur demande l'exécution de toutes les requêtes (Ctrl-Maj-Entrée).
   */
  'run-all': []
}>()

const host = useTemplateRef<HTMLDivElement>('host')

/**
 * Emplacement de la langue dans la configuration, pour la changer avec le
 * dialecte, le catalogue ou la base en cours.
 */
const language = new Compartment()

let view: EditorView | null = null

/**
 * Délai entre la dernière frappe et l'analyse du texte, en millisecondes.
 */
const ANALYSIS_DELAY = 250

/**
 * Analyse différée en attente, absente si aucune.
 */
let analysisTimer: ReturnType<typeof setTimeout> | undefined

/**
 * Programme l'analyse du texte, en remplaçant celle qui attendait.
 */
function scheduleAnalysis(): void {
  clearTimeout(analysisTimer)
  analysisTimer = setTimeout(analyze, ANALYSIS_DELAY)
}

/**
 * Fait analyser le texte par le serveur et transmet le résultat à l'éditeur,
 * sauf si le texte a changé entre-temps (une autre analyse suivra). Un échec
 * est sans conséquence : l'éditeur garde l'analyse précédente.
 */
async function analyze(): Promise<void> {
  const doc = view?.state.doc
  if (doc === undefined) {
    return
  }
  try {
    const analysis = await analyzeSql(doc.toString())
    if (view?.state.doc === doc) {
      view.dispatch({ effects: setStatements.of(analysis.statements) })
    }
  } catch {
    // Serveur injoignable : l'analyse reviendra à la prochaine frappe.
  }
}

/**
 * Langue CodeMirror de l'éditeur : dialecte et complétion, d'après les propriétés.
 */
function currentLanguage() {
  return sql({
    dialect: props.flavor === 'mariadb' ? MariaSQL : MySQL,
    schema: props.schema,
    defaultSchema: props.defaultSchema ?? undefined,
  })
}

onMounted(() => {
  view = new EditorView({
    parent: host.value!,
    doc: text.value,
    extensions: [
      // Avant basicSetup, pour passer devant ses propres raccourcis.
      keymap.of([
        {
          key: 'Mod-Enter',
          run: (editor) => {
            emit('run', editor.state.selection.main.head)
            return true
          },
        },
        {
          key: 'Mod-Shift-Enter',
          run: () => {
            emit('run-all')
            return true
          },
        },
      ]),
      basicSetup,
      language.of(currentLanguage()),
      currentStatement(),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          text.value = update.state.doc.toString()
          scheduleAnalysis()
        }
      }),
    ],
  })
  scheduleAnalysis()
})

// Texte modifié de l'extérieur (et non par la frappe) : on remplace le contenu.
watch(text, (value) => {
  if (view && value !== view.state.doc.toString()) {
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } })
  }
})

watch(
  () => [props.flavor, props.schema, props.defaultSchema],
  () => view?.dispatch({ effects: language.reconfigure(currentLanguage()) }),
)

onBeforeUnmount(() => {
  clearTimeout(analysisTimer)
  view?.destroy()
  view = null
})

/**
 * Position du curseur dans le texte, en unités UTF-16 (0 avant le montage).
 */
function cursor(): number {
  return view?.state.selection.main.head ?? 0
}

defineExpose({ cursor })
</script>

<template>
  <div ref="host" class="sql-editor" />
</template>

<style scoped>
.sql-editor {
  border: 1px solid var(--border-color, #ccc);
  border-radius: 4px;
  overflow: hidden;
}

.sql-editor :deep(.cm-editor) {
  min-height: 8rem;
  max-height: 24rem;
  font-size: 0.9rem;
}

.sql-editor :deep(.cm-scroller) {
  font-family: ui-monospace, 'Cascadia Code', Consolas, monospace;
}
</style>
