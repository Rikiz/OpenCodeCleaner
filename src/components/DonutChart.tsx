interface DonutSlice {
  value: number;
  color: string;
  label: string;
}

interface DonutProps {
  slices: DonutSlice[];
  centerValue: string;
  centerLabel: string;
}

export function DonutChart({ slices, centerValue, centerLabel }: DonutProps) {
  const total = slices.reduce((s, x) => s + x.value, 0) || 1;
  const r = 56;
  const c = 2 * Math.PI * r;
  let offset = 0;
  const cx = 75;
  const circ = slices
    .filter((s) => s.value > 0)
    .map((s, i) => {
      const frac = s.value / total;
      const len = frac * c;
      const dash = `${len} ${c - len}`;
      const rot = (offset / c) * 360 - 90;
      offset += len;
      return { ...s, dash, rot, key: i };
    });

  return (
    <div className="donut">
      <svg width="150" height="150" viewBox="0 0 150 150">
        <circle
          cx="75"
          cy="75"
          r={r}
          fill="none"
          stroke="rgba(255,255,255,0.05)"
          strokeWidth="13"
        />
        {circ.map((s) => (
          <circle
            key={s.key}
            cx={75}
            cy={75}
            r={r}
            fill="none"
            stroke={s.color}
            strokeWidth="13"
            strokeDasharray={s.dash}
            strokeDashoffset={0}
            transform={`rotate(${s.rot} ${cx} ${cx})`}
            style={{ transition: "stroke-dasharray 0.9s cubic-bezier(0.16,1,0.3,1)" }}
            strokeLinecap="butt"
          />
        ))}
      </svg>
      <div className="donut-center">
        <div className="donut-value">{centerValue}</div>
        <div className="donut-label">{centerLabel}</div>
      </div>
    </div>
  );
}
