<!--
  Settings of a source held in files (SQLite), on a single page, in lists of
  settings by subject: Général (name), Base principale (its origin and its
  location), Bases attachées (zero to ten), Session (queries run on
  connection, keep-alive delay), Navigateur (schema `temp`). Shown by the
  connections manager for a saved connection and for the settings step of a
  creation.

  The main database (schema `main`) comes from an existing file, a new file
  created empty when the connection is saved, or memory: empty at each
  session, lost when it closes. Each attached database has its schema, its
  origin and its location; every one attaches at each opening of a session.
  An attached database of the saved connection that is removed stays struck
  through, its file left on the disk, and "Rétablir" brings it back until
  saving; one added since goes at once.
  A location is an absolute path on the machine of Prisme's server, or the
  address of a remote file (`https`), copied and read only: the pill beside
  the field shows the protocol read from the typing.

  The maximum lengths repeat the server's (`connectionForm.ts`); the other
  checks (schemas, locations, files to create) are made by the server,
  whose message the manager shows.
-->
<script setup lang="ts">
import { FileCheck, FilePlus, MemoryStick, Plus, Trash2, Undo2 } from '@lucide/vue'
import { RadioGroupItem, RadioGroupRoot } from 'reka-ui'
import { type Component, computed, useId } from 'vue'

import {
  ATTACHED_MAX,
  type ConnectionForm,
  type DatabaseInput,
  type DatabaseOrigin,
  LENGTHS,
  MAIN_SCHEMA,
  SESSION_TIMEOUT_MAX,
  schemeOf,
} from './connectionForm'

import ConnectionSchemeBadge from './ConnectionSchemeBadge.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiSettingList from '@/components/ui/UiSettingList.vue'
import UiSettingRow from '@/components/ui/UiSettingRow.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import UiTextarea from '@/components/ui/UiTextarea.vue'

/**
 * Input, changed in place.
 */
const form = defineModel<ConnectionForm>('form', { required: true })

/**
 * Prefix of the ids of the fields, that their names label.
 */
const id = useId()

/**
 * Origins of a database, in the order offered, with what each one means.
 */
const ORIGINS: {
  value: DatabaseOrigin
  label: string
  short: string
  description: string
  icon: Component
}[] = [
  {
    value: 'existing',
    label: 'Fichier existant',
    short: 'Existant',
    description: 'Ouvert tel quel ; une adresse https est copiée et lue seulement.',
    icon: FileCheck,
  },
  {
    value: 'new',
    label: 'Nouveau fichier',
    short: 'À créer',
    description: 'Créé vide à l’enregistrement ; refusé s’il existe déjà.',
    icon: FilePlus,
  },
  {
    value: 'memory',
    label: 'En mémoire',
    short: 'En mémoire',
    description: 'Vide à chaque session, perdue à sa fermeture.',
    icon: MemoryStick,
  },
]

/**
 * Origins offered for an attached database, under their short label: the
 * row of the database leaves the room to its location.
 */
const ATTACHED_ORIGINS = ORIGINS.map(({ value, short, icon }) => ({ value, label: short, icon }))

/**
 * Description of the location of a database, by its origin.
 */
const LOCATION_DESCRIPTIONS: Record<Exclude<DatabaseOrigin, 'memory'>, string> = {
  existing: 'Chemin absolu sur la machine du serveur de Prisme, ou adresse https.',
  new: 'Chemin absolu du fichier à créer, sur la machine du serveur de Prisme.',
}

/**
 * Main database: always the first.
 */
const main = computed(() => form.value.databases[0]!)

/**
 * Databases attached to the main one, in their order.
 */
const attached = computed(() => form.value.databases.slice(1))

/**
 * Number of the databases attached, those removed left out.
 */
const kept = computed(() => attached.value.filter((database) => !database.removed).length)

/**
 * Adds an attached database, to open from an existing file.
 */
function attach(): void {
  form.value.databases.push({
    schema: '',
    origin: 'existing',
    location: '',
    saved: false,
    removed: false,
  })
}

