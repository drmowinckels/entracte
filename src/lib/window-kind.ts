export type WindowKind = "main" | "overlay" | "pause";

export function readWindowKind(search: string): WindowKind {
  const kind = new URLSearchParams(search).get("window");
  if (kind === "overlay") return "overlay";
  if (kind === "pause") return "pause";
  return "main";
}

export function titleForWindow(kind: WindowKind): string {
  switch (kind) {
    case "overlay":
      return "Entracte — Break";
    case "pause":
      return "Entracte — Pause";
    default:
      return "Entracte — Settings";
  }
}

export const windowKind: WindowKind = readWindowKind(window.location.search);
