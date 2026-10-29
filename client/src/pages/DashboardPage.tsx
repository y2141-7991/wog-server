import { useCallback, useEffect, useState } from 'react'
import { createKey, fetchKeys, fetchUsage, revokeKey, saveKey } from '../api/marketplace'
import type { ApiKey, UsageResponse } from '../types'

export function DashboardPage() {
  const [keys, setKeys] = useState<ApiKey[]>([])
  const [usage, setUsage] = useState<UsageResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [keyName, setKeyName] = useState('')
  const [newSecret, setNewSecret] = useState<string | null>(null)

  const refreshKeys = useCallback(() => fetchKeys().then(setKeys), [])

  useEffect(() => {
    let cancelled = false
    Promise.all([fetchKeys(), fetchUsage()])
      .then(([k, u]) => {
        if (!cancelled) {
          setKeys(k)
          setUsage(u)
        }
      })
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : 'Could not load the dashboard'))
    return () => {
      cancelled = true
    }
  }, [])

  async function act(action: () => Promise<unknown>) {
    setError(null)
    try {
      await action()
      await refreshKeys()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Something went wrong')
    }
  }

  const maxDay = Math.max(1, ...(usage?.by_day.map((d) => d.requests) ?? [1]))

  return (
    <div className="detail">
      <h1>Dashboard</h1>
      {error && <p className="error">{error}</p>}

      <section className="panel">
        <h3>API keys</h3>
        {newSecret && (
          <p className="secret">
            Copy your new key now — it will not be shown again.
            <code>{newSecret}</code>
          </p>
        )}
        <form
          className="inline-form"
          onSubmit={(e) => {
            e.preventDefault()
            void act(async () => {
              const created = await createKey(keyName.trim())
              setNewSecret(created.secret)
              saveKey(created.secret)
              setKeyName('')
            })
          }}
        >
          <input value={keyName} onChange={(e) => setKeyName(e.target.value)} placeholder="Key name, e.g. my-scraper" maxLength={100} required />
          <button type="submit" className="btn-primary">Create key</button>
        </form>
        <table className="params">
          <tbody>
            {keys.map((k) => (
              <tr key={k.id}>
                <td><code>{k.key_prefix}…</code></td>
                <td>{k.name}</td>
                <td className="text-muted">{k.last_used_at ? `Used ${new Date(k.last_used_at).toLocaleString()}` : 'Never used'}</td>
                <td>
                  {k.revoked_at ? (
                    <span className="text-muted">Revoked</span>
                  ) : (
                    <button className="btn-secondary btn-sm" onClick={() => void act(() => revokeKey(k.id))}>Revoke</button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {keys.length === 0 && <p className="text-muted">No keys yet.</p>}
      </section>

      <section className="panel">
        <h3>Usage — last {usage?.days ?? 30} days</h3>
        {usage?.by_api.map((u) => (
          <div key={u.slug} className="usage-row">
            <span>{u.name}</span>
            <progress value={u.used_this_month} max={u.monthly_quota} />
            <span className="text-muted">
              {u.used_this_month.toLocaleString()} / {u.monthly_quota.toLocaleString()} this month · {u.requests} requests, {u.errors} errors
            </span>
          </div>
        ))}
        <div className="bars">
          {usage?.by_day.map((d) => (
            <div key={d.day} title={`${d.day}: ${d.requests}`} style={{ height: `${(d.requests / maxDay) * 100}%` }} />
          ))}
        </div>
      </section>
    </div>
  )
}
