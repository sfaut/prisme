/**
 * Look of a single-line input field, shared by `UiInput.vue` and the fields a dependency draws
 * (`ListboxFilter` of Reka UI).
 */

/**
 * Classes of the field common to both sizes.
 */
const LOOK =
  'w-full min-w-0 rounded-control border border-line bg-surface text-fg-strong [appearance:textfield] placeholder:text-fg-faint focus-visible:outline-(length:--focus-width) focus-visible:outline-offset-(--focus-offset) focus-visible:outline-primary'

/**
 * Classes of the field, at the regular size of the controls.
 */
export const INPUT_FIELD = `h-8 px-3 text-base ${LOOK}`

/**
 * Classes of the field, at the small size of the controls. Two steps of padding, one more than a
 * small button: the focus ring covers the border, and the text keeps its distance from it.
 */
export const INPUT_FIELD_SMALL = `h-7 px-2 text-sm ${LOOK}`
