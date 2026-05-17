import type { ReactNode } from "react";

export function Advanced({
  label = "Show advanced",
  children,
}: {
  label?: string;
  children: ReactNode;
}) {
  return (
    <details className="advanced-section">
      <summary>{label}</summary>
      <div className="advanced-body">{children}</div>
    </details>
  );
}
