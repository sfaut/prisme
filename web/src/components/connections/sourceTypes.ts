/**
 * Kinds of data sources that a connection can reach, as the connections
 * manager offers them, grouped in families (database servers, files).
 *
 * The list states where Prisme is going: only MySQL / MariaDB is supported
 * today (`ready`). The other kinds show greyed out, marked "à venir", in the
 * choice of the type of a new connection; the family of the files has no
 * connection yet.
 */

import {
  Braces,
  ChartNoAxesColumn,
  FileSpreadsheet,
  HardDrive,
  Layers,
  Server,
  Sheet,
} from '@lucide/vue'
import type { Component } from 'vue'

/**
 * Key of a kind of source.
 */
export type SourceTypeKey =
  'mysql' | 'postgresql' | 'clickhouse' | 'sqlite' | 'duckdb' | 'csv' | 'excel' | 'json' | 'parquet'

/**
 * A kind of source.
 */
export interface SourceType {
  /**
   * Key of the kind.
   */
  key: SourceTypeKey
  /**
   * Name, as shown.
   */
  label: string
  /**
   * What it reaches, in a sentence.
   */
  description: string
  /**
   * Lucide icon of the kind.
   */
  icon: Component
  /**
   * Can a connection of this kind be created today?
   */
  ready: boolean
}

/**
 * A family of kinds of sources.
 */
export interface SourceFamily {
  /**
   * Key of the family.
   */
  key: 'server' | 'file'
  /**
   * Name of the family in the list of the connections.
   */
  title: string
  /**
   * Name of the family in the choice of the type.
   */
  heading: string
  /**
   * Kinds of the family, in the order shown.
   */
  types: SourceType[]
  /**
   * Can a connection of the family be created today?
   */
  ready: boolean
}

/**
 * Kind of the MySQL and MariaDB connections, the only one supported: the
 * flavor is detected on connection.
 */
export const MYSQL: SourceType = {
  key: 'mysql',
  label: 'MySQL / MariaDB',
  description: 'Serveur MySQL ou MariaDB ; la variante se détecte à la connexion.',
  icon: Server,
  ready: true,
}

/**
 * Families of the kinds of sources, in the order shown.
 */
export const SOURCE_FAMILIES: SourceFamily[] = [
  {
    key: 'server',
    title: 'Serveurs',
    heading: 'Serveurs de bases de données',
    ready: true,
    types: [
      MYSQL,
      {
        key: 'postgresql',
        label: 'PostgreSQL',
        description: 'Serveur PostgreSQL.',
        icon: Server,
        ready: false,
      },
      {
        key: 'clickhouse',
        label: 'ClickHouse',
        description: 'Serveur ClickHouse, base en colonnes.',
        icon: ChartNoAxesColumn,
        ready: false,
      },
    ],
  },
  {
    key: 'file',
    title: 'Fichiers',
    heading: 'Fichiers',
    ready: false,
    types: [
      {
        key: 'sqlite',
        label: 'SQLite',
        description: 'Base dans un fichier .sqlite ou .db.',
        icon: HardDrive,
        ready: false,
      },
      {
        key: 'duckdb',
        label: 'DuckDB',
        description: 'Base dans un fichier .duckdb.',
        icon: HardDrive,
        ready: false,
      },
      {
        key: 'csv',
        label: 'CSV',
        description: 'Texte délimité, lu entièrement comme texte.',
        icon: FileSpreadsheet,
        ready: false,
      },
      {
        key: 'excel',
        label: 'Excel',
        description: 'Classeur .xlsx ; une table par feuille.',
        icon: Sheet,
        ready: false,
      },
      {
        key: 'json',
        label: 'JSON',
        description: 'Tableau d’objets, ou JSON Lines.',
        icon: Braces,
        ready: false,
      },
      {
        key: 'parquet',
        label: 'Parquet',
        description: 'Fichier en colonnes.',
        icon: Layers,
        ready: false,
      },
    ],
  },
]
