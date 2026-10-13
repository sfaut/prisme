/**
 * Largeurs de colonnes choisies dans la grille de résultats, retenues par
 * requête pour la durée de la page.
 *
 * Chaque exécution recrée la grille de résultats : sans mémoire, relancer
 * une requête ferait perdre les largeurs choisies. Elles sont donc gardées
 * ici, par texte de requête, et ne s'appliquent que si le résultat a les
 * mêmes colonnes, dans le même ordre (une table modifiée entre-temps repart
 * des largeurs estimées).
 */

/**
 * Nombre de requêtes dont les largeurs sont retenues ; au-delà, les plus
 * anciennes sont oubliées.
 */
const MAX_REMEMBERED = 100

/**
 * Largeurs retenues pour une requête.
 */
interface RememberedWidths {
  /**
   * Noms des colonnes du résultat, dans l'ordre du `SELECT`.
   */
  columns: string[]
  /**
   * Largeur choisie, en pixels, par indice de colonne dans le `SELECT`.
   */
  widths: Record<number, number>
}

/**
 * Largeurs retenues, par texte de requête, de la plus ancienne à la plus récente.
 */
const remembered = new Map<string, RememberedWidths>()

/**
 * Largeurs choisies pour une requête, vides si aucune n'est retenue ou si les
 * colonnes ont changé.
 *
 * @param sql Texte de la requête.
 * @param columns Noms des colonnes du résultat.
 */
export function rememberedWidths(sql: string, columns: string[]): Record<number, number> {
  const entry = remembered.get(sql)
  if (entry === undefined || entry.columns.join('\u0000') !== columns.join('\u0000')) {
    return {}
  }
  return { ...entry.widths }
}

/**
 * Retient les largeurs choisies pour une requête.
 *
 * @param sql Texte de la requête.
 * @param columns Noms des colonnes du résultat.
 * @param widths Largeur choisie, par indice de colonne.
 */
export function rememberWidths(
  sql: string,
  columns: string[],
  widths: Record<number, number>,
): void {
  // Supprimer puis remettre : la requête devient la plus récente.
  remembered.delete(sql)
  remembered.set(sql, { columns: [...columns], widths: { ...widths } })
  if (remembered.size > MAX_REMEMBERED) {
    const oldest = remembered.keys().next().value
    if (oldest !== undefined) {
      remembered.delete(oldest)
    }
  }
}
