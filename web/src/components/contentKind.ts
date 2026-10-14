/**
 * Détection du contenu des colonnes de texte : JSON ou XML.
 *
 * Une colonne `TEXT` (ou `VARCHAR`) peut contenir des documents JSON ou
 * XML que le serveur ne type pas (le JSON de MariaDB arrive aussi en
 * `LONGTEXT`). L'en-tête de la grille montre alors l'icône du contenu à la
 * place de celle du texte. La détection lit un échantillon des premières
 * valeurs non vides : toutes doivent être des documents du même format,
 * sinon la colonne reste du texte.
 */

/**
 * Format reconnu dans une colonne de texte.
 */
export type ContentKind = 'json' | 'xml'

/**
 * Nombre de valeurs non vides lues pour décider.
 */
const SAMPLE_SIZE = 50

/**
 * Nombre de lignes parcourues au plus pour réunir l'échantillon.
 */
const SCAN_LIMIT = 500

/**
 * Format des valeurs de la colonne `column`, `null` si ce n'est ni du JSON
 * ni du XML (ou si aucune valeur n'est encore là).
 *
 * @param rows Lignes du résultat.
 * @param column Indice de la colonne.
 */
export function detectContent(rows: (string | null)[][], column: number): ContentKind | null {
  const sample: string[] = []
  for (let index = 0; index < rows.length && index < SCAN_LIMIT; index++) {
    const value = rows[index]?.[column]?.trim()
    if (value) {
      sample.push(value)
      if (sample.length === SAMPLE_SIZE) {
        break
      }
    }
  }
  if (sample.length === 0) {
    return null
  }
  if (sample.every(isJsonDocument)) {
    return 'json'
  }
  if (sample.every(isXmlDocument)) {
    return 'xml'
  }
  return null
}

/**
 * Le texte est-il un objet ou un tableau JSON ? Un nombre ou une chaîne
 * seuls, du JSON valide, ne font pas d'une colonne une colonne JSON.
 *
 * @param value Texte, sans espaces autour.
 */
function isJsonDocument(value: string): boolean {
  const first = value[0]
  if (first !== '{' && first !== '[') {
    return false
  }
  try {
    JSON.parse(value)
    return true
  } catch {
    return false
  }
}

/**
 * Le texte est-il un document XML bien formé ?
 *
 * @param value Texte, sans espaces autour.
 */
function isXmlDocument(value: string): boolean {
  if (!value.startsWith('<') || !value.endsWith('>')) {
    return false
  }
  const document = new DOMParser().parseFromString(value, 'application/xml')
  return document.getElementsByTagName('parsererror').length === 0
}
