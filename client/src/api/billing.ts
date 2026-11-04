import type {
  AdminOrders,
  AdminPaymentEvents,
  Order,
  OrderPayment,
  OrderStatusResponse,
  Plan,
  PlanSlug,
  Subscription,
} from '../types'

const API = import.meta.env.VITE_API_URL || ''
const BASE = `${API}/api/v1/billing`

async function request<T>(path: string, init: RequestInit, failure: string, base = BASE): Promise<T> {
  const res = await fetch(`${base}${path}`, { credentials: 'include', ...init })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) throw new Error(body.message || failure)
  return body as T
}

export const fetchPlans = () => request<Plan[]>('/plans', {}, 'Could not load the plans')

export const fetchSubscription = () =>
  request<Subscription>('/subscription', {}, 'Could not load your subscription')

export const createOrder = (plan: PlanSlug) =>
  request<OrderPayment>(
    '/orders',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ plan }),
    },
    'Could not start the payment',
  )

export const fetchOrders = () => request<Order[]>('/orders', {}, 'Could not load your orders')

export const fetchOrder = (id: string) =>
  request<OrderPayment>(`/orders/${id}`, {}, 'Could not load this order')

export const fetchOrderStatus = (id: string) =>
  request<OrderStatusResponse>(`/orders/${id}/status`, {}, 'Could not check the order')

export const cancelOrder = (id: string) =>
  request<OrderStatusResponse>(`/orders/${id}/cancel`, { method: 'POST' }, 'Could not cancel the order')

/** "I've paid, check now": asks the server to look for the transfer. */
export const checkOrder = (id: string) =>
  request<OrderStatusResponse>(`/orders/${id}/check`, { method: 'POST' }, 'Could not check the payment')

// ───────── Admin ─────────

const ADMIN = `${API}/api/v1/admin/payments`

export const fetchAdminOrders = (filter: string, page: number) =>
  request<AdminOrders>(
    `/orders?${new URLSearchParams({ filter, page: String(page) })}`,
    {},
    'Could not load orders',
    ADMIN,
  )

export const fetchAdminEvents = (filter: string, needsReview: boolean, page: number) =>
  request<AdminPaymentEvents>(
    `/events?${new URLSearchParams({ filter, needs_review: String(needsReview), page: String(page) })}`,
    {},
    'Could not load payment events',
    ADMIN,
  )
