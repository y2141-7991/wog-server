import type { MarketplaceApi } from '../types'
import { compact, timeAgo } from '../lib/format'

function Icon({ d }: { d: string }) {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d={d} />
    </svg>
  )
}

export function ApiAvatar({ api, size = 'sm' }: { api: MarketplaceApi; size?: 'sm' | 'lg' }) {
  return <span className={`api-avatar api-avatar-${size}`}>{api.provider[0].toUpperCase()}</span>
}

export function ApiCard({ api, onOpen }: { api: MarketplaceApi; onOpen: () => void }) {
  return (
    <button className="api-card" onClick={onOpen}>
      <span className="tag">{api.category}</span>
      <span className="api-card-main">
        <ApiAvatar api={api} />
        <span className="api-card-text">
          <span className="api-card-name">{api.name}</span>
          <span className="api-card-desc">{api.description}</span>
        </span>
      </span>
      <span className="api-card-meta">
        <span>By {api.provider}</span>
        <span>Updated {timeAgo(api.updated_at)}</span>
      </span>
      <span className="api-card-metrics">
        <span className="pill" title="Total calls">
          <Icon d="M3 17l6-6 4 4 8-8M15 7h6v6" />
          {compact(api.total_calls)}
        </span>
        <span className="pill" title="Average latency, last 30 days">
          <Icon d="M12 6v6l4 2M12 22a10 10 0 100-20 10 10 0 000 20z" />
          {api.avg_latency_ms != null ? `${api.avg_latency_ms}ms` : '–'}
        </span>
        <span className="pill" title="Success rate, last 30 days">
          <Icon d="M20 6L9 17l-5-5" />
          {api.success_rate != null ? `${Math.round(api.success_rate)}%` : '–'}
        </span>
      </span>
    </button>
  )
}
