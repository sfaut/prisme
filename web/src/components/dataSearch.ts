/**
 * Search of the data explorer (`DataExplorer.vue`): a JSONPath expression
 * (RFC 9535) on a JSON document, an XPath 1.0 expression on an XML document.
 *
 * The search runs on the document shown, the value alone or the values
 * gathered (`[…]`, `<data>…</data>`). It gives the place of each node found
 * in the text, to highlight it, and the document of the results, to show
 * only them:
 *
 * - JSON: the array of the values found, in the order of the JSONPath result;
 * - XML: the nodes found, in the order of the document, gathered in
 *   `<data>`; the attributes and the texts found as text. An XPath
 *   expression whose result is a number, a string or a boolean gives that
 *   value.
 *
 * The values keep their writing: the results are slices of the text,
 * reindented (`dataFormat.ts`). JSONPath reads the document with
 * `JSON.parse`: in a filter, an integer beyond 2^53 is compared
 * approximately.
 *
 * In XML, the indentation the explorer puts between the children of an
 * element (`isStructuredXml`) is left out of the document: `//text()` only
 * finds the texts of the values. XPath 1.0 has no default namespace: an
 * element of a namespace without prefix is found by
 * `*[local-name() = 'a']`; the prefixes declared in the document can be used.
 */

import { jsonLanguage } from '@codemirror/lang-json'
import { jsonpath } from 'json-p3'

import {
  DATA_INDENT,
  type XmlNode,
  isBlankXml,
  isStructuredXml,
  parseXml,
  reindentJson,
  xmlLines,
} from './dataFormat'

/**
 * Place of a node found in the text.
 */
export interface SearchRange {
  from: number
  to: number
}

/**
 * Result of a search.
 *
 * - `empty`: no expression;
 * - `error`: the expression is invalid, or cannot be evaluated;
 * - `nodes`: the nodes found (`count`), the places of those found in the
 *   text, in the order of the text (`matches`), the document of the
 *   results (`text`) and the place of each result in it (`results`);
 * - `value`: the number, string or boolean computed by an XPath expression.
 */
export type SearchResult =
  | { state: 'empty' }
  | { state: 'error'; message: string }
  | {
      state: 'nodes'
      count: number
      matches: SearchRange[]
      text: string
      results: SearchRange[]
    }
  | { state: 'value'; text: string }

/**
 * Runs a search expression on the document shown.
 *
 * @param kind Format of the document.
 * @param text Text of the document, well-formed (`dataDocument`).
 * @param expression JSONPath expression for JSON, XPath for XML.
 */
export function searchData(kind: 'json' | 'xml', text: string, expression: string): SearchResult {
  const trimmed = expression.trim()
  if (trimmed === '') {
    return { state: 'empty' }
  }
  return kind === 'json' ? searchJson(text, trimmed) : searchXml(text, trimmed)
}

/**
 * Error of an expression that cannot be evaluated.
 *
 * @param language Language of the expression.
 * @param error Error raised by the engine.
 */
function failure(language: 'JSONPath' | 'XPath', error: unknown): SearchResult {
  if (error instanceof RangeError) {
    return {
      state: 'error',
      message: 'Trop de résultats : la recherche dépasse les capacités du navigateur.',
    }
  }
  const detail = error instanceof Error ? error.message : String(error)
  // The message of the engine is in English: it is introduced, quoted as it is.
  return {
    state: 'error',
    message: `Expression ${language} invalide. Le moteur ${language} a signalé : ${detail}`,
  }
}

/**
 * Nodes found, in the order of the text, each place once: an expression can
 * find the same node twice (`$[0,0]`).
 *
 * @param ranges Places of the nodes found.
 */
function textOrder(ranges: SearchRange[]): SearchRange[] {
  const sorted = [...ranges].sort((a, b) => a.from - b.from || b.to - a.to)
  return sorted.filter(
    (range, index) =>
      index === 0 || range.from !== sorted[index - 1]!.from || range.to !== sorted[index - 1]!.to,
  )
}

/**
 * JSON document read for the search: its value, and the place of each value
 * in the text, by location (`JSON.stringify` of the keys and indices from the
 * root, as JSONPath gives them).
 */
interface JsonSource {
  text: string
  value: ReturnType<typeof JSON.parse>
  ranges: Map<string, SearchRange>
}

/**
 * Last JSON document read: the expression changes with each key pressed, the
 * document does not.
 */
let jsonSource: JsonSource | null = null

/**
 * Runs a JSONPath expression on a JSON document.
 *
 * @param text Text of the document, valid JSON.
 * @param expression JSONPath expression.
 */
