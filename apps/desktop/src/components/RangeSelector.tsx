import { SegmentedControl } from "./Primitives";
import type { RangeKey } from "../types";

const ranges: { key: RangeKey; label: string }[] = [
  { key: "today", label: "Today" },
  { key: "7d", label: "7D" },
  { key: "30d", label: "30D" },
  { key: "month", label: "Month" },
  { key: "all", label: "All" },
];

export function RangeSelector({ value, onChange }: { value: RangeKey; onChange: (range: RangeKey) => void }) {
  return <SegmentedControl label="Time range" value={value} options={ranges.map((range) => ({value:range.key,label:range.label}))} onChange={onChange} />;
}
