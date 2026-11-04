import type { ChatMessage, SupportMessage } from '../../types'

export const MAX_BODY_CHARS = 2000

export function newClientId(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) return crypto.randomUUID()
  return `c${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`
}

export const isPending = (m: ChatMessage) => m.state === 'sending' || m.state === 'failed'

function compareServer(a: ChatMessage, b: ChatMessage): number {
  const t = new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
  return t !== 0 ? t : a.id.localeCompare(b.id)
}

/**
 * Fold server messages into the list: dedupe by id, and replace the local
 * copy of a message the server now confirms (matched by `client_msg_id`).
 * Stored messages come first in server order, unconfirmed ones after.
 */
export function mergeMessages(current: ChatMessage[], incoming: SupportMessage[]): ChatMessage[] {
  const byId = new Map<string, ChatMessage>()
  const pending: ChatMessage[] = []
  for (const m of current) {
    if (isPending(m)) pending.push(m)
    else byId.set(m.id, m)
  }
  const confirmed = new Set<string>()
  for (const m of incoming) {
    byId.set(m.id, m)
    if (m.client_msg_id) confirmed.add(m.client_msg_id)
  }
  const stored = [...byId.values()].sort(compareServer)
  const stillPending = pending.filter((m) => !m.client_msg_id || !confirmed.has(m.client_msg_id))
  return [...stored, ...stillPending]
}

export const lastStoredId = (messages: ChatMessage[]): string | undefined =>
  [...messages].reverse().find((m) => !isPending(m))?.id

export const firstStoredId = (messages: ChatMessage[]): string | undefined =>
  messages.find((m) => !isPending(m))?.id

const sameDay = (a: Date, b: Date) =>
  a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate()

/** `Today`, `Yesterday` or a date, in the viewer's time zone. */
export function dayLabel(iso: string, now = new Date()): string {
  const d = new Date(iso)
  if (sameDay(d, now)) return 'Today'
  const yesterday = new Date(now)
  yesterday.setDate(now.getDate() - 1)
  if (sameDay(d, yesterday)) return 'Yesterday'
  return d.toLocaleDateString(undefined, {
    day: 'numeric',
    month: 'short',
    year: d.getFullYear() === now.getFullYear() ? undefined : 'numeric',
  })
}

export const clockTime = (iso: string) =>
  new Date(iso).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })

export const dayKey = (iso: string) => {
  const d = new Date(iso)
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`
}
