export type ChatRole = 'user' | 'admin'
export type ConversationStatus = 'open' | 'closed'

export interface SupportMessage {
  id: string
  conversation_id: string
  sender_id: string
  sender_role: ChatRole
  body: string
  client_msg_id: string | null
  created_at: string
  read_at: string | null
}

/** A message as the UI holds it: possibly not yet stored by the server. */
export interface ChatMessage extends SupportMessage {
  /** `sending` until the server acks, `failed` when it refused or never answered. */
  state?: 'sending' | 'failed'
  error?: string
}

/** A conversation joined with its user: one inbox row. */
export interface SupportConversation {
  id: string
  user_id: string
  status: ConversationStatus
  subject: string | null
  last_message_at: string
  last_message_preview: string
  user_unread_count: number
  admin_unread_count: number
  assigned_admin_id: string | null
  created_at: string
  closed_at: string | null
  username: string
  email: string
  avatar_url: string | null
  plan_name: string | null
  online: boolean
}

export interface SupportMessagesPage {
  messages: SupportMessage[]
  has_more: boolean
}

export interface SupportUserView extends SupportMessagesPage {
  conversation: SupportConversation | null
}

export interface SupportInbox {
  conversations: SupportConversation[]
  page: number
  per_page: number
  total: number
  total_unread: number
}

// ── WebSocket protocol (see "Support chat" in the README) ──

export type SupportClientEvent =
  | { type: 'send_message'; conversation_id?: string; body: string; client_msg_id: string }
  | { type: 'typing'; conversation_id?: string; is_typing: boolean }
  | { type: 'mark_read'; conversation_id?: string }
  | { type: 'ping' }
  | { type: 'join_conversation'; conversation_id: string }
  | { type: 'leave_conversation'; conversation_id: string }
  | { type: 'close_conversation'; conversation_id: string }
  | { type: 'reopen_conversation'; conversation_id: string }
  | { type: 'assign_to_me'; conversation_id: string }

export type SupportServerEvent =
  | { type: 'ready'; user_id: string; role: ChatRole; conversation_id: string | null }
  | { type: 'message'; message: SupportMessage }
  | { type: 'ack'; client_msg_id: string; message_id: string; conversation_id: string; created_at: string; duplicate: boolean }
  | { type: 'typing'; conversation_id: string; user_id: string; role: ChatRole; is_typing: boolean }
  | { type: 'read'; conversation_id: string; by: ChatRole; at: string }
  | { type: 'conversation_updated'; conversation: SupportConversation; total_unread: number | null }
  | { type: 'presence'; user_id: string; online: boolean }
  | { type: 'error'; code: string; message: string; client_msg_id: string | null }
  | { type: 'pong' }

export type SocketStatus = 'idle' | 'connecting' | 'open' | 'reconnecting' | 'offline'
