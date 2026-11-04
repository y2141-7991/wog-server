import { useCallback, useEffect, useState } from 'react'
import { createOrder, fetchOrder, fetchOrders, fetchPlans, fetchSubscription } from '../api/billing'
import { formatVnd } from '../lib/format'
import type { Order, OrderPayment, Plan, Subscription } from '../types'
import { PaymentModal } from './PaymentModal'

const RECOMMENDED = 'pro'
const RENEWAL_WINDOW_DAYS = 7

function date(iso: string) {
  return new Date(iso).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' })
}

function statusClass(status: Order['status']) {
  if (status === 'paid') return 'status-ok'
  if (status === 'pending') return 'status-pending'
  return 'status-bad'
}

type ButtonState = { label: string; disabled: boolean; hint?: string }

/** What the button of `plan` says, given what the user has now. */
function buttonFor(plan: Plan, subscription: Subscription | null, plans: Plan[]): ButtonState {
  const current = subscription?.plan
  if (!current) return { label: 'Choose', disabled: false }
  const currentPlan = plans.find((p) => p.slug === current.slug)
  if (plan.slug === current.slug) {
    const daysLeft = (new Date(current.current_period_end).getTime() - Date.now()) / 86_400_000
    return daysLeft <= RENEWAL_WINDOW_DAYS
      ? { label: 'Renew', disabled: false }
      : { label: 'Current plan', disabled: true }
  }
  if (currentPlan && plan.sort_order > currentPlan.sort_order) return { label: 'Upgrade', disabled: false }
  return { label: 'Included in your plan', disabled: true, hint: 'You already have a higher plan' }
}

export function SubscriptionSection() {
  const [plans, setPlans] = useState<Plan[] | null>(null)
  const [subscription, setSubscription] = useState<Subscription | null>(null)
  const [orders, setOrders] = useState<Order[] | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [payment, setPayment] = useState<OrderPayment | null>(null)

  const refresh = useCallback(async () => {
    const [p, s, o] = await Promise.all([fetchPlans(), fetchSubscription(), fetchOrders()])
    setPlans(p)
    setSubscription(s)
    setOrders(o)
  }, [])

  useEffect(() => {
    let cancelled = false
    refresh().catch((e) => !cancelled && setLoadError(e instanceof Error ? e.message : 'Could not load billing'))
    return () => {
      cancelled = true
    }
  }, [refresh])

  async function pay(slug: Plan['slug']) {
    setBusy(slug)
    setActionError(null)
    try {
      setPayment(await createOrder(slug))
      void refresh().catch(() => undefined)
    } catch (e) {
      setActionError(e instanceof Error ? e.message : 'Could not start the payment')
    } finally {
      setBusy(null)
    }
  }

  async function resume(id: string) {
    setActionError(null)
    try {
      setPayment(await fetchOrder(id))
    } catch (e) {
      setActionError(e instanceof Error ? e.message : 'Could not open this order')
    }
  }

  const closeModal = useCallback(() => {
    setPayment(null)
    void refresh().catch(() => undefined)
  }, [refresh])

  const refreshQuietly = useCallback(() => {
    void refresh().catch(() => undefined)
  }, [refresh])

  if (loadError) {
    return (
      <section className="panel sub-section">
        <h3>Subscription</h3>
        <p className="error">{loadError}</p>
      </section>
    )
  }

  if (!plans || !subscription || !orders) {
    return (
      <section className="panel sub-section">
        <h3>Subscription</h3>
        <p className="text-muted">Loading…</p>
      </section>
    )
  }

  const current = subscription.plan
  const quota = subscription.usage.quota
  const ratio = quota ? Math.min(1, subscription.usage.used / quota) : 0

  return (
    <section className="sub-section">
      <h2>Subscription</h2>
      {actionError && <p className="error">{actionError}</p>}

      <div className="panel plan-current">
        <div className="plan-current-head">
          <div>
            <span className="text-muted">Current plan</span>
            <h3>{current ? current.name : 'Free'}</h3>
          </div>
          {current && (
            <div className="plan-current-end">
              <span className="text-muted">Renews or ends</span>
              <strong>{date(current.current_period_end)}</strong>
            </div>
          )}
        </div>
        {current && quota ? (
          <>
            <progress value={subscription.usage.used} max={quota} aria-label="Requests used" className={ratio >= 0.9 ? 'progress-hot' : ''} />
            <p className="text-muted">
              {subscription.usage.used.toLocaleString()} of {quota.toLocaleString()} requests used since {date(subscription.usage.period_start)}
              {' · '}{current.rate_limit_per_minute} requests per minute
            </p>
          </>
        ) : (
          <p className="text-muted">
            The free plan uses each API's own monthly quota and rate limit.
            {subscription.usage.used > 0 && ` ${subscription.usage.used.toLocaleString()} requests this month.`}{' '}
            Pick a plan below for a larger shared quota across all APIs.
          </p>
        )}
      </div>

      <div className="plan-grid">
        {plans.map((plan) => {
          const state = buttonFor(plan, subscription, plans)
          const isCurrent = current?.slug === plan.slug
          return (
            <div key={plan.slug} className={`plan-card${plan.slug === RECOMMENDED ? ' plan-card-featured' : ''}${isCurrent ? ' plan-card-current' : ''}`}>
              {plan.slug === RECOMMENDED && <span className="plan-badge">Recommended</span>}
              <h3>{plan.name}</h3>
              <p className="plan-price">
                <strong>{formatVnd(plan.price_vnd)}</strong>
                <span className="text-muted"> / {plan.period_days} days</span>
              </p>
              <ul className="plan-features">
                {plan.features.map((f) => <li key={f}>{f}</li>)}
              </ul>
              <button
                className={plan.slug === RECOMMENDED && !state.disabled ? 'btn-primary' : 'btn-secondary'}
                disabled={state.disabled || busy !== null}
                title={state.hint}
                onClick={() => void pay(plan.slug)}
              >
                {busy === plan.slug ? 'Starting…' : state.label}
              </button>
            </div>
          )
        })}
      </div>

      <div className="panel">
        <h3>Payment history</h3>
        {orders.length === 0 ? (
          <p className="text-muted">No payments yet.</p>
        ) : (
          <div className="table-scroll">
            <table className="params orders-table">
              <thead>
                <tr><th>Date</th><th>Plan</th><th>Amount</th><th>Code</th><th>Status</th><th /></tr>
              </thead>
              <tbody>
                {orders.map((o) => (
                  <tr key={o.id}>
                    <td className="text-muted">{new Date(o.created_at).toLocaleString()}</td>
                    <td>{o.plan_name}</td>
                    <td>{formatVnd(o.amount_vnd)}</td>
                    <td><code>{o.payment_code}</code></td>
                    <td className={statusClass(o.status)}>{o.status}</td>
                    <td>
                      {o.status === 'pending' && (
                        <button className="btn-secondary btn-sm" onClick={() => void resume(o.id)}>Pay</button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {payment && (
        <PaymentModal
          key={payment.id}
          order={payment}
          onClose={closeModal}
          onPaid={refreshQuietly}
          onChanged={refreshQuietly}
          onRetry={() => void pay(payment.plan)}
        />
      )}
    </section>
  )
}
