export type WindowKind = "main" | "overlay";

export function readWindowKind(search: string): WindowKind {
  return new URLSearchParams(search).get("window") === "overlay"
    ? "overlay"
    : "main";
}

export function titleForWindow(kind: WindowKind): string {
  return kind === "overlay" ? "Entracte — Break" : "Entracte — Settings";
}

export const windowKind: WindowKind = readWindowKind(window.location.search);