function searchJson(text: string, expression: string): SearchResult {
  if (jsonSource?.text !== text) {
    jsonSource = { text, value: JSON.parse(text), ranges: jsonRanges(text) }
  }
  let nodes
  try {
    nodes = jsonpath.query(expression, jsonSource.value).nodes
  } catch (error) {
    return failure('JSONPath', error)
  }
  const matches: SearchRange[] = []
  const values = nodes.map((node) => {
    const range = jsonSource!.ranges.get(JSON.stringify(node.location))
    if (range === undefined) {
      return JSON.stringify(node.value)
    }
    matches.push(range)
    return text.slice(range.from, range.to)
  })
  const results = reindentJson(`[${values.join(',')}]`)
  const places = jsonRanges(results)
  return {
    state: 'nodes',
    count: nodes.length,
    matches: textOrder(matches),
    text: results,
    results: nodes.map((_, index) => places.get(JSON.stringify([index]))!),
  }
}

/**
 * Names of the nodes of a JSON value in the syntax tree of `@lezer/json`.
 */
const JSON_VALUES = new Set(['Object', 'Array', 'String', 'Number', 'True', 'False', 'Null'])

/**
 * Node of the syntax tree of a JSON text.
 */
type JsonNode = NonNullable<ReturnType<typeof jsonLanguage.parser.parse>['topNode']['firstChild']>

/**
 * Place of each value of a JSON text, by location. Of two members with the
 * same key, the last one wins, as in `JSON.parse`.
 *
 * @param text Valid JSON text.
 */
function jsonRanges(text: string): Map<string, SearchRange> {
  const ranges = new Map<string, SearchRange>()
  const visit = (node: JsonNode, location: (string | number)[]): void => {
    ranges.set(JSON.stringify(location), { from: node.from, to: node.to })
    let index = 0
    for (let child = node.firstChild; child !== null; child = child.nextSibling) {
      if (node.name === 'Array' && JSON_VALUES.has(child.name)) {
        visit(child, [...location, index++])
      } else if (child.name === 'Property') {
        const name = child.getChild('PropertyName')
        const value = child.lastChild
        if (name !== null && value !== null && JSON_VALUES.has(value.name)) {
          visit(value, [...location, JSON.parse(text.slice(name.from, name.to)) as string])
        }
      }
    }
  }
  const top = jsonLanguage.parser.parse(text).topNode.firstChild
  if (top !== null) {
    visit(top, [])
  }
  return ranges
}

/**
 * XML document read for the search: its DOM, without the indentation of the
 * explorer, the place of its nodes in the text, and the namespaces declared,
 * by prefix (the first declaration of a prefix wins).
 */
interface XmlSource {
  text: string
  document: XMLDocument
  ranges: Map<Node, SearchRange>
  namespaces: Map<string, string>
}

/**
 * Last XML document read.
 */
let xmlSource: XmlSource | null = null

/**
 * Runs an XPath expression on an XML document.
 *
 * @param text Text of the document, well-formed XML.
 * @param expression XPath expression.
 */
function searchXml(text: string, expression: string): SearchResult {
  if (xmlSource?.text !== text) {
    xmlSource = readXml(text)
  }
  const { document, namespaces, ranges } = xmlSource
  const resolver = (prefix: string | null) =>
    prefix === null ? null : (namespaces.get(prefix) ?? null)
  let snapshot: XPathResult
  try {
    const result = document.evaluate(expression, document, resolver, XPathResult.ANY_TYPE, null)
    switch (result.resultType) {
      case XPathResult.NUMBER_TYPE:
        return { state: 'value', text: String(result.numberValue) }
      case XPathResult.STRING_TYPE:
        return { state: 'value', text: result.stringValue }
      case XPathResult.BOOLEAN_TYPE:
        return { state: 'value', text: String(result.booleanValue) }
    }
    snapshot = document.evaluate(
      expression,
      document,
      resolver,
      XPathResult.ORDERED_NODE_SNAPSHOT_TYPE,
      null,
    )
  } catch (error) {
    return failure('XPath', error)
  }
  const matches: SearchRange[] = []
  const lines: string[] = []
  const results: SearchRange[] = []
  // Each result starts on a new line, after `<data>` and its indentation.
  let offset = '<data>\n'.length
  for (let index = 0; index < snapshot.snapshotLength; index++) {
    const node = snapshot.snapshotItem(index)!
    const range = ranges.get(node)
    if (range !== undefined) {
      matches.push(range)
    }
    const own = resultLines(text, node, range)
    if (own.length > 0) {
      const length = own.reduce((total, line) => total + line.length + 1, 0) - 1
      results.push({ from: offset + DATA_INDENT.length, to: offset + length })
      offset += length + 1
    }
    lines.push(...own)
  }
  return {
    state: 'nodes',
    count: snapshot.snapshotLength,
    matches: textOrder(matches),
    text: lines.length === 0 ? '<data/>' : ['<data>', ...lines, '</data>'].join('\n'),
    results,
  }
}

