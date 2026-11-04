import { useEffect, useState } from 'react'
import { fetchAdminEvents, fetchAdminOrders } from '../api/billing'
import { formatVnd } from '../lib/format'
import type { AdminOrders, AdminPaymentEvents } from '../types'

type View = 'orders' | 'events'

const ORDER_FILTERS = ['', 'pending', 'paid', 'expired', 'canceled'] as const
const EVENT_FILTERS = ['', 'activated', 'underpaid', 'unmatched', 'duplicate', 'ignored', 'already_paid', 'canceled_order', 'not_applied'] as const

function Pager({ page, total, perPage, onPage }: { page: number; total: number; perPage: number; onPage: (p: number) => void }) {
  const last = Math.max(1, Math.ceil(total / perPage))
  if (total <= perPage) return null
  return (
    <div className="pager">
      <button className="btn-secondary btn-sm" disabled={page <= 1} onClick={() => onPage(page - 1)}>Previous</button>
      <span className="text-muted">Page {page} of {last}</span>
      <button className="btn-secondary btn-sm" disabled={page >= last} onClick={() => onPage(page + 1)}>Next</button>
    </div>
  )
}

function outcomeClass(outcome: string, needsReview: boolean) {
  if (needsReview) return 'status-bad'
  return outcome === 'activated' ? 'status-ok' : 'text-muted'
}

export function AdminPayments() {
  const [view, setView] = useState<View>('orders')
  const [filter, setFilter] = useState('')
  const [reviewOnly, setReviewOnly] = useState(false)
  const [page, setPage] = useState(1)
  const [orders, setOrders] = useState<AdminOrders | null>(null)
  const [events, setEvents] = useState<AdminPaymentEvents | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [reload, setReload] = useState(0)

  useEffect(() => {
    let cancelled = false
    const run =
      view === 'orders'
        ? fetchAdminOrders(filter, page).then((o) => !cancelled && (setOrders(o), setError(null)))
        : fetchAdminEvents(filter, reviewOnly, page).then((ev) => !cancelled && (setEvents(ev), setError(null)))
    run.catch((e) => !cancelled && setError(e instanceof Error ? e.message : 'Could not load payments'))
    return () => {
      cancelled = true
    }
  }, [view, filter, reviewOnly, page, reload])

  function switchView(next: View) {
    setView(next)
    setFilter('')
    setReviewOnly(false)
    setPage(1)
  }

  const filters = view === 'orders' ? ORDER_FILTERS : EVENT_FILTERS

  return (
    <section className="panel">
      <div className="panel-head">
        <h3>Payments</h3>
        <button className="btn-secondary btn-sm" onClick={() => setReload((n) => n + 1)}>Refresh</button>
      </div>

      <div className="chips chips-tight">
        <button className={`chip${view === 'orders' ? ' chip-active' : ''}`} onClick={() => switchView('orders')}>Orders</button>
        <button className={`chip${view === 'events' ? ' chip-active' : ''}`} onClick={() => switchView('events')}>Notifications</button>
      </div>

      <div className="pay-filters">
        <select
          value={filter}
          onChange={(e) => { setFilter(e.target.value); setPage(1) }}
          aria-label={view === 'orders' ? 'Order status' : 'Outcome'}
        >
          {filters.map((f) => (
            <option key={f} value={f}>{f === '' ? (view === 'orders' ? 'All statuses' : 'All outcomes') : f.replace('_', ' ')}</option>
          ))}
        </select>
        {view === 'events' && (
          <label className="check">
            <input type="checkbox" checked={reviewOnly} onChange={(e) => { setReviewOnly(e.target.checked); setPage(1) }} />
            Needs review only
          </label>
        )}
      </div>

      {error && <p className="error">{error}</p>}

      {view === 'orders' && (
        <>
          {orders === null ? (
            <p className="text-muted">Loading…</p>
          ) : orders.orders.length === 0 ? (
            <p className="text-muted">No orders match.</p>
          ) : (
            <div className="table-scroll">
              <table className="params orders-table">
                <thead>
                  <tr><th>Created</th><th>User</th><th>Plan</th><th>Amount</th><th>Code</th><th>Status</th><th>Paid</th><th>SePay id</th></tr>
                </thead>
                <tbody>
                  {orders.orders.map((o) => (
                    <tr key={o.id}>
                      <td className="text-muted">{new Date(o.created_at).toLocaleString()}</td>
                      <td>{o.username ?? '(deleted)'}</td>
                      <td>{o.plan_name}</td>
                      <td>{formatVnd(o.amount_vnd)}</td>
                      <td><code>{o.payment_code}</code></td>
                      <td className={o.status === 'paid' ? 'status-ok' : o.status === 'pending' ? 'status-pending' : 'text-muted'}>{o.status}</td>
                      <td>{o.paid_amount_vnd != null ? formatVnd(o.paid_amount_vnd) : '–'}{o.paid_at && <><br /><span className="text-muted">{new Date(o.paid_at).toLocaleString()}</span></>}</td>
                      <td className="text-muted">{o.sepay_transaction_id ?? '–'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {orders && <Pager page={page} total={orders.total} perPage={orders.per_page} onPage={setPage} />}
        </>
      )}

      {view === 'events' && (
        <>
          {events === null ? (
            <p className="text-muted">Loading…</p>
          ) : events.events.length === 0 ? (
            <p className="text-muted">No notifications match.</p>
          ) : (
            <div className="table-scroll">
              <table className="params orders-table">
                <thead>
                  <tr><th>Received</th><th>Source</th><th>SePay id</th><th>Amount</th><th>Content</th><th>Outcome</th></tr>
                </thead>
                <tbody>
                  {events.events.map((ev) => (
                    <tr key={ev.id} className={ev.needs_review ? 'row-flagged' : ''}>
                      <td className="text-muted">{new Date(ev.created_at).toLocaleString()}</td>
                      <td>{ev.source}{ev.transfer_type === 'out' && <span className="text-muted"> (out)</span>}</td>
                      <td className="text-muted">{ev.sepay_transaction_id ?? '–'}</td>
                      <td>{ev.amount_vnd != null ? formatVnd(ev.amount_vnd) : '–'}</td>
                      <td className="event-content">{ev.content ?? '–'}{ev.payment_code && <><br /><code>{ev.payment_code}</code></>}</td>
                      <td className={outcomeClass(ev.outcome, ev.needs_review)}>
                        {ev.outcome.replace('_', ' ')}
                        {ev.needs_review && <span className="flag"> needs review</span>}
                        {ev.note && <><br /><span className="text-muted">{ev.note}</span></>}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {events && <Pager page={page} total={events.total} perPage={events.per_page} onPage={setPage} />}
        </>
      )}
    </section>
  )
}
