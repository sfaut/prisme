/**
 * Capabilities of MySQL and MariaDB, as the server sends them with the
 * identity of a source (`crates/connectors/src/mysql/driver.rs`), for the
 * messages of the tests.
 */

import type { Capabilities } from '../generated/Capabilities'

/**
 * What MySQL and MariaDB can do.
 */
export const MYSQL_CAPABILITIES: Capabilities = {
  transactions: true,
  cancellation: true,
  parallel_sessions: true,
  switch_namespace: true,
  attachments: false,
  namespace_levels: ['database'],
  object_kinds: ['table', 'view', 'system_table', 'procedure', 'function', 'event'],
  write_kinds: [
    'insert',
    'update',
    'delete',
    'truncate_table',
    'create_table',
    'alter_table',
    'rename_table',
    'drop_table',
    'create_temporary_table',
    'alter_temporary_table',
    'rename_temporary_table',
    'drop_temporary_table',
    'create_view',
    'drop_view',
    'create_function',
    'drop_function',
    'create_procedure',
    'drop_procedure',
    'create_event',
    'alter_event',
    'drop_event',
  ],
  notices: true,
  rows_estimate: true,
}
