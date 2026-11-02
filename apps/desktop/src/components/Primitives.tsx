import type { ReactNode } from "react";

export function SectionHeader({ title, detail, action }: { title: string; detail?: string; action?: ReactNode }) {
  return <div className="panel-heading"><div><h2>{title}</h2>{detail ? <p>{detail}</p> : null}</div>{action}</div>;
}

export function EmptyState({ icon, title, detail, action }: { icon: ReactNode; title: string; detail: string; action?: ReactNode }) {
  return <div className="substantial-empty">{icon}<h2>{title}</h2><p>{detail}</p>{action}</div>;
}

export function SegmentedControl<T extends string>({ label, value, options, onChange }: {
  label: string; value: T; options: readonly { value: T; label: string; icon?: ReactNode }[]; onChange: (value: T) => void;
}) {
  return <div className="segmented-control" role="group" aria-label={label}>
    {options.map((option, index) => <button type="button" key={option.value} aria-pressed={value === option.value}
      className={value === option.value ? "active" : ""} onClick={() => onChange(option.value)}
      onKeyDown={(event) => {
        const offset = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
        if (!offset) return;
        event.preventDefault();
        const next = (index + offset + options.length) % options.length;
        onChange(options[next]!.value);
        event.currentTarget.parentElement?.querySelectorAll("button")[next]?.focus();
      }}>{option.icon}{option.label}</button>)}
  </div>;
}
