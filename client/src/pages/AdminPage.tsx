import { useEffect, useState } from 'react'
import { fetchAuditLog, fetchOverview, fetchUsers } from '../api/admin'
import { AdminUserDetail } from '../components/AdminUserDetail'
import { DailyBars } from '../components/DailyBars'
import { compact, timeAgo } from '../lib/format'
import type { AdminOverview, AdminUsers, AuditLogEntry } from '../types'

type Tab = 'usage' | 'audit'

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <span className="text-muted">{label}</span>
      <strong>{value}</strong>
    </div>
  )
}

export function AdminPage({ currentUserId }: { currentUserId: string }) {
  const [tab, setTab] = useState<Tab>('usage')
  const [days, setDays] = useState(30)
  const [search, setSearch] = useState('')
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(0)
  const [selected, setSelected] = useState<string | null>(null)
  const [reload, setReload] = useState(0)

  const [overview, setOverview] = useState<AdminOverview | null>(null)
  const [users, setUsers] = useState<AdminUsers | null>(null)
  const [audit, setAudit] = useState<AuditLogEntry[]>([])
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    fetchOverview(days)
      .then((o) => !cancelled && setOverview(o))
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : 'Could not load the overview'))
    return () => {
      cancelled = true
    }
  }, [days])

  useEffect(() => {
    let cancelled = false
    fetchUsers(query, days, page)
      .then((u) => !cancelled && setUsers(u))
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : 'Could not load users'))
    return () => {
      cancelled = true
    }
  }, [query, days, page, reload])

  useEffect(() => {
    if (tab !== 'audit') return
    let cancelled = false
    fetchAuditLog()
      .then((a) => !cancelled && setAudit(a))
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : 'Could not load the audit log'))
    return () => {
      cancelled = true
    }
  }, [tab, reload])

  const totals = overview?.totals
  const errorRate = totals && totals.requests > 0 ? `${((totals.errors / totals.requests) * 100).toFixed(1)}%` : '–'
  const lastPage = users ? Math.max(0, Math.ceil(users.total / users.per_page) - 1) : 0

  return (
    <div className="detail">
      <div className="admin-head">
        <h1>Admin</h1>
        <div className="chips">
          <button className={`chip${tab === 'usage' ? ' chip-active' : ''}`} onClick={() => setTab('usage')}>Usage</button>
          <button className={`chip${tab === 'audit' ? ' chip-active' : ''}`} onClick={() => setTab('audit')}>Audit log</button>
        </div>
        {tab === 'usage' && (
          <select value={days} onChange={(e) => { setDays(Number(e.target.value)); setPage(0) }} aria-label="Window">
            <option value={7}>Last 7 days</option>
            <option value={30}>Last 30 days</option>
            <option value={90}>Last 90 days</option>
          </select>
        )}
      </div>
      {error && <p className="error">{error}</p>}

      {tab === 'usage' && (
        <>
          <div className="stats">
            <Stat label="Users" value={totals ? totals.total_users.toLocaleString() : '–'} />
            <Stat label="Active users" value={totals ? totals.active_users.toLocaleString() : '–'} />
            <Stat label="Requests" value={totals ? compact(totals.requests) : '–'} />
            <Stat label="Error rate" value={errorRate} />
            <Stat label="Avg latency" value={totals?.avg_latency_ms != null ? `${totals.avg_latency_ms} ms` : '–'} />
          </div>

          <section className="panel">
            <h3>Requests per day</h3>
            {overview && <DailyBars days={overview.by_day} />}
            <h4>By API</h4>
            <table className="params">
              <tbody>
                {overview?.by_api.map((a) => (
                  <tr key={a.slug}>
                    <td>{a.name}</td>
                    <td>{a.requests.toLocaleString()} requests</td>
                    <td className="text-muted">{a.errors.toLocaleString()} errors</td>
                    <td className="text-muted">{a.users} users</td>
                    <td className="text-muted">{a.avg_latency_ms != null ? `${a.avg_latency_ms} ms` : '–'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>

          {selected && (
            <AdminUserDetail
              key={selected}
              userId={selected}
              days={days}
              isSelf={selected === currentUserId}
              onClose={() => setSelected(null)}
              onRoleChanged={() => setReload((n) => n + 1)}
            />
          )}

          <section className="panel">
            <div className="panel-head">
              <h3>Users{users ? ` (${users.total})` : ''}</h3>
              <form
                className="inline-form"
                onSubmit={(e) => {
                  e.preventDefault()
                  setPage(0)
                  setQuery(search)
                }}
              >
                <input value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Search username or email" spellCheck={false} />
                <button type="submit" className="btn-secondary">Search</button>
              </form>
            </div>
            <table className="params users-table">
              <thead>
                <tr>
                  <th>User</th><th>Role</th><th>Requests</th><th>Errors</th><th>Keys</th><th>Last active</th>
                </tr>
              </thead>
              <tbody>
                {users?.users.map((u) => (
                  <tr key={u.id} className={u.id === selected ? 'row-active' : ''} onClick={() => setSelected(u.id)}>
                    <td>{u.username}<br /><span className="text-muted">{u.email}</span></td>
                    <td>{u.role}</td>
                    <td>{u.requests.toLocaleString()}</td>
                    <td>{u.errors.toLocaleString()}</td>
                    <td>{u.active_keys}</td>
                    <td className="text-muted">{u.last_active ? timeAgo(u.last_active) : 'Never'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {users && users.users.length === 0 && <p className="text-muted">No users match.</p>}
            {users && users.total > users.per_page && (
              <div className="pager">
                <button className="btn-secondary btn-sm" disabled={page === 0} onClick={() => setPage(page - 1)}>Previous</button>
                <span className="text-muted">Page {page + 1} of {lastPage + 1}</span>
                <button className="btn-secondary btn-sm" disabled={page >= lastPage} onClick={() => setPage(page + 1)}>Next</button>
              </div>
            )}
          </section>
        </>
      )}

      {tab === 'audit' && (
        <section className="panel">
          <h3>Recent admin actions</h3>
          {audit.length === 0 && <p className="text-muted">Nothing yet.</p>}
          <table className="params">
            <tbody>
              {audit.map((a) => (
                <tr key={a.id}>
                  <td className="text-muted">{new Date(a.created_at).toLocaleString()}</td>
                  <td>{a.actor_username ?? 'system'}</td>
                  <td><code>{a.action}</code></td>
                  <td>{a.target_username ?? '(deleted user)'}</td>
                  <td className="text-muted">{a.old_value ?? '–'} → {a.new_value ?? '–'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}
    </div>
  )
}
