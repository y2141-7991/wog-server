import { useMemo, useState } from 'react'
import { useCatalog } from '../api/queries'
import { ApiCard } from '../components/ApiCard'
import { useNavigationStore } from '../stores/navigation'
import type { MarketplaceApi } from '../types'

type Sort = 'relevance' | 'calls' | 'latency' | 'updated'

export function HubPage() {
  const query = useNavigationStore((s) => s.query)
  const setQuery = useNavigationStore((s) => s.setQuery)
  const navigate = useNavigationStore((s) => s.navigate)
  const { data, error: loadError } = useCatalog()
  const apis = data ?? null
  // With a cached copy on screen, a failed background refresh is not worth an error.
  const error = !data && loadError ? loadError.message || 'Could not load the catalog' : null
  const [category, setCategory] = useState('All')
  const [sort, setSort] = useState<Sort>('relevance')

  const categories = useMemo(() => ['All', ...new Set((apis ?? []).map((a) => a.category))], [apis])

  const results = useMemo(() => {
    const q = query.trim().toLowerCase()
    const matches = (apis ?? []).filter(
      (a) =>
        (category === 'All' || a.category === category) &&
        (!q || [a.name, a.description, a.provider, a.category].some((f) => f.toLowerCase().includes(q))),
    )
    const byName = (a: MarketplaceApi, b: MarketplaceApi) => a.name.localeCompare(b.name)
    const order: Record<Sort, (a: MarketplaceApi, b: MarketplaceApi) => number> = {
      relevance: byName,
      calls: (a, b) => b.total_calls - a.total_calls,
      latency: (a, b) => (a.avg_latency_ms ?? Infinity) - (b.avg_latency_ms ?? Infinity),
      updated: (a, b) => b.updated_at.localeCompare(a.updated_at),
    }
    return [...matches].sort(order[sort])
  }, [apis, query, category, sort])

  return (
    <div className="hub">
      <h1 className="hub-lead">Find an API, get a key, and start calling it in minutes.</h1>

      <section className="results">
        <div className="results-head">
          <h2>
            {query.trim() ? `All Results For "${query.trim()}"` : 'All APIs'} ({results.length})
          </h2>
          <select value={sort} onChange={(e) => setSort(e.target.value as Sort)} aria-label="Sort">
            <option value="relevance">By Relevance</option>
            <option value="calls">Most Used</option>
            <option value="latency">Fastest</option>
            <option value="updated">Recently Updated</option>
          </select>
        </div>

        <div className="filterbar">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
            <circle cx="11" cy="11" r="7" />
            <path d="M21 21l-4.3-4.3" />
          </svg>
          <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search APIs" spellCheck={false} />
          {query && <button className="link-btn" onClick={() => setQuery('')}>Clear</button>}
        </div>

        {categories.length > 2 && (
          <div className="chips">
            {categories.map((c) => (
              <button key={c} className={`chip${c === category ? ' chip-active' : ''}`} onClick={() => setCategory(c)}>
                {c}
              </button>
            ))}
          </div>
        )}

        {error && <p className="error">{error}</p>}
        {apis === null && !error && <p className="text-muted">Loading...</p>}
        {apis !== null && results.length === 0 && <p className="text-muted">No APIs match your search.</p>}

        <div className="api-grid">
          {results.map((api) => (
            <ApiCard key={api.id} api={api} onOpen={() => navigate({ name: 'api', slug: api.slug })} />
          ))}
        </div>
      </section>
    </div>
  )
}
