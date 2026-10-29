import { useState } from 'react'
import { callGateway, getSavedKey, saveKey } from '../api/marketplace'
import type { MarketplaceApi } from '../types'

export function TryIt({ api }: { api: MarketplaceApi }) {
  const [apiKey, setApiKey] = useState(getSavedKey)
  // Only required params start filled in; an optional one such as `cursor` must stay
  // empty or the first call would not return the first page.
  const [values, setValues] = useState<Record<string, string>>(
    Object.fromEntries(api.params.map((p) => [p.name, p.required ? p.example : ''])),
  )
  const [running, setRunning] = useState(false)
  const [result, setResult] = useState<{ status: number; ms: number; text: string } | null>(null)
  const [nextCursor, setNextCursor] = useState<string | null>(null)
  const paged = api.params.some((p) => p.name === 'cursor')

  async function call(params: Record<string, string>) {
    setRunning(true)
    saveKey(apiKey.trim())
    try {
      const r = await callGateway(api.gateway_path, apiKey.trim(), params)
      setResult({ status: r.status, ms: r.ms, text: JSON.stringify(r.body, null, 2) })
      const next = (r.body as { next_cursor?: unknown } | null)?.next_cursor
      setNextCursor(typeof next === 'string' ? next : null)
    } catch (err) {
      setResult({ status: 0, ms: 0, text: err instanceof Error ? err.message : 'Request failed' })
      setNextCursor(null)
    } finally {
      setRunning(false)
    }
  }

  function run(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault()
    void call(values)
  }

  function loadNextPage() {
    if (nextCursor === null) return
    const next = { ...values, cursor: nextCursor }
    setValues(next)
    void call(next)
  }

  return (
    <form className="panel tryit" onSubmit={run}>
      <h3>Test endpoint</h3>
      <label>
        X-API-Key
        <input
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="wog_..."
          autoComplete="off"
          spellCheck={false}
          required
        />
      </label>
      {api.params.map((p) => (
        <label key={p.name}>
          {p.name}{p.required && ' *'}
          <input
            value={values[p.name] ?? ''}
            onChange={(e) => setValues({ ...values, [p.name]: e.target.value })}
            placeholder={p.required ? undefined : 'optional'}
            required={p.required}
          />
        </label>
      ))}
      <button type="submit" className="btn-primary" disabled={running}>
        {running ? 'Calling...' : 'Test Endpoint'}
      </button>
      {result && (
        <>
          <p className={result.status >= 200 && result.status < 300 ? 'status-ok' : 'status-bad'}>
            {result.status ? `HTTP ${result.status} · ${result.ms} ms` : 'Error'}
          </p>
          <pre>{result.text}</pre>
          {paged && nextCursor !== null && (
            <button type="button" className="btn-secondary" onClick={loadNextPage} disabled={running}>
              Next page (cursor {nextCursor})
            </button>
          )}
        </>
      )}
    </form>
  )
}
