import { useCallback, useEffect, useState } from 'react'
import { fetchSubscription } from '../api/billing'
import { createKey, fetchKeys, fetchUsage, revokeKey, saveKey } from '../api/marketplace'
import { CopyButton } from '../components/CopyButton'
import { NEAR_LIMIT, usePlanStatus } from '../hooks/usePlanStatus'
import type { ApiKey, Subscription, UsageResponse } from '../types'

export function DashboardPage({ onUpgrade }: { onUpgrade: () => void }) {
  const plan = usePlanStatus('dashboard')
  const [keys, setKeys] = useState<ApiKey[]>([])
  const [usage, setUsage] = useState<UsageResponse | null>(null)
  const [subscription, setSubscription] = useState<Subscription | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [keyName, setKeyName] = useState('')
  const [newSecret, setNewSecret] = useState<string | null>(null)

  const refreshKeys = useCallback(() => fetchKeys().then(setKeys), [])

  useEffect(() => {
    let cancelled = false
    Promise.all([fetchKeys(), fetchUsage(), fetchSubscription().catch(() => null)])
      .then(([k, u, sub]) => {
        if (!cancelled) {
          setKeys(k)
          setUsage(u)
          setSubscription(sub)
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

  const maxDay = Math.max(0, ...(usage?.by_day.map((d) => d.requests) ?? [0]))
  // A paid plan has one quota shared by every API; per-API quotas only apply to the free tier.
  const pool = subscription?.plan && subscription.usage.quota
    ? { used: subscription.usage.used, quota: subscription.usage.quota }
    : null

  return (
    <div className="detail">
      <h1>Dashboard</h1>
      {error && <p className="error">{error}</p>}
      {plan.planName && plan.ratio >= NEAR_LIMIT && (
        <p className="upgrade-hint">
          {plan.ratio >= 1 ? 'You have used up your quota' : `You have used ${Math.round(plan.ratio * 100)}% of your quota`}
          {plan.paid ? ` on the ${plan.planName} plan.` : ' on the free plan.'}{' '}
          <button className="link-btn" onClick={onUpgrade}>{plan.paid ? 'See plans' : 'Upgrade your plan'}</button>
        </p>
      )}

      <section className="panel">
        <h3>API keys</h3>
        {newSecret && (
          <div className="secret">
            <span>Copy your new key now — it will not be shown again.</span>
            <div className="secret-row">
              <code>{newSecret}</code>
              <CopyButton value={newSecret} label="Copy key" />
            </div>
          </div>
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
        {pool && (
          <div className="usage-row">
            <span>{subscription?.plan?.name} plan</span>
            <progress value={pool.used} max={pool.quota} />
            <span className="text-muted">
              {pool.used.toLocaleString()} / {pool.quota.toLocaleString()} requests in this period, shared by all APIs
              {subscription?.plan && ` · until ${new Date(subscription.plan.current_period_end).toLocaleDateString()}`}
            </span>
          </div>
        )}
        {usage?.by_api.map((u) =>
          pool ? (
            <div key={u.slug} className="usage-row">
              <span>{u.name}</span>
              <span className="text-muted usage-span">{u.requests.toLocaleString()} requests, {u.errors.toLocaleString()} errors</span>
            </div>
          ) : (
            <div key={u.slug} className="usage-row">
              <span>{u.name}</span>
              <progress value={u.used_this_month} max={u.monthly_quota} />
              <span className="text-muted">
                {u.used_this_month.toLocaleString()} / {u.monthly_quota.toLocaleString()} this month · {u.requests} requests, {u.errors} errors
              </span>
            </div>
          ),
        )}
        <h4>Requests per day</h4>
        {maxDay > 0 ? (
          <>
            <div className="bars">
              {usage?.by_day.map((d) => (
                <div key={d.day} title={`${d.day}: ${d.requests}`} style={{ height: `${(d.requests / maxDay) * 100}%` }} />
              ))}
            </div>
            <p className="text-muted bars-caption">Busiest day: {maxDay.toLocaleString()} requests</p>
          </>
        ) : (
          <p className="text-muted">No requests yet.</p>
        )}
      </section>
    </div>
  )
}
