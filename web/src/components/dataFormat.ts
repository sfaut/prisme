/**
 * Readable form of the JSON and XML values shown in the data explorer
 * (`DataExplorer.vue`): indented with four spaces, one value alone or several
 * gathered in one document (`[…]` for JSON, `<data>…</data>` for XML).
 *
 * The text is reindented token by token, never parsed then written again: a
 * number keeps its writing (`1.50`, `12345678901234567890`, beyond the
 * precision of JavaScript's numbers), a key its duplicates, and what is
 * copied from the explorer is the value of the database, only reindented.
 * Each value is still checked beforehand (`JSON.parse`, `DOMParser`): the
 * reindentation can rely on a well-formed text.
 *
 * NULLs follow the SQL aggregates: `null` in a JSON array (like
 * `JSON_ARRAYAGG`), left out of an XML document (like `XMLAGG`).
 */

/**
 * Indentation of one level.
 */
export const DATA_INDENT = '    '

/**
 * Selected cell: number of its row in the grid (from 1), and its value.
 */
export interface DataCell {
  row: number
  value: string | null
}

/**
 * Document to show: its text, or the message that replaces it.
 */
export type DataDocument = { text: string } | { message: string }

/**
 * Builds the document of the selected cells of a JSON or XML column: the
 * value alone if a single cell is selected, gathered into one document
 * otherwise.
 *
 * @param kind Format of the column.
 * @param cells Selected cells, from top to bottom.
 */
export function dataDocument(kind: 'json' | 'xml', cells: DataCell[]): DataDocument {
  return kind === 'json' ? jsonDocument(cells) : xmlDocument(cells)
}

/**
 * JSON document of the cells: the value alone, or the array of the values.
 *
 * @param cells Selected cells.
 */
function jsonDocument(cells: DataCell[]): DataDocument {
  for (const cell of cells) {
    if (cell.value !== null && !isJson(cell.value)) {
      return { message: `La valeur de la ligne ${cell.row} n’est pas du JSON valide.` }
    }
  }
  const values = cells.map((cell) => cell.value ?? 'null')
  return { text: reindentJson(values.length === 1 ? values[0]! : `[${values.join(',')}]`) }
}

/**
 * Is the text valid JSON?
 *
 * @param value Text.
 */
function isJson(value: string): boolean {
  try {
    JSON.parse(value)
    return true
  } catch {
    return false
  }
}

/**
 * Reindents a valid JSON text: one member or element per line, four spaces
 * per level, a space after `:`. Strings, numbers and literals keep their
 * writing; empty objects and arrays stay on one line.
 *
 * @param text Valid JSON text.
 */
export function reindentJson(text: string): string {
  const out: string[] = []
  let depth = 0
  const newline = () => `\n${DATA_INDENT.repeat(depth)}`
  let index = 0
  while (index < text.length) {
    const char = text[index]!
    if (char === '"') {
      let end = index + 1
      while (text[end] !== '"') {
        end += text[end] === '\\' ? 2 : 1
      }
      out.push(text.slice(index, end + 1))
      index = end + 1
    } else if (char === '{' || char === '[') {
      const next = skipBlanks(text, index + 1)
      if (text[next] === (char === '{' ? '}' : ']')) {
        out.push(char, text[next]!)
        index = next + 1
      } else {
        depth++
        out.push(char, newline())
        index++
      }
    } else if (char === '}' || char === ']') {
      depth--
      out.push(newline(), char)
      index++
    } else if (char === ',') {
      out.push(',', newline())
      index++
    } else if (char === ':') {
      out.push(': ')
      index++
    } else if (JSON_BLANKS.includes(char)) {
      index++
    } else {
      let end = index
      while (end < text.length && !JSON_DELIMITERS.includes(text[end]!)) {
        end++
      }
      out.push(text.slice(index, end))
      index = end
    }
  }
  return out.join('')
}

/**
 * Blanks allowed between JSON tokens.
 */
const JSON_BLANKS = ' \t\n\r'

/**
 * Characters that end a number or a literal (`true`, `false`, `null`).
 */
