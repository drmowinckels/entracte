import { useId, useState, type KeyboardEvent } from "react";

export function InfoTip({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const popupId = useId();

  const toggle = () => setOpen((v) => !v);

  const onKeyDown = (e: KeyboardEvent<HTMLSpanElement>) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      toggle();
    } else if (e.key === "Escape" && open) {
      e.preventDefault();
      setOpen(false);
    }
  };

  return (
    <span
      className={open ? "info-tip info-tip-open" : "info-tip"}
      tabIndex={0}
      role="button"
      aria-label="More information"
      aria-expanded={open}
      aria-describedby={popupId}
      onKeyDown={onKeyDown}
      onClick={toggle}
      onBlur={() => setOpen(false)}
    >
      <span aria-hidden="true">i</span>
      <span id={popupId} className="info-tip-popup" role="tooltip">
        {text}
      </span>
    </span>
  );
}
