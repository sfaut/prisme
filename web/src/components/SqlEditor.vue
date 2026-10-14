<!--
  Éditeur SQL, basé sur CodeMirror 6.

  Coloration selon le dialecte du serveur (MySQL ou MariaDB), numéros de
  ligne, thème clair ou sombre comme l'interface (`editorTheme.ts`),
  historique d'annulation, complétion des noms seulement : bases, tables,
  colonnes, fonctions et procédures (espace de noms construit à partir du
  catalogue, voir `sqlCompletion.ts`). Les mots-clefs (SELECT, UPDATE...)
  ne sont pas proposés, et une proposition doit commencer par le texte
  tapé (ou en reprendre les initiales) : pas de correspondance dispersée. La requête que Ctrl-Entrée exécuterait est
  surlignée (`currentStatement.ts`) et les erreurs de syntaxe sont soulignées,
  avec leur message au survol : le texte est analysé par le serveur peu
  après chaque frappe. Ctrl-Entrée (Cmd-Entrée sur Mac) émet `run`
  avec la position du curseur ; Ctrl-Maj-Entrée émet `run-all`. Le texte se
  lie avec `v-model`.
-->
<script setup lang="ts">
import { autocompletion } from '@codemirror/autocomplete'
import { LanguageSupport } from '@codemirror/language'
import { type Diagnostic, setDiagnostics } from '@codemirror/lint'
import { MariaSQL, MySQL, type SQLNamespace, schemaCompletionSource } from '@codemirror/lang-sql'
import { Compartment } from '@codemirror/state'
import { keymap } from '@codemirror/view'
import { EditorView, basicSetup } from 'codemirror'
import { onBeforeUnmount, onMounted, useTemplateRef, watch } from 'vue'

import { analyzeSql } from '@/api/analyze'
import type { Flavor } from '@/api/generated/Flavor'

import { currentStatement, setStatements } from './currentStatement'
import { editorTheme } from './editorTheme'

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
 * Fait analyser le texte par le serveur et transmet le résultat à l'éditeur
 * (requêtes et erreurs de syntaxe),
 * sauf si le texte a changé entre-temps (une autre analyse suivra). Un échec
 * est sans conséquence : l'éditeur garde l'analyse précédente.
 */
async function analyze(): Promise<void> {
  const doc = view?.state.doc
  if (doc === undefined) {
    return
  }
  try {
    const analysis = await analyzeSql(doc.toString(), props.flavor)
    if (view?.state.doc === doc) {
      const diagnostics: Diagnostic[] = analysis.errors.map((error) => ({
        from: error.start,
        to: error.end,
        severity: 'error',
        message: error.message,
      }))
      view.dispatch(setDiagnostics(view.state, diagnostics), {
        effects: setStatements.of(analysis.statements),
      })
    }
  } catch {
    // Serveur injoignable : l'analyse reviendra à la prochaine frappe.
  }
}

/**
 * Langue CodeMirror de l'éditeur : dialecte et complétion, d'après les propriétés.
 */
function currentLanguage(): LanguageSupport {
  const dialect = props.flavor === 'mariadb' ? MariaSQL : MySQL
  // Langue montée à la main plutôt que par `sql()`, qui ajouterait la
  // complétion des mots-clefs : seuls les noms du catalogue sont proposés.
  const names = schemaCompletionSource({
    dialect,
    schema: props.schema,
    defaultSchema: props.defaultSchema ?? undefined,
  })
  return new LanguageSupport(dialect.language, [dialect.language.data.of({ autocomplete: names })])
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
      // Complétion stricte : début du nom ou initiales, jamais des lettres éparses.
      autocompletion({ filterStrict: true }),
      editorTheme(),
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

// Le dialecte change à l'ouverture de la session : l'analyse est à refaire.
watch(() => props.flavor, scheduleAnalysis)

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
  block-size: 100%;
  border: var(--line-width) solid var(--line);
  border-radius: var(--control-radius);
  overflow: hidden;
}

.sql-editor :deep(.cm-editor) {
  block-size: 100%;
  font-size: var(--editor-font-size);
}

.sql-editor :deep(.cm-scroller) {
  font-family: var(--font-code);
}
</style>
