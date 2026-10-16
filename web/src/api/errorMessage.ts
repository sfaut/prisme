/**
 * Text of a failure, for the messages of the interface.
 *
 * Errors thrown by the code are `Error` objects, whose message is shown; any
 * other value (a rejected string, for example) is shown as it is.
 */

/**
 * Text of a caught or rejected value.
 *
 * @param failure Value caught or rejected.
 */
export function errorMessage(failure: unknown): string {
  return failure instanceof Error ? failure.message : String(failure)
}
