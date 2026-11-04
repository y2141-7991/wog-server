import { useCallback, useEffect, useRef, useState } from 'react'
import {
  firstStoredId,
  isPending,
  mergeMessages,
  newClientId,
} from '../components/chat/chatFormat'
import type {
  ChatMessage,
  ChatRole,
  SupportClientEvent,
  SupportMessage,
  SupportMessagesPage,
  SupportServerEvent,
} from '../types'

/** A message the server has not acked after this long is shown as failed, with a retry. */
const ACK_TIMEOUT_MS = 15_000
/** "is typing" disappears by itself if no stop event arrives. */
const TYPING_TTL_MS = 5_000

interface Options {
  /** Which side of the chat this is. */
  role: ChatRole
  selfId: string
  /** The conversation being shown, when known (always for admins). */
  conversationId?: string
  send: (event: SupportClientEvent) => boolean
}

/**
 * The message list of one conversation: optimistic sends with ack / failure /
 * retry, dedupe between REST pages and live events, "typing" and "seen".
 * Both the user widget and the admin inbox use it.
 */
export function useChatThread({ role, selfId, conversationId, send }: Options) {
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [hasMore, setHasMore] = useState(false)
  const [loadingOlder, setLoadingOlder] = useState(false)
  const [peerTyping, setPeerTyping] = useState(false)
  const sentAt = useRef(new Map<string, number>())
  const typingTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  /** Replace the stored messages with a fresh first page (unconfirmed sends stay). */
  const reset = useCallback((page: SupportMessagesPage) => {
    setMessages((cur) => mergeMessages(cur.filter(isPending), page.messages))
    setHasMore(page.has_more)
  }, [])

  const mergeIncoming = useCallback((incoming: SupportMessage[]) => {
    if (incoming.length) setMessages((cur) => mergeMessages(cur, incoming))
  }, [])

  const clear = useCallback(() => {
    setMessages([])
    setHasMore(false)
    setPeerTyping(false)
    sentAt.current.clear()
  }, [])

  const transmit = useCallback(
    (m: ChatMessage) => {
      sentAt.current.set(m.client_msg_id ?? m.id, Date.now())
      send({
        type: 'send_message',
        conversation_id: role === 'admin' ? conversationId : undefined,
        body: m.body,
        client_msg_id: m.client_msg_id ?? m.id,
      })
    },
    [send, role, conversationId],
  )

  const sendMessage = useCallback(
    (body: string) => {
      const clientId = newClientId()
      const local: ChatMessage = {
        id: `local:${clientId}`,
        conversation_id: conversationId ?? '',
        sender_id: selfId,
        sender_role: role,
        body,
        client_msg_id: clientId,
        created_at: new Date().toISOString(),
        read_at: null,
        state: 'sending',
      }
      setMessages((cur) => [...cur, local])
      transmit(local)
    },
    [conversationId, selfId, role, transmit],
  )

  const retry = useCallback(
    (localId: string) => {
      setMessages((cur) =>
        cur.map((m) => (m.id === localId ? { ...m, state: 'sending', error: undefined } : m)),
      )
      const m = messages.find((x) => x.id === localId)
      if (m) transmit(m)
    },
    [messages, transmit],
  )

  const discard = useCallback((localId: string) => {
    setMessages((cur) => cur.filter((m) => m.id !== localId))
  }, [])

  /** After a reconnect: resend everything still waiting for its ack (same ids, so no duplicates). */
  const flush = useCallback(() => {
    for (const m of messages) if (m.state === 'sending') transmit(m)
  }, [messages, transmit])

  const loadOlder = useCallback(
    async (fetchPage: (before: string) => Promise<SupportMessagesPage>) => {
      const before = firstStoredId(messages)
      if (!before || loadingOlder || !hasMore) return
      setLoadingOlder(true)
      try {
        const page = await fetchPage(before)
        setMessages((cur) => mergeMessages(cur, page.messages))
        setHasMore(page.has_more)
      } finally {
        setLoadingOlder(false)
      }
    },
    [messages, loadingOlder, hasMore],
  )

  /** Feed every socket event here; it takes what concerns this thread and ignores the rest. */
  const handleEvent = useCallback(
    (e: SupportServerEvent) => {
      switch (e.type) {
        case 'message':
          if (!conversationId || e.message.conversation_id === conversationId) mergeIncoming([e.message])
          break
        case 'ack':
          sentAt.current.delete(e.client_msg_id)
          setMessages((cur) =>
            cur.map((m) =>
              m.client_msg_id === e.client_msg_id && isPending(m)
                ? { ...m, id: e.message_id, conversation_id: e.conversation_id, created_at: e.created_at, state: undefined, error: undefined }
                : m,
            ),
          )
          break
        case 'error':
          if (e.client_msg_id) {
            sentAt.current.delete(e.client_msg_id)
            setMessages((cur) =>
              cur.map((m) =>
                m.client_msg_id === e.client_msg_id && m.state === 'sending'
                  ? { ...m, state: 'failed', error: e.message }
                  : m,
              ),
            )
          }
          break
        case 'typing':
          if (e.role !== role && (!conversationId || e.conversation_id === conversationId)) {
            clearTimeout(typingTimer.current)
            setPeerTyping(e.is_typing)
            if (e.is_typing) typingTimer.current = setTimeout(() => setPeerTyping(false), TYPING_TTL_MS)
          }
          break
        case 'read':
          if (e.by !== role && (!conversationId || e.conversation_id === conversationId)) {
            setMessages((cur) =>
              cur.map((m) => (m.sender_role === role && !m.read_at && !isPending(m) ? { ...m, read_at: e.at } : m)),
            )
          }
          break
      }
    },
    [conversationId, mergeIncoming, role],
  )

  // Sends that never got an answer become "failed" so the user can retry.
  useEffect(() => {
    const timer = setInterval(() => {
      const now = Date.now()
      setMessages((cur) => {
        let changed = false
        const next = cur.map((m) => {
          const at = m.client_msg_id ? sentAt.current.get(m.client_msg_id) : undefined
          if (m.state === 'sending' && at !== undefined && now - at > ACK_TIMEOUT_MS) {
            changed = true
            return { ...m, state: 'failed' as const, error: 'Not delivered. Check your connection.' }
          }
          return m
        })
        return changed ? next : cur
      })
    }, 3_000)
    return () => {
      clearInterval(timer)
      clearTimeout(typingTimer.current)
    }
  }, [])

  return { messages, hasMore, loadingOlder, peerTyping, reset, mergeIncoming, clear, sendMessage, retry, discard, flush, loadOlder, handleEvent }
}
