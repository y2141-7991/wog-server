export interface User {
  id: string
  username: string
  email: string
  avatar_url: string | null
  is_online: boolean
  email_verified: boolean
  role: 'user' | 'admin'
  must_change_password: boolean
}

export interface ApiParam {
  name: string
  type: string
  required: boolean
  description: string
  example: string
}

export interface MarketplaceApi {
  id: string
  slug: string
  name: string
  description: string
  provider: string
  category: string
  gateway_path: string
  params: ApiParam[]
  example_response: unknown | null
  rate_limit_per_minute: number
  monthly_quota: number
  updated_at: string
  avg_latency_ms: number | null
  success_rate: number | null
  total_calls: number
}

export interface ApiKey {
  id: string
  name: string
  key_prefix: string
  created_at: string
  last_used_at: string | null
  revoked_at: string | null
}

export interface CreateApiKeyResponse {
  key: ApiKey
  secret: string
}

export interface UsageByApi {
  slug: string
  name: string
  monthly_quota: number
  requests: number
  errors: number
  used_this_month: number
}

export interface UsageByDay {
  day: string
  requests: number
}

export interface UsageResponse {
  days: number
  by_api: UsageByApi[]
  by_day: UsageByDay[]
}

export interface AdminTotals {
  total_users: number
  active_users: number
  requests: number
  errors: number
  avg_latency_ms: number | null
}

export interface AdminApiTraffic {
  slug: string
  name: string
  requests: number
  errors: number
  avg_latency_ms: number | null
  users: number
}

export interface AdminOverview {
  days: number
  totals: AdminTotals
  by_api: AdminApiTraffic[]
  by_day: UsageByDay[]
}

export interface AdminUserRow {
  id: string
  username: string
  email: string
  role: 'user' | 'admin'
  created_at: string
  requests: number
  errors: number
  last_active: string | null
  active_keys: number
}

export interface AdminUsers {
  days: number
  page: number
  per_page: number
  total: number
  users: AdminUserRow[]
}

export interface AdminUserDetail {
  user: User
  days: number
  by_api: UsageByApi[]
  by_day: UsageByDay[]
  keys: ApiKey[]
}

export interface AuditLogEntry {
  id: string
  actor_username: string | null
  target_user_id: string | null
  target_username: string | null
  action: string
  old_value: string | null
  new_value: string | null
  created_at: string
}

// ───────── Billing ─────────

export type PlanSlug = 'basic' | 'pro' | 'ultra'

export interface Plan {
  slug: PlanSlug
  name: string
  /** Integer VND. */
  price_vnd: number
  period_days: number
  monthly_quota: number
  rate_limit_per_minute: number
  features: string[]
  sort_order: number
}

export interface SubscriptionPlan {
  slug: PlanSlug
  name: string
  monthly_quota: number
  rate_limit_per_minute: number
  current_period_start: string
  current_period_end: string
}

export interface Subscription {
  status: 'active' | 'free'
  plan: SubscriptionPlan | null
  usage: {
    used: number
    /** Absent on the free tier, where every API has its own quota. */
    quota: number | null
    period_start: string
  }
}

export type OrderStatus = 'pending' | 'paid' | 'expired' | 'canceled' | 'failed'

export interface Order {
  id: string
  plan: PlanSlug
  plan_name: string
  amount_vnd: number
  status: OrderStatus
  payment_code: string
  paid_at: string | null
  expires_at: string
  created_at: string
}

/** An order plus everything needed to pay it. */
export interface OrderPayment {
  id: string
  plan: PlanSlug
  plan_name: string
  amount_vnd: number
  status: OrderStatus
  expires_at: string
  created_at: string
  transfer_content: string
  bank_name: string
  account_number: string
  account_holder: string
  qr_url: string
}

export interface OrderStatusResponse {
  id: string
  status: OrderStatus
  paid_at: string | null
}

export interface AdminOrder extends Order {
  username: string | null
  sepay_transaction_id: number | null
  paid_amount_vnd: number | null
}

export interface AdminOrders {
  orders: AdminOrder[]
  total: number
  page: number
  per_page: number
}

export interface PaymentEvent {
  id: string
  source: 'webhook' | 'reconcile'
  sepay_transaction_id: number | null
  transfer_type: string | null
  amount_vnd: number | null
  payment_code: string | null
  content: string | null
  matched_order_id: string | null
  outcome: string
  note: string | null
  needs_review: boolean
  created_at: string
}

export interface AdminPaymentEvents {
  events: PaymentEvent[]
  total: number
  page: number
  per_page: number
}
export * from './support'