/**
 * Lines of a node found in the document of the results, inside `<data>`:
 * the attributes and the texts as text, the other nodes as written.
 *
 * @param text Text of the document.
 * @param node Node found.
 * @param range Its place in the text, if known.
 */
function resultLines(text: string, node: Node, range: SearchRange | undefined): string[] {
  if (node.nodeType === Node.DOCUMENT_NODE) {
    return xmlLines(text, 1)
  }
  if (node.nodeType === Node.ATTRIBUTE_NODE) {
    return [`${DATA_INDENT}${escapeXml((node as Attr).value)}`]
  }
  if (node.nodeType === Node.TEXT_NODE) {
    const written = range ? text.slice(range.from, range.to) : escapeXml(node.textContent ?? '')
    return written.trim() === '' ? [] : [`${DATA_INDENT}${written.trim()}`]
  }
  return range
    ? xmlLines(text.slice(range.from, range.to), 1)
    : [`${DATA_INDENT}${new XMLSerializer().serializeToString(node)}`]
}

/**
 * Escapes a text to write it as XML content.
 *
 * @param value Text.
 */
function escapeXml(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

/**
 * Reads an XML text for the search.
 *
 * @param text Well-formed XML text.
 */
function readXml(text: string): XmlSource {
  const document = new DOMParser().parseFromString(text, 'application/xml')
  const ranges = new Map<Node, SearchRange>([[document, { from: 0, to: text.length }]])
  const top = parseXml(text).filter(
    (node) => node.kind !== 'declaration' && !isBlankXml(text, node),
  )
  pairXml(text, document, top, false, ranges)
  const namespaces = new Map<string, string>()
  for (const element of document.getElementsByTagName('*')) {
    for (const attribute of element.attributes) {
      if (attribute.prefix === 'xmlns' && !namespaces.has(attribute.localName)) {
        namespaces.set(attribute.localName, attribute.value)
      }
    }
  }
  return { text, document, ranges, namespaces }
}

/**
 * DOM types of each kind of node of `parseXml`.
 */
const DOM_TYPES: Record<XmlNode['kind'], number[]> = {
  element: [Node.ELEMENT_NODE],
  text: [Node.TEXT_NODE],
  cdata: [Node.CDATA_SECTION_NODE],
  other: [Node.PROCESSING_INSTRUCTION_NODE, Node.COMMENT_NODE],
  doctype: [Node.DOCUMENT_TYPE_NODE],
  declaration: [],
}

/**
 * Matches the children of a DOM node with the nodes read in the text, in
 * the order of the document, and records their places, those of the
 * attributes included. In a structured element, the blanks between the
 * children are indentation of the explorer: they leave the DOM. If the two
 * lists differ (an entity of the DOCTYPE that holds markup), the places of
 * this level stay unknown.
 *
 * @param text Text of the document.
 * @param parent DOM node.
 * @param nodes Its children read in the text.
 * @param structured Is the parent a structured element?
 * @param ranges Places found, completed.
 */
function pairXml(
  text: string,
  parent: Node,
  nodes: XmlNode[],
  structured: boolean,
  ranges: Map<Node, SearchRange>,
): void {
  if (structured) {
    // A copy: `childNodes` is live, and loses the texts removed.
    for (const child of Array.from(parent.childNodes)) {
      if (child.nodeType === Node.TEXT_NODE) {
        parent.removeChild(child)
      }
    }
    nodes = nodes.filter((node) => node.kind !== 'text')
  }
  const children = [...parent.childNodes]
  if (
    children.length !== nodes.length ||
    children.some((child, index) => !DOM_TYPES[nodes[index]!.kind].includes(child.nodeType))
  ) {
    return
  }
  children.forEach((child, index) => {
    const node = nodes[index]!
    ranges.set(child, { from: node.from, to: node.to })
    if (node.kind === 'element') {
      const element = child as Element
      const attributes = attributeRanges(text, node.from, node.openEnd)
      for (const attribute of element.attributes) {
        const range = attributes.get(attribute.name)
        if (range !== undefined) {
          ranges.set(attribute, range)
        }
      }
      pairXml(text, child, node.children, isStructuredXml(text, node), ranges)
    }
  })
}

/**
 * Place of each attribute of a start tag, from its name to its closing
 * quote, by name as written.
 *
 * @param text Text of the document.
 * @param from Start of the tag.
 * @param to End of the tag.
 */
function attributeRanges(text: string, from: number, to: number): Map<string, SearchRange> {
  const ranges = new Map<string, SearchRange>()
  // Each match takes a whole quoted value: a `=` between quotes never
  // starts an attribute.
  for (const match of text.slice(from, to).matchAll(/\s([^\s=]+)\s*=\s*("[^"]*"|'[^']*')/g)) {
    const start = from + match.index + 1
    ranges.set(match[1]!, { from: start, to: from + match.index + match[0].length })
  }
  return ranges
}
