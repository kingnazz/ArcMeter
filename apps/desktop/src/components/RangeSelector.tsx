import type { RangeKey } from "../types";

const ranges: { key: RangeKey; label: string }[] = [
  { key: "today", label: "Today" },
  { key: "7d", label: "7D" },
  { key: "30d", label: "30D" },
  { key: "month", label: "Month" },
  { key: "all", label: "All" },
];

export function RangeSelector({ value, onChange }: { value: RangeKey; onChange: (range: RangeKey) => void }) {
  return (
    <div className="range-selector" role="group" aria-label="Time range">
      {ranges.map((range) => (
        <button key={range.key} type="button" className={range.key === value ? "active" : ""} onClick={() => onChange(range.key)}>
          {range.label}
        </button>
      ))}
    </div>
  );
}
