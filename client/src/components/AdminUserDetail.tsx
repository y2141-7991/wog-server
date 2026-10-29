import { useEffect, useState } from 'react'
import { fetchUserDetail, setRole } from '../api/admin'
import type { AdminUserDetail as Detail } from '../types'
import { DailyBars } from './DailyBars'

interface Props {
  userId: string
  days: number
  isSelf: boolean
  onClose: () => void
  onRoleChanged: () => void
}

export function AdminUserDetail({ userId, days, isSelf, onClose, onRoleChanged }: Props) {
  const [detail, setDetail] = useState<Detail | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [confirming, setConfirming] = useState(false)

  useEffect(() => {
    let cancelled = false
    fetchUserDetail(userId, days)
      .then((d) => !cancelled && setDetail(d))
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : 'Could not load this user'))
    return () => {
      cancelled = true
    }
  }, [userId, days])

  async function toggleRole() {
    if (!detail) return
    setError(null)
    try {
      const updated = await setRole(userId, detail.user.role === 'admin' ? 'user' : 'admin')
      setDetail({ ...detail, user: updated })
      setConfirming(false)
      onRoleChanged()
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not change the role')
    }
  }

  return (
    <section className="panel">
      <div className="panel-head">
        <h3>{detail ? `${detail.user.username} · ${detail.user.email}` : 'Loading...'}</h3>
        <button className="btn-secondary btn-sm" onClick={onClose}>Close</button>
      </div>
      {error && <p className="error">{error}</p>}
      {detail && (
        <>
          <p className="text-muted">
            Role: <strong>{detail.user.role}</strong>
            {!isSelf && (
              confirming ? (
                <>
                  {' '}
                  <button className="btn-primary btn-sm" onClick={() => void toggleRole()}>
                    Confirm: make {detail.user.role === 'admin' ? 'user' : 'admin'}
                  </button>{' '}
                  <button className="btn-secondary btn-sm" onClick={() => setConfirming(false)}>Cancel</button>
                </>
              ) : (
                <>
                  {' '}
                  <button className="btn-secondary btn-sm" onClick={() => setConfirming(true)}>
                    {detail.user.role === 'admin' ? 'Remove admin' : 'Make admin'}
                  </button>
                </>
              )
            )}
          </p>

          <h4>Usage by API</h4>
          {detail.by_api.map((u) => (
            <div key={u.slug} className="usage-row">
              <span>{u.name}</span>
              <progress value={u.used_this_month} max={u.monthly_quota} />
              <span className="text-muted">
                {u.used_this_month.toLocaleString()} / {u.monthly_quota.toLocaleString()} this month · {u.requests} requests, {u.errors} errors
              </span>
            </div>
          ))}

          <h4>Requests per day</h4>
          <DailyBars days={detail.by_day} />

          <h4>API keys</h4>
          {detail.keys.length === 0 && <p className="text-muted">No keys.</p>}
          <table className="params">
            <tbody>
              {detail.keys.map((k) => (
                <tr key={k.id}>
                  <td><code>{k.key_prefix}…</code></td>
                  <td>{k.name}</td>
                  <td className="text-muted">{k.last_used_at ? `Used ${new Date(k.last_used_at).toLocaleString()}` : 'Never used'}</td>
                  <td>{k.revoked_at ? <span className="text-muted">Revoked</span> : 'Active'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </section>
  )
}
