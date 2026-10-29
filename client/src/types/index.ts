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
