/**
 * Kinds of write that the settings allow or not (`WriteKind`, classified by
 * the server in `crates/dialects/src/classify.rs`): their statement, for the
 * messages and the settings, and their groups in the settings.
 */

import { FLAVORS } from './flavors'
import type { Flavor } from './generated/Flavor'
import type { WriteKind } from './generated/WriteKind'

/**
 * Statement of each kind, as the messages and the settings show it.
 */
export const WRITE_KIND_STATEMENTS: Record<WriteKind, string> = {
  insert: 'INSERT',
  update: 'UPDATE',
  delete: 'DELETE',
  truncate_table: 'TRUNCATE TABLE',
  create_table: 'CREATE TABLE',
  alter_table: 'ALTER TABLE',
  rename_table: 'RENAME TABLE',
  drop_table: 'DROP TABLE',
  create_temporary_table: 'CREATE TEMPORARY TABLE',
  alter_temporary_table: 'ALTER TABLE sur une table temporaire',
  rename_temporary_table: 'RENAME TABLE sur des tables temporaires',
  drop_temporary_table: 'DROP TEMPORARY TABLE',
  create_view: 'CREATE [OR REPLACE] VIEW',
  drop_view: 'DROP VIEW',
  create_function: 'CREATE FUNCTION',
  drop_function: 'DROP FUNCTION',
  create_procedure: 'CREATE PROCEDURE',
  drop_procedure: 'DROP PROCEDURE',
  create_event: 'CREATE EVENT',
  alter_event: 'ALTER EVENT',
  drop_event: 'DROP EVENT',
  attach: 'ATTACH / DETACH',
  maintenance: 'VACUUM, REINDEX, ANALYZE',
}

/**
 * Kinds that a read-only session still allows, under their switch
 * (`WriteKind::allowed_read_only`): `ATTACH` / `DETACH`, as a file attached
 * by a read-only connection is read-only too.
 */
export const READ_ONLY_KINDS: readonly WriteKind[] = ['attach']

/**
 * Other statements that a kind allows, alone or with another kind.
 */
export const WRITE_KIND_NOTES: Partial<Record<WriteKind, string>> = {
  insert:
    'Tables temporaires comprises. Avec DELETE : REPLACE, INSERT OR REPLACE. Avec UPDATE : INSERT … ON DUPLICATE KEY UPDATE, INSERT … ON CONFLICT DO UPDATE.',
  update:
    'Sur une table ou plusieurs (jointure), tables temporaires comprises. Avec DELETE : UPDATE OR REPLACE.',
  delete:
    'Sur une table ou plusieurs (jointure), tables temporaires comprises. Avec INSERT : REPLACE, INSERT OR REPLACE. Avec UPDATE : UPDATE OR REPLACE.',
  truncate_table: 'Tables temporaires comprises',
  create_table: 'Aussi CREATE VIRTUAL TABLE (SQLite)',
  alter_table: 'Aussi CREATE INDEX et DROP INDEX',
  create_temporary_table:
    'Prisme suit les tables temporaires créées par la session, et celles du schéma temp de SQLite : les changements qui ne portent que sur elles relèvent de ce groupe, leurs données du groupe Données',
  alter_temporary_table: 'Aussi CREATE INDEX et DROP INDEX',
  drop_temporary_table: 'Aussi DROP TABLE, quand toutes les tables nommées sont temporaires',
  create_view: 'Aussi ALTER VIEW',
  attach:
    'Pour la session, même en lecture seule. Un fichier nommé par un chemin absolu, ou :memory: ; une base attachée en lecture seule le reste.',
  maintenance:
    'Aussi les pragmas qui écrivent dans le fichier : optimize, wal_checkpoint, incremental_vacuum. VACUUM INTO reste refusé.',
}

/**
 * Group of kinds in the settings: the object, then the verb of each kind.
 */
export interface WriteKindGroup {
  /**
   * Object written: data, tables, views...
   */
  label: string
  /**
   * Kinds of the group, with their verb (`CREATE`, `DROP`...).
   */
  kinds: { kind: WriteKind; verb: string }[]
}

/**
 * Groups of the settings, in the order of the server (`WriteKind::ALL`).
 */
export const WRITE_KIND_GROUPS: WriteKindGroup[] = [
  {
    label: 'Données',
    kinds: [
      { kind: 'insert', verb: 'INSERT' },
      { kind: 'update', verb: 'UPDATE' },
      { kind: 'delete', verb: 'DELETE' },
      { kind: 'truncate_table', verb: 'TRUNCATE' },
    ],
  },
  {
    label: 'Tables',
    kinds: [
      { kind: 'create_table', verb: 'CREATE' },
      { kind: 'alter_table', verb: 'ALTER' },
      { kind: 'rename_table', verb: 'RENAME' },
      { kind: 'drop_table', verb: 'DROP' },
    ],
  },
  {
    label: 'Tables temporaires',
    kinds: [
      { kind: 'create_temporary_table', verb: 'CREATE' },
      { kind: 'alter_temporary_table', verb: 'ALTER' },
      { kind: 'rename_temporary_table', verb: 'RENAME' },
      { kind: 'drop_temporary_table', verb: 'DROP' },
    ],
  },
  {
    label: 'Vues',
    kinds: [
      { kind: 'create_view', verb: 'CREATE' },
      { kind: 'drop_view', verb: 'DROP' },
    ],
  },
  {
    label: 'Fonctions',
    kinds: [
      { kind: 'create_function', verb: 'CREATE' },
      { kind: 'drop_function', verb: 'DROP' },
    ],
  },
  {
    label: 'Procédures',
    kinds: [
      { kind: 'create_procedure', verb: 'CREATE' },
      { kind: 'drop_procedure', verb: 'DROP' },
    ],
  },
  {
    label: 'Événements',
    kinds: [
      { kind: 'create_event', verb: 'CREATE' },
      { kind: 'alter_event', verb: 'ALTER' },
      { kind: 'drop_event', verb: 'DROP' },
    ],
  },
  {
    label: 'Fichiers SQLite',
    kinds: [
      { kind: 'attach', verb: 'ATTACH' },
      { kind: 'maintenance', verb: 'VACUUM' },
    ],
  },
]

/**
 * Names of flavors as a French list: `MySQL`, `MySQL et MariaDB`,
 * `MySQL, MariaDB et SQLite`.
 *
 * @param flavors Flavors, at least one.
 */
function flavorList(flavors: Flavor[]): string {
  const names = flavors.map((flavor) => FLAVORS[flavor].name)
  const last = names.pop()
  return names.length === 0 ? (last ?? '') : `${names.join(', ')} et ${last}`
}

/**
 * Tooltip of a kind in the settings: its statement, then what else it allows
 * and the engines it concerns.
 *
 * @param kind Kind of write.
 * @param flavors
 *     Flavors whose engine knows the kind (`flavorsOfWriteKind`); `null`
 *     while unknown.
 * @returns
 *     The statement as title; its note, then the engines it concerns, each
 *     on its line, as description.
 */
export function writeKindTooltip(
  kind: WriteKind,
  flavors: Flavor[] | null = null,
): { title: string; description?: string } {
  const engines = flavors === null || flavors.length === 0 ? null : `Pour ${flavorList(flavors)}`
  const lines = [WRITE_KIND_NOTES[kind], engines].filter((line) => line != null)
  return {
    title: WRITE_KIND_STATEMENTS[kind],
    description: lines.length === 0 ? undefined : lines.join('\n'),
  }
}
