/**
 * Kinds of write that the settings allow or not (`WriteKind`, classified by
 * the server in `crates/dialects/src/classify.rs`): their statement, for the
 * messages and the settings, and their groups in the settings.
 */

import type { WriteKind } from './generated/WriteKind'

/**
 * Statement of each kind, as the messages and the settings show it.
 */
export const WRITE_KIND_STATEMENTS: Record<WriteKind, string> = {
  insert: 'INSERT',
  update: 'UPDATE',
  delete: 'DELETE',
  create_table: 'CREATE TABLE',
  alter_table: 'ALTER TABLE',
  rename_table: 'RENAME TABLE',
  truncate_table: 'TRUNCATE TABLE',
  drop_table: 'DROP TABLE',
  create_temporary_table: 'CREATE TEMPORARY TABLE',
  alter_temporary_table: 'ALTER TABLE sur une table temporaire',
  rename_temporary_table: 'RENAME TABLE sur des tables temporaires',
  truncate_temporary_table: 'TRUNCATE TABLE sur une table temporaire',
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
}

/**
 * Other statements that a kind allows, alone or with another kind.
 */
export const WRITE_KIND_NOTES: Partial<Record<WriteKind, string>> = {
  insert:
    'Tables temporaires comprises. Avec DELETE : REPLACE. Avec UPDATE : INSERT … ON DUPLICATE KEY UPDATE.',
  update: 'Sur une table ou plusieurs (jointure), tables temporaires comprises.',
  delete:
    'Sur une table ou plusieurs (jointure), tables temporaires comprises. Avec INSERT : REPLACE.',
  alter_table: 'Aussi CREATE INDEX et DROP INDEX.',
  create_temporary_table:
    'Prisme suit les tables temporaires créées par la session : les changements qui ne portent que sur elles relèvent de ce groupe, leurs données du groupe Données.',
  alter_temporary_table: 'Aussi CREATE INDEX et DROP INDEX.',
  drop_temporary_table: 'Aussi DROP TABLE, quand toutes les tables nommées sont temporaires.',
  create_view: 'Aussi ALTER VIEW.',
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
    ],
  },
  {
    label: 'Tables',
    kinds: [
      { kind: 'create_table', verb: 'CREATE' },
      { kind: 'alter_table', verb: 'ALTER' },
      { kind: 'rename_table', verb: 'RENAME' },
      { kind: 'truncate_table', verb: 'TRUNCATE' },
      { kind: 'drop_table', verb: 'DROP' },
    ],
  },
  {
    label: 'Tables temporaires',
    kinds: [
      { kind: 'create_temporary_table', verb: 'CREATE' },
      { kind: 'alter_temporary_table', verb: 'ALTER' },
      { kind: 'rename_temporary_table', verb: 'RENAME' },
      { kind: 'truncate_temporary_table', verb: 'TRUNCATE' },
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
]

/**
 * Tooltip of a kind in the settings: its statement, then what else it allows.
 *
 * @param kind Kind of write.
 * @returns The statement, followed by its note if it has one.
 */
export function writeKindTooltip(kind: WriteKind): string {
  const note = WRITE_KIND_NOTES[kind]
  return note === undefined
    ? WRITE_KIND_STATEMENTS[kind]
    : `${WRITE_KIND_STATEMENTS[kind]}. ${note}`
}
