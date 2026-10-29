import type {
  AdminOverview,
  AdminUserDetail,
  AdminUsers,
  AuditLogEntry,
  User,
} from '../types'

const API = import.meta.env.VITE_API_URL || ''
const BASE = `${API}/api/v1/admin`

async function request<T>(path: string, init: RequestInit, failure: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`, { credentials: 'include', ...init })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) throw new Error(body.message || failure)
  return body as T
}

export const fetchOverview = (days: number) =>
  request<AdminOverview>(`/overview?days=${days}`, {}, 'Could not load the overview')

export const fetchUsers = (q: string, days: number, page: number) =>
  request<AdminUsers>(
    `/users?${new URLSearchParams({ q, days: String(days), page: String(page) })}`,
    {},
    'Could not load users',
  )

export const fetchUserDetail = (id: string, days: number) =>
  request<AdminUserDetail>(`/users/${id}/usage?days=${days}`, {}, 'Could not load this user')

export const setRole = (id: string, role: 'user' | 'admin') =>
  request<User>(
    `/users/${id}/role`,
    {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ role }),
    },
    'Could not change the role',
  )

export const fetchAuditLog = () =>
  request<AuditLogEntry[]>('/audit-log', {}, 'Could not load the audit log')