/**
 * Removes an attached database: struck through if it comes from the saved
 * connection, dropped from the input otherwise.
 *
 * @param index Rank of the database among the attached ones.
 */
function remove(index: number): void {
  const database = form.value.databases[index + 1]!
  if (database.saved) {
    database.removed = true
  } else {
    form.value.databases.splice(index + 1, 1)
  }
}

/**
 * Protocol of a database as typed: `memory` in memory, else that of its
 * location.
 *
 * @param database Input of the database.
 */
function schemeOfInput(database: DatabaseInput): string | null {
  return database.origin === 'memory' ? 'memory' : schemeOf(database.location)
}

/**
 * What the removal of a database does: its file stays on the disk, its
 * content in memory goes with the sessions.
 *
 * @param database Input of the database removed.
 */
function removedText(database: DatabaseInput): string {
  const scheme = schemeOfInput(database)
  return scheme === 'memory'
    ? 'Retirée à l’enregistrement, avec son contenu en mémoire.'
    : scheme === 'file'
      ? 'Retirée à l’enregistrement ; le fichier reste sur le disque.'
      : 'Retirée à l’enregistrement.'
}

/**
 * Brings back an attached database removed.
 *
 * @param index Rank of the database among the attached ones.
 */
function restore(index: number): void {
  form.value.databases[index + 1]!.removed = false
}
</script>

