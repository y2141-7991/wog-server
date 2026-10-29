import { useEffect, useState } from 'react'
import { fetchApi } from '../api/marketplace'
import { ApiAvatar } from '../components/ApiCard'
import { TryIt } from '../components/TryIt'
import { timeAgo } from '../lib/format'
import type { MarketplaceApi } from '../types'

const GATEWAY = import.meta.env.VITE_API_URL || window.location.origin

// Only required params go in the examples: an optional one such as `cursor` must be left out
// for the example to return the first page.
function exampleParams(api: MarketplaceApi) {
  return api.params.filter((p) => p.required)
}

function curlFor(api: MarketplaceApi) {
  const query = exampleParams(api)
    .map((p) => `${p.name}=${encodeURIComponent(p.example)}`)
    .join('&')
  return `curl -H "X-API-Key: YOUR_API_KEY" \\\n  "${GATEWAY}${api.gateway_path}${query ? `?${query}` : ''}"`
}

function pythonFor(api: MarketplaceApi) {
  const params = exampleParams(api)
  const lines = [
    'import requests',
    '',
    'response = requests.get(',
    `    ${JSON.stringify(GATEWAY + api.gateway_path)},`,
    '    headers={"X-API-Key": "YOUR_API_KEY"},',
  ]
  if (params.length) {
    lines.push(`    params={${params.map((p) => `${JSON.stringify(p.name)}: ${JSON.stringify(p.example)}`).join(', ')}},`)
  }
  lines.push(')', 'print(response.json())')
  return lines.join('\n')
}

type Lang = 'curl' | 'python'

export function ApiDetailPage({ slug, onBack, onDashboard }: { slug: string; onBack: () => void; onDashboard: () => void }) {
  const [api, setApi] = useState<MarketplaceApi | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [lang, setLang] = useState<Lang>('curl')

  useEffect(() => {
    let cancelled = false
    fetchApi(slug)
      .then((a) => !cancelled && setApi(a))
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : 'Could not load this API'))
    return () => {
      cancelled = true
    }
  }, [slug])

  if (error) {
    return (
      <div className="detail">
        <button className="link-btn" onClick={onBack}>← All APIs</button>
        <p className="error">{error}</p>
      </div>
    )
  }
  if (!api) return <div className="detail"><p className="text-muted">Loading...</p></div>

  return (
    <div className="detail">
      <button className="link-btn" onClick={onBack}>← All APIs</button>

      <header className="detail-head">
        <ApiAvatar api={api} size="lg" />
        <div>
          <h1>{api.name}</h1>
          <p className="text-muted">
            By {api.provider} · Updated {timeAgo(api.updated_at)} <span className="tag">{api.category}</span>
          </p>
        </div>
      </header>
      <p className="detail-desc">{api.description}</p>

      <div className="detail-cols">
        <div className="detail-docs">
          <section className="panel">
            <h3>Endpoint</h3>
            <p><code className="method">GET</code> <code>{api.gateway_path}</code></p>
            <h4>Parameters</h4>
            <table className="params">
              <tbody>
                {api.params.map((p) => (
                  <tr key={p.name}>
                    <td><code>{p.name}</code>{p.required && <span className="req"> required</span>}</td>
                    <td>{p.type}</td>
                    <td>{p.description}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <h4>Example request</h4>
            <div className="chips chips-tight">
              <button className={`chip${lang === 'curl' ? ' chip-active' : ''}`} onClick={() => setLang('curl')}>cURL</button>
              <button className={`chip${lang === 'python' ? ' chip-active' : ''}`} onClick={() => setLang('python')}>Python</button>
            </div>
            <pre>{lang === 'curl' ? curlFor(api) : pythonFor(api)}</pre>
            {api.example_response != null && (
              <>
                <h4>Example response</h4>
                <pre>{JSON.stringify(api.example_response, null, 2)}</pre>
              </>
            )}
          </section>

          <section className="panel">
            <h3>Free plan</h3>
            <p>
              {api.monthly_quota.toLocaleString()} requests per month · {api.rate_limit_per_minute} requests per minute.
              Create a key on the <button className="link-btn" onClick={onDashboard}>dashboard</button>.
            </p>
          </section>
        </div>

        <TryIt api={api} />
      </div>
    </div>
  )
}
