import { useMemo, useState } from "react";
import { formatSpeed } from "@/lib/format";

interface Props {
  data: number[];
  height?: number;
}

/** Single-series throughput sparkline: thin line, recessive fill, hover readout. */
export function Sparkline({ data, height = 44 }: Props) {
  const [hover, setHover] = useState<number | null>(null);
  const w = 100;
  const max = Math.max(1, ...data);
  const pts = useMemo(() => data.map((v, i) => [(i / (data.length - 1)) * w, height - 2 - (v / max) * (height - 6)] as const), [data, max, height]);
  const line = pts.map(([x, y]) => `${x.toFixed(2)},${y.toFixed(2)}`).join(" ");
  const area = `0,${height} ${line} ${w},${height}`;
  const hp = hover != null ? pts[hover] : null;

  return (
    <div className="relative">
      <svg
        viewBox={`0 0 ${w} ${height}`}
        preserveAspectRatio="none"
        className="block h-11 w-full"
        onMouseMove={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          setHover(Math.round(((e.clientX - r.left) / r.width) * (data.length - 1)));
        }}
        onMouseLeave={() => setHover(null)}
      >
        <defs>
          <linearGradient id="spark-fill" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="var(--live)" stopOpacity="0.28" />
            <stop offset="100%" stopColor="var(--live)" stopOpacity="0" />
          </linearGradient>
        </defs>
        <polygon points={area} fill="url(#spark-fill)" />
        <polyline points={line} fill="none" stroke="var(--live)" strokeWidth="1.5" vectorEffect="non-scaling-stroke" strokeLinejoin="round" strokeLinecap="round" />
        {hp && (
          <>
            <line x1={hp[0]} x2={hp[0]} y1="0" y2={height} stroke="var(--border)" strokeWidth="1" vectorEffect="non-scaling-stroke" />
            <circle cx={hp[0]} cy={hp[1]} r="2.5" fill="var(--live)" stroke="var(--background)" strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
          </>
        )}
      </svg>
      {hover != null && (
        <div className="pointer-events-none absolute -top-5 right-0 rounded border border-border bg-popover px-1.5 py-0.5 font-mono text-[10px] text-foreground">
          {formatSpeed(data[hover])}
        </div>
      )}
    </div>
  );
}