<template>
  <ui-setting-list title="Général">
    <ui-setting-row
      label="Nom"
      description="Nom de la source dans le navigateur et les éditeurs."
      :for="`${id}-name`"
    >
      <ui-input
        :id="`${id}-name`"
        v-model="form.name"
        :maxlength="LENGTHS.name"
        placeholder="ventes, inventaire…"
        class="w-60"
      />
    </ui-setting-row>
  </ui-setting-list>

  <ui-setting-list title="Base principale">
    <ui-setting-row
      label="Origine"
      :description="`Sous le schéma ${MAIN_SCHEMA}, où se crée une table écrite sans schéma.`"
      stacked
    >
      <radio-group-root
        v-model="main.origin"
        orientation="horizontal"
        aria-label="Origine de la base principale"
        class="grid grid-cols-3 gap-3"
      >
        <radio-group-item
          v-for="origin in ORIGINS"
          :key="origin.value"
          :value="origin.value"
          class="flex cursor-pointer items-start gap-3 rounded-control border border-line bg-surface p-3 text-start data-[state=checked]:border-primary data-[state=checked]:bg-primary/10 data-[state=unchecked]:hover:bg-surface-raised"
        >
          <component :is="origin.icon" class="size-5 shrink-0 text-primary" aria-hidden="true" />
          <span class="flex min-w-0 flex-col gap-1">
            <span class="font-medium text-fg-strong">{{ origin.label }}</span>
            <span class="text-sm text-fg-secondary">{{ origin.description }}</span>
          </span>
        </radio-group-item>
      </radio-group-root>
    </ui-setting-row>
    <ui-setting-row
      v-if="main.origin !== 'memory'"
      label="Emplacement"
      :description="LOCATION_DESCRIPTIONS[main.origin]"
      :for="`${id}-main`"
      stacked
    >
      <ui-input
        :id="`${id}-main`"
        v-model="main.location"
        :maxlength="LENGTHS.location"
        placeholder="D:\data\ventes.db"
        spellcheck="false"
        class="w-full"
      >
        <template v-if="schemeOf(main.location)" #trailing>
          <connection-scheme-badge :scheme="schemeOf(main.location)!" />
        </template>
      </ui-input>
    </ui-setting-row>
  </ui-setting-list>

  <div class="flex flex-col gap-3">
    <ui-setting-list title="Bases attachées">
      <p v-if="attached.length === 0" class="px-4 py-3 text-sm text-fg-secondary">
        Aucune. Une base attachée se lit sous son schéma, comme
        <code class="font-mono">archive.clients</code>.
      </p>
      <template v-for="(database, index) in attached" :key="index">
        <div v-if="database.removed" class="flex items-center gap-3 px-4 py-3">
          <div class="flex min-w-0 flex-1 flex-col gap-1">
            <div class="flex min-w-0 items-center gap-2 text-fg-secondary">
              <span class="font-mono line-through">{{ database.schema }}</span>
              <connection-scheme-badge
                v-if="schemeOfInput(database)"
                :scheme="schemeOfInput(database)!"
              />
              <span
                v-if="database.origin !== 'memory'"
                class="min-w-0 truncate font-mono text-sm line-through"
              >
                {{ database.location }}
              </span>
            </div>
            <p class="text-sm text-fg-secondary">{{ removedText(database) }}</p>
          </div>
          <ui-button
            :icon="Undo2"
            label="Rétablir"
            small
            :disabled="kept >= ATTACHED_MAX"
            @click="restore(index)"
          />
        </div>
        <div v-else class="flex items-center gap-3 px-4 py-3">
          <ui-input
            v-model="database.schema"
            :maxlength="LENGTHS.schema"
            placeholder="schéma"
            spellcheck="false"
            :aria-label="`Schéma de la base attachée ${index + 1}`"
            class="w-36"
          />
          <ui-select
            v-model="database.origin"
            :items="ATTACHED_ORIGINS"
            :label="`Origine de la base attachée ${index + 1}`"
            class="w-48"
          />
          <ui-input
            v-if="database.origin !== 'memory'"
            v-model="database.location"
            :maxlength="LENGTHS.location"
            :placeholder="database.origin === 'new' ? 'fichier à créer' : 'chemin ou adresse https'"
            spellcheck="false"
            :aria-label="`Emplacement de la base attachée ${index + 1}`"
            class="min-w-0 flex-1"
          >
            <template v-if="schemeOf(database.location)" #trailing>
              <connection-scheme-badge :scheme="schemeOf(database.location)!" />
            </template>
          </ui-input>
          <span v-else class="flex min-w-0 flex-1 items-center gap-2 text-sm text-fg-secondary">
            <connection-scheme-badge scheme="memory" />
            vide à chaque session
          </span>
          <ui-button
            :icon="Trash2"
            variant="ghost"
            :aria-label="`Retirer la base attachée ${database.schema || index + 1}`"
            @click="remove(index)"
          />
        </div>
      </template>
    </ui-setting-list>
    <div class="flex items-center gap-3">
      <ui-button
        :icon="Plus"
        label="Attacher une base"
        :disabled="kept >= ATTACHED_MAX"
        @click="attach"
      />
      <span class="text-sm text-fg-secondary">
        Chacune s’attache à chaque session ; {{ ATTACHED_MAX }} au plus.
      </span>
    </div>
  </div>

  <ui-setting-list title="Session">
    <ui-setting-row
      label="À la connexion"
      description="Pragmas exécutés à l’ouverture de chaque connexion, séparés par des « ; »."
      :for="`${id}-init`"
      stacked
    >
      <ui-textarea
        :id="`${id}-init`"
        v-model="form.init_sql"
        :maxlength="LENGTHS.initSql"
        rows="3"
        placeholder="PRAGMA foreign_keys = ON;"
        class="w-full"
      />
    </ui-setting-row>
    <ui-setting-row
      label="Session gardée"
      description="La session (connexion, transactions, requêtes en cours, bases en mémoire) reste ouverte ce temps après la fermeture de la page, qui s’y raccroche à son retour ; 0 la ferme aussitôt."
      :for="`${id}-timeout`"
    >
      <ui-input
        :id="`${id}-timeout`"
        v-model="form.session_timeout_minutes"
        type="number"
        :min="0"
        :max="SESSION_TIMEOUT_MAX"
        required
        class="w-27"
      />
      <span class="text-fg-secondary">min</span>
    </ui-setting-row>
  </ui-setting-list>

  <ui-setting-list title="Navigateur">
    <ui-setting-row
      label="Afficher le schéma système"
      description="temp : les tables temporaires de la session."
      :for="`${id}-system`"
    >
      <ui-switch :id="`${id}-system`" v-model="form.show_system_schemas" />
    </ui-setting-row>
  </ui-setting-list>
</template>