const JSON_DELIMITERS = `${JSON_BLANKS},:{}[]"`

/**
 * Position of the first non-blank character from `index`.
 *
 * @param text Text.
 * @param index Starting position.
 */
function skipBlanks(text: string, index: number): number {
  while (index < text.length && JSON_BLANKS.includes(text[index]!)) {
    index++
  }
  return index
}

/**
 * XML document of the cells: the document alone, or the documents gathered
 * in `<data>`, without their `<?xml?>` declaration.
 *
 * @param cells Selected cells.
 */
function xmlDocument(cells: DataCell[]): DataDocument {
  const documents: XmlNode[][] = []
  for (const cell of cells) {
    if (cell.value === null) {
      continue
    }
    if (!isXml(cell.value)) {
      return { message: `La valeur de la ligne ${cell.row} n’est pas du XML bien formé.` }
    }
    const nodes = parseXml(cell.value)
    if (cells.length > 1 && nodes.some((node) => node.kind === 'doctype')) {
      return {
        message: `La valeur de la ligne ${cell.row} déclare un DOCTYPE : elle ne peut pas être regroupée dans <data>.`,
      }
    }
    documents.push(nodes)
  }
  if (cells.length === 1) {
    const [nodes] = documents
    return nodes === undefined
      ? { message: 'La valeur sélectionnée est NULL.' }
      : { text: printXml(cells[0]!.value!, nodes, 0).join('\n') }
  }
  const lines = cells
    .filter((cell) => cell.value !== null)
    .flatMap((cell, index) =>
      printXml(
        cell.value!,
        documents[index]!.filter((node) => node.kind !== 'declaration'),
        1,
      ),
    )
  return { text: ['<data>', ...lines, '</data>'].join('\n') }
}

/**
 * Is the text a well-formed XML document?
 *
 * @param value Text.
 */
function isXml(value: string): boolean {
  const document = new DOMParser().parseFromString(value, 'application/xml')
  return document.getElementsByTagName('parsererror').length === 0
}

/**
 * Node of an XML text, with its place in the text.
 *
 * - `element`: from its start tag to its end tag; `openEnd` ends the start
 *   tag, `closeStart` starts the end tag (both equal to `to` for an empty
 *   element `<a/>`);
 * - `text`: characters between two tags; `cdata`: a `<![CDATA[…]]>` section;
 * - `declaration`: `<?xml…?>`; `doctype`: `<!DOCTYPE…>`; `other`: comment or
 *   processing instruction.
 */
export interface XmlNode {
  kind: 'element' | 'text' | 'cdata' | 'declaration' | 'doctype' | 'other'
  from: number
  to: number
  openEnd: number
  closeStart: number
  children: XmlNode[]
}

/**
 * Splits a well-formed XML text into nodes, without decoding anything.
 *
 * @param text Well-formed XML text.
 * @returns The nodes of the top level.
 */
export function parseXml(text: string): XmlNode[] {
  const top: XmlNode[] = []
  const open: XmlNode[] = []
  const add = (kind: XmlNode['kind'], from: number, to: number): XmlNode => {
    const node = { kind, from, to, openEnd: to, closeStart: to, children: [] }
    ;(open[open.length - 1]?.children ?? top).push(node)
    return node
  }
  let index = 0
  while (index < text.length) {
    if (text.startsWith('<!--', index)) {
      index = add('other', index, text.indexOf('-->', index + 4) + 3).to
    } else if (text.startsWith('<![CDATA[', index)) {
      index = add('cdata', index, text.indexOf(']]>', index + 9) + 3).to
    } else if (text.startsWith('<?', index)) {
      const end = text.indexOf('?>', index + 2) + 2
      index = add(
        /^<\?xml[\s?]/.test(text.slice(index, index + 6)) ? 'declaration' : 'other',
        index,
        end,
      ).to
    } else if (text.startsWith('<!', index)) {
      index = add('doctype', index, doctypeEnd(text, index)).to
    } else if (text.startsWith('</', index)) {
      const element = open.pop()!
      element.closeStart = index
      element.to = text.indexOf('>', index) + 1
      index = element.to
    } else if (text[index] === '<') {
      const end = tagEnd(text, index)
      const element = add('element', index, end)
      if (text[end - 2] !== '/') {
        open.push(element)
      }
      index = end
    } else {
      const end = text.indexOf('<', index)
      index = add('text', index, end === -1 ? text.length : end).to
    }
  }
  return top
}

