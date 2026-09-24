// Stand-in for `@excalidraw/common`, holding only what textWrapping.ts and
// textMeasurements.ts import. Values copied from packages/common/src at the pinned SHA;
// none of the constants reach `wrapText`, they only have to exist for the import to link.

// constants.ts:404, :216, :261 (FONT_FAMILY.Excalifont)
export const BOUND_TEXT_PADDING = 5;
export const DEFAULT_FONT_SIZE = 20;
export const DEFAULT_FONT_FAMILY = 5;

// Dev mode ON, so textWrapping.ts `satisfiesWordInvariant` throws if wrapWord is ever
// handed a word holding whitespace — the generator runs the oracle with its guard armed.
export const isDevEnv = () => true;
export const isTestEnv = () => false;

// utils.ts:139 (reduced to what the metrics provider needs: a distinct string per font)
export const getFontString = ({ fontSize, fontFamily }: { fontSize: number; fontFamily: number }) =>
  `${fontSize}px font${fontFamily}`;

// utils.ts:1038, verbatim
export const normalizeEOL = (str: string) => {
  return str.replace(/\r?\n|\r/g, "\n");
};
