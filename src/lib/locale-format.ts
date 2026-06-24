export type DateField = "day" | "month" | "year";

/** The order day/month/year appear in for a locale — e.g. `en-US` →
 * `["month","day","year"]`, `en-GB` → `["day","month","year"]`, `ja-JP` →
 * `["year","month","day"]`. Falls back to ISO order if Intl can't resolve
 * the parts (unknown locale / missing ICU data). The locale is passed
 * explicitly because the WebView's default Intl locale is unreliable in a
 * non-localised app (it reports en-US even when the OS region differs). */
export function dateFieldOrder(locale: string): DateField[] {
  try {
    const parts = new Intl.DateTimeFormat(locale, {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    }).formatToParts(new Date(2000, 0, 2));
    const order = parts
      .map((p) => p.type)
      .filter(
        (t): t is DateField => t === "day" || t === "month" || t === "year",
      );
    return order.length === 3 ? order : ["year", "month", "day"];
  } catch {
    return ["year", "month", "day"];
  }
}

/** Localised full month names (index 0 = January) for the given locale. */
export function monthNames(locale: string): string[] {
  try {
    const fmt = new Intl.DateTimeFormat(locale, { month: "long" });
    return Array.from({ length: 12 }, (_, i) =>
      fmt.format(new Date(2000, i, 1)),
    );
  } catch {
    return [
      "January",
      "February",
      "March",
      "April",
      "May",
      "June",
      "July",
      "August",
      "September",
      "October",
      "November",
      "December",
    ];
  }
}
