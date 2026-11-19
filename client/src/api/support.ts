import type {
  SupportConversation,
  SupportInbox,
  SupportMessagesPage,
  SupportUserView,
} from '../types'
import { apiFetch } from './http'

const API = import.meta.env.VITE_API_URL || ''
const BASE = `${API}/api/v1/support`

async function request<T>(path: string, init: RequestInit, failure: string): Promise<T> {
  const res = await apiFetch(`${BASE}${path}`, init)
  const body = await res.json().catch(() => ({}))
  if (!res.ok) throw new Error(body.message || failure)
  return body as T
}

interface PageOptions {
  before?: string
  after?: string
  limit?: number
}

function pageQuery({ before, after, limit }: PageOptions): string {
  const params = new URLSearchParams()
  if (before) params.set('before', before)
  if (after) params.set('after', after)
  if (limit) params.set('limit', String(limit))
  const q = params.toString()
  return q ? `?${q}` : ''
}

/** The signed-in user's own conversation (open, else the latest closed) and a page of it. */
export const fetchMyConversation = (options: PageOptions = {}) =>
  request<SupportUserView>(`/conversation${pageQuery(options)}`, {}, 'Could not load the chat')

export const fetchMessages = (conversationId: string, options: PageOptions = {}) =>
  request<SupportMessagesPage>(
    `/conversations/${conversationId}/messages${pageQuery(options)}`,
    {},
    'Could not load messages',
  )

export const fetchInbox = (status: 'open' | 'closed' | 'all', q: string, page: number) =>
  request<SupportInbox>(
    `/conversations?${new URLSearchParams({ status, q, page: String(page) })}`,
    {},
    'Could not load the inbox',
  )

export const fetchConversation = (id: string) =>
  request<SupportConversation>(`/conversations/${id}`, {}, 'Could not load the conversation')

export const closeConversation = (id: string) =>
  request<SupportConversation>(`/conversations/${id}/close`, { method: 'POST' }, 'Could not close the conversation')

export const reopenConversation = (id: string) =>
  request<SupportConversation>(`/conversations/${id}/reopen`, { method: 'POST' }, 'Could not reopen the conversation')

/**
 * The chat socket's URL. A browser cannot set an Authorization header on a WebSocket,
 * so a Bearer call first buys a single-use ticket (good for 30 seconds) and the URL
 * carries that, never the access token. Every connection, including each reconnect,
 * needs its own ticket.
 */
export async function supportSocketUrl(): Promise<string> {
  const { ticket } = await request<{ ticket: string }>('/ticket', { method: 'POST' }, 'Could not open the chat')
  const base = API ? new URL(API, window.location.href) : new URL(window.location.href)
  const scheme = base.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${scheme}//${base.host}/api/v1/support/ws?ticket=${encodeURIComponent(ticket)}`
}