/**
 * End of a start tag (after its `>`), whose attributes may hold `>` between
 * quotes.
 *
 * @param text XML text.
 * @param index Position of the `<`.
 */
function tagEnd(text: string, index: number): number {
  let quote: string | null = null
  for (let position = index + 1; position < text.length; position++) {
    const char = text[position]
    if (quote !== null) {
      quote = char === quote ? null : quote
    } else if (char === '"' || char === "'") {
      quote = char
    } else if (char === '>') {
      return position + 1
    }
  }
  return text.length
}

/**
 * End of a `<!DOCTYPE…>` declaration, whose internal subset (`[…]`) may hold
 * `>`.
 *
 * @param text XML text.
 * @param index Position of the `<`.
 */
function doctypeEnd(text: string, index: number): number {
  let quote: string | null = null
  let subset = false
  for (let position = index + 2; position < text.length; position++) {
    const char = text[position]
    if (quote !== null) {
      quote = char === quote ? null : quote
    } else if (char === '"' || char === "'") {
      quote = char
    } else if (char === '[' || char === ']') {
      subset = char === '['
    } else if (char === '>' && !subset) {
      return position + 1
    }
  }
  return text.length
}

/**
 * Is the node a blank text (only spaces, tabs and line breaks)?
 *
 * @param text XML text of the node.
 * @param node Node.
 */
export function isBlankXml(text: string, node: XmlNode): boolean {
  return node.kind === 'text' && text.slice(node.from, node.to).trim() === ''
}

/**
 * Is the element structured, written with one child per line? Its content is
 * only elements (and comments), without `xml:space="preserve"`. The blanks
 * between its children are then indentation, given by the explorer.
 *
 * @param text XML text of the node.
 * @param node Node.
 */
export function isStructuredXml(text: string, node: XmlNode): boolean {
  const children = node.children.filter((child) => !isBlankXml(text, child))
  return (
    node.kind === 'element' &&
    children.length > 0 &&
    children.every((child) => child.kind !== 'text' && child.kind !== 'cdata') &&
    !/\sxml:space\s*=\s*["']preserve["']/.test(text.slice(node.from, node.openEnd))
  )
}

/**
 * Lines of a well-formed XML text, or of a fragment (an element, a
 * comment...), indented from the level `depth`, without its `<?xml?>`
 * declaration nor its DOCTYPE: the text is meant to go in an element.
 *
 * @param text XML text.
 * @param depth Level of the text.
 */
export function xmlLines(text: string, depth: number): string[] {
  const nodes = parseXml(text).filter(
    (node) => node.kind !== 'declaration' && node.kind !== 'doctype',
  )
  return printXml(text, nodes, depth)
}

/**
 * Lines of the nodes, indented from the level `depth`. An element whose
 * content is only elements (and comments) puts each child on its own line;
 * one that holds text (mixed content), a CDATA section or
 * `xml:space="preserve"` stays as written, like the text and the comments.
 * The blanks between elements give way to the indentation.
 *
 * @param text XML text of the nodes.
 * @param nodes Nodes to write.
 * @param depth Level of the nodes.
 */
function printXml(text: string, nodes: XmlNode[], depth: number): string[] {
  const indent = DATA_INDENT.repeat(depth)
  const lines: string[] = []
  for (const node of nodes) {
    if (isBlankXml(text, node)) {
      continue
    }
    if (isStructuredXml(text, node)) {
      lines.push(
        `${indent}${text.slice(node.from, node.openEnd)}`,
        ...printXml(
          text,
          node.children.filter((child) => !isBlankXml(text, child)),
          depth + 1,
        ),
        `${indent}${text.slice(node.closeStart, node.to)}`,
      )
    } else {
      lines.push(`${indent}${text.slice(node.from, node.to)}`)
    }
  }
  return lines
}
