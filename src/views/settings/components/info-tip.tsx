import { useId, useState, type KeyboardEvent } from "react";

export function InfoTip({ text, warn }: { text: string; warn?: boolean }) {
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

  const classes = ["info-tip"];
  if (warn) classes.push("info-tip-warn");
  if (open) classes.push("info-tip-open");

  return (
    <span
      className={classes.join(" ")}
      tabIndex={0}
      role="button"
      aria-label={warn ? "Warning" : "More information"}
      aria-expanded={open}
      aria-describedby={popupId}
      onKeyDown={onKeyDown}
      onClick={toggle}
      onBlur={() => setOpen(false)}
    >
      <span aria-hidden="true">{warn ? "!" : "i"}</span>
      <span id={popupId} className="info-tip-popup" role="tooltip">
        {text}
      </span>
    </span>
  );
}
