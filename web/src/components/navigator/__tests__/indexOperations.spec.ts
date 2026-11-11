import { describe, expect, it } from 'vitest'

import { indexOperations, versionAtLeast } from '../indexOperations'

/**
 * Index `idx` of the table `shop`.`orders`.
 */
const INDEX = { schema: 'shop', table: 'orders', name: 'idx' }

/**
 * Server of the given flavor and version, without `ANSI_QUOTES`.
 *
 * @param flavor Flavor of the server.
 * @param version Version of the server.
 */
function server(flavor: 'mysql' | 'mariadb', version: string) {
  return { flavor, version, ansiQuotes: false }
}

describe('versions', () => {
  it('compares the version given by the server', () => {
    expect(versionAtLeast('8.0.11', [8, 0, 0])).toBe(true)
    expect(versionAtLeast('5.7.44-log', [8, 0, 0])).toBe(false)
    expect(versionAtLeast('10.6.3-MariaDB-1:10.6.3+maria~focal', [10, 6, 0])).toBe(true)
    expect(versionAtLeast('10.5.29-MariaDB-ubu2004', [10, 6, 0])).toBe(false)
    expect(versionAtLeast('10.5.2-MariaDB', [10, 5, 2])).toBe(true)
    expect(versionAtLeast('11.0.1', [10, 6, 0])).toBe(true)
    expect(versionAtLeast('inconnue', [1, 0, 0])).toBe(false)
  })
})

describe('operations on an index', () => {
  it('writes the visibility of MySQL', () => {
    expect(indexOperations(INDEX, server('mysql', '8.4.11'), ';')).toBe(
      [
        '-- ALTER TABLE `shop`.`orders` ADD PRIMARY KEY (column_name /* , ... */);',
        '-- ALTER TABLE `shop`.`orders` ADD UNIQUE KEY (column_name /* , ... */);',
        '-- ALTER TABLE `shop`.`orders` ADD KEY (column_name /* , ... */);',
        '-- ALTER TABLE `shop`.`orders` ALTER INDEX `idx` VISIBLE;',
        '-- ALTER TABLE `shop`.`orders` ALTER INDEX `idx` INVISIBLE;',
        '-- ALTER TABLE `shop`.`orders` DROP KEY `idx`;',
        '-- ALTER TABLE `shop`.`orders` RENAME KEY `idx` TO new_index_name;',
      ].join('\n'),
    )
  })

  it('writes the ignored indexes of MariaDB from 10.6 only', () => {
    const recent = indexOperations(INDEX, server('mariadb', '10.6.28-MariaDB'), ';')
    expect(recent).toContain('-- ALTER TABLE `shop`.`orders` ALTER INDEX `idx` NOT IGNORED;')
    expect(recent).toContain('-- ALTER TABLE `shop`.`orders` ALTER INDEX `idx` IGNORED;')
    expect(recent).not.toContain('VISIBLE')
    const older = indexOperations(INDEX, server('mariadb', '10.5.29-MariaDB'), ';')
    expect(older).not.toContain('ALTER INDEX')
    expect(older).toContain('RENAME KEY')
    expect(indexOperations(INDEX, server('mariadb', '10.4.34-MariaDB'), ';')).not.toContain(
      'RENAME KEY',
    )
  })

  it('removes the primary key, without visibility or renaming', () => {
    const primary = indexOperations({ ...INDEX, name: 'PRIMARY' }, server('mysql', '9.3.0'), ';')
    expect(primary.split('\n').slice(3)).toEqual([
      '-- ALTER TABLE `shop`.`orders` DROP PRIMARY KEY;',
    ])
  })

  it('ends each statement with the separator in force, names quoted as the session wants', () => {
    const block = indexOperations(
      { schema: 'a"b', table: 't', name: 'i' },
      { flavor: 'mysql', version: '8.0.36', ansiQuotes: true },
      '//',
    )
    expect(block.split('\n')[5]).toBe('-- ALTER TABLE "a""b"."t" DROP KEY "i"//')
  })
})
