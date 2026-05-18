import { useEffect } from "react";

/** Apply a supporter-supplied stylesheet to the current document via
 * `adoptedStyleSheets`. Using the Constructable StyleSheet API means
 * we never need `'unsafe-inline'` in `style-src` — a strict CSP holds
 * for free users who can't set this field. Empty input is a no-op.
 *
 * `replaceSync` parses the whole stylesheet; a parser-fatal error
 * surfaces in the console but doesn't break the page. */
export function useCustomStylesheet(css: string): void {
  useEffect(() => {
    if (!css) return;
    let sheet: CSSStyleSheet;
    try {
      sheet = new CSSStyleSheet();
      sheet.replaceSync(css);
    } catch (e) {
      console.error("custom_css replaceSync rejected:", e);
      return;
    }
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
    return () => {
      document.adoptedStyleSheets = document.adoptedStyleSheets.filter(
        (s) => s !== sheet,
      );
    };
  }, [css]);
}
