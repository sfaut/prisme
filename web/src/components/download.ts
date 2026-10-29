/**
 * Files the interface hands to the browser, which saves them like any
 * download (folder, progress, history): a file built in the page, and its
 * name.
 */

/**
 * Time a link to a file built in the page stays valid after the click: the
 * download starts asynchronously, and some browsers fail it when the link is
 * revoked at once.
 */
const LINK_LIFETIME = 40_000

/**
 * Characters that Windows refuses in a file name.
 */
const FORBIDDEN = /[\\/:*?"<>|]/g

/**
 * Saves a file built in the page.
 *
 * @param blob Content of the file.
 * @param name Proposed name.
 */
export function saveBlob(blob: Blob, name: string): void {
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = name
  link.click()
  setTimeout(() => URL.revokeObjectURL(url), LINK_LIFETIME)
}

/**
 * Name of a downloaded file: `<label>-YYYYMMDD-HHMMSS.<extension>`, in local
 * time (`docs/IDEAS-EXPORTS.md`, section 4). The characters a file name
 * cannot hold become `_`.
 *
 * @param label Name of what is downloaded (result tab).
 * @param extension Extension, without its dot.
 * @param date Instant of the download.
 */
export function downloadName(label: string, extension: string, date: Date = new Date()): string {
  const pad = (value: number) => String(value).padStart(2, '0')
  const day = `${date.getFullYear()}${pad(date.getMonth() + 1)}${pad(date.getDate())}`
  const time = `${pad(date.getHours())}${pad(date.getMinutes())}${pad(date.getSeconds())}`
  return `${label.trim().replace(FORBIDDEN, '_')}-${day}-${time}.${extension}`
}
