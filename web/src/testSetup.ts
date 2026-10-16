/**
 * Préparation des tests Vitest, chargée avant chaque fichier de test
 * (`setupFiles` de `vitest.config.ts`).
 *
 * jsdom ne calcule aucune mise en page : son `Range` n'a ni
 * `getClientRects` ni `getBoundingClientRect`. CodeMirror s'en sert pour
 * placer la sélection et les soulignements d'erreur ; sans elles, il écrit
 * une `TypeError` dans la console, selon le moment où il mesure. Les deux
 * méthodes rendent ici des rectangles vides, comme un texte non affiché.
 */

if (typeof Range !== 'undefined') {
  Range.prototype.getClientRects ??= function (): DOMRectList {
    const rects: DOMRect[] = []
    return Object.assign(rects, { item: (index: number) => rects[index] ?? null })
  }
  Range.prototype.getBoundingClientRect ??= () => new DOMRect(0, 0, 0, 0)
}
