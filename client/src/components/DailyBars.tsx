import type { UsageByDay } from '../types'

export function DailyBars({ days }: { days: UsageByDay[] }) {
  const max = Math.max(1, ...days.map((d) => d.requests))
  return (
    <div className="bars">
      {days.map((d) => (
        <div key={d.day} title={`${d.day}: ${d.requests}`} style={{ height: `${(d.requests / max) * 100}%` }} />
      ))}
    </div>
  )
}
