import { useCallback, useEffect, useRef, useState } from 'react'
import { fetchMessages, fetchMyConversation } from '../api/support'
import { useChatThread } from '../hooks/useChatThread'
import { useSupportSocket } from '../hooks/useSupportSocket'
import type { SupportClientEvent, SupportConversation, SupportServerEvent, SupportUserView, User } from '../types'
import { ChatComposer } from './chat/ChatComposer'
import { ChatThread } from './chat/ChatThread'
import { lastStoredId } from './chat/chatFormat'

const OPEN_KEY = 'wog.support.open'
const MAX_CATCH_UP_PAGES = 20

function readOpen(): boolean {
  try {
    return sessionStorage.getItem(OPEN_KEY) === '1'
  } catch {
    return false
  }
}

function writeOpen(open: boolean) {
  try {
    sessionStorage.setItem(OPEN_KEY, open ? '1' : '0')
  } catch {
    // storage can be unavailable; the panel just starts closed next time
  }
}

interface Props {
  user: User
  /** The server says the session is gone (e.g. expired): treat the user as signed out. */
  onAuthLost?: () => void
}

/**
 * The customer's chat bubble. Mounted only while a verified, non-admin user is
 * signed in, so it never shows on the login or verification pages and goes
 * away (closing its socket) on logout.
 *
 * Connection policy: one REST call when it mounts gives the unread count. The
 * WebSocket is opened right away only if the user already has an open
 * conversation (replies can arrive any time); otherwise it waits for the first
 * time the panel is opened, so most users never hold an idle socket.
 */
export function SupportChatWidget({ user, onAuthLost }: Props) {
  const [open, setOpen] = useState(readOpen)
  const [conversation, setConversation] = useState<SupportConversation | null>(null)
  const [load, setLoad] = useState<{ state: 'loading' | 'ready' | 'error'; message?: string }>({ state: 'loading' })
  const [unread, setUnread] = useState(0)
  const [wantSocket, setWantSocket] = useState(false)
  const [attempt, setAttempt] = useState(0)

  const inputRef = useRef<HTMLTextAreaElement>(null)
  const launcherRef = useRef<HTMLButtonElement>(null)
  const sendRef = useRef<(e: SupportClientEvent) => boolean>(() => false)
  const proxySend = useCallback((e: SupportClientEvent) => sendRef.current(e), [])

  const thread = useChatThread({ role: 'user', selfId: user.id, conversationId: conversation?.id, send: proxySend })
  const { reset, mergeIncoming, flush, handleEvent, messages } = thread

  const viewing = () => open && document.visibilityState === 'visible' && document.hasFocus()

  const applyView = useCallback(
    (view: SupportUserView, forPanel: boolean) => {
      setConversation(view.conversation)
      reset(view)
      setUnread(forPanel ? 0 : (view.conversation?.user_unread_count ?? 0))
      setWantSocket((w) => w || view.conversation?.status === 'open')
    },
    [reset],
  )

  // First load (and "Try again").
  useEffect(() => {
    let cancelled = false
    fetchMyConversation({ limit: 50 })
      .then((view) => {
        if (cancelled) return
        applyView(view, readOpen())
        setLoad({ state: 'ready' })
      })
      .catch((e) => !cancelled && setLoad({ state: 'error', message: e instanceof Error ? e.message : 'Could not load the chat' }))
    return () => {
      cancelled = true
    }
  }, [attempt, applyView])

  const markRead = useCallback(() => {
    proxySend({ type: 'mark_read' })
    setUnread(0)
  }, [proxySend])

  /** Fetch whatever arrived while the socket was down, then resend unacked messages. */
  const catchUp = useCallback(async () => {
    try {
      const after = lastStoredId(messages)
      if (!conversation || !after) {
        applyView(await fetchMyConversation({ limit: 50 }), viewing())
      } else {
        let cursor: string | undefined = after
        for (let i = 0; i < MAX_CATCH_UP_PAGES && cursor; i++) {
          const page = await fetchMessages(conversation.id, { after: cursor, limit: 100 })
          mergeIncoming(page.messages)
          cursor = page.has_more ? page.messages[page.messages.length - 1]?.id : undefined
        }
      }
    } catch {
      // The next reconnect or the next event brings it up to date.
    }
    flush()
    if (viewing()) markRead()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [messages, conversation, applyView, mergeIncoming, flush, markRead, open])

  const onEvent = (e: SupportServerEvent) => {
    handleEvent(e)
    if (e.type === 'message' && e.message.sender_role === 'admin') {
      if (viewing()) markRead()
      else setUnread((n) => n + 1)
    } else if (e.type === 'conversation_updated') {
      setConversation(e.conversation)
      if (!viewing()) setUnread(e.conversation.user_unread_count)
    }
  }

  const socket = useSupportSocket({
    enabled: wantSocket || open,
    onEvent,
    onReady: () => void catchUp(),
    onAuthLost,
  })
  useEffect(() => {
    sendRef.current = socket.send
  })

  // Mark as read when the window regains focus while the panel is open.
  useEffect(() => {
    if (!open) return
    const onFocus = () => proxySend({ type: 'mark_read' })
    window.addEventListener('focus', onFocus)
    return () => window.removeEventListener('focus', onFocus)
  }, [open, proxySend])

  function openPanel() {
    writeOpen(true)
    setOpen(true)
    setUnread(0)
    proxySend({ type: 'mark_read' })
    setTimeout(() => inputRef.current?.focus(), 0)
  }

  function closePanel() {
    writeOpen(false)
    setOpen(false)
    setTimeout(() => launcherRef.current?.focus(), 0)
  }

  // If the panel was restored open from sessionStorage, focus its input once.
  useEffect(() => {
    if (open) inputRef.current?.focus()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const status = socket.status
  const connected = status === 'open'
  const statusText = connected ? 'Connected' : status === 'offline' ? 'Offline' : status === 'idle' ? 'Not connected' : 'Connecting…'
  const closed = conversation?.status === 'closed'

  const typingLabel = thread.peerTyping ? 'Support is typing…' : null

  return (
    <div className="chat-widget">
      {open && (
        <section
          id="support-chat-panel"
          className="chat-panel"
          role="dialog"
          aria-label="Support chat"
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              e.stopPropagation()
              closePanel()
            }
          }}
        >
          <header className="chat-header">
            <h2>Support</h2>
            <span className={`chat-dot chat-dot-${connected ? 'ok' : status === 'offline' ? 'bad' : 'wait'}`} role="img" aria-label={statusText} title={statusText} />
            <span className="chat-status">{statusText}</span>
            <button className="chat-x" onClick={closePanel} aria-label="Close support chat">×</button>
          </header>

          {load.state === 'loading' && <p className="chat-note">Loading…</p>}
          {load.state === 'error' && (
            <div className="chat-note">
              <p className="error">{load.message}</p>
              <button className="btn-secondary btn-sm" onClick={() => { setLoad({ state: 'loading' }); setAttempt((a) => a + 1) }}>Try again</button>
            </div>
          )}
          {load.state === 'ready' && (
            <>
              {(status === 'reconnecting' || status === 'offline') && (
                <p className="chat-banner" role="status">
                  {status === 'offline' ? 'You are offline.' : 'Reconnecting…'} Messages you send now go out when the connection is back.
                </p>
              )}
              {closed && (
                <p className="chat-banner" role="status">This conversation was closed. Send a message to start it again.</p>
              )}
              <ChatThread
                messages={thread.messages}
                selfRole="user"
                hasMore={thread.hasMore}
                loadingOlder={thread.loadingOlder}
                onLoadOlder={() => conversation && void thread.loadOlder((before) => fetchMessages(conversation.id, { before, limit: 30 }))}
                onRetry={(id) => { setWantSocket(true); thread.retry(id) }}
                onDiscard={thread.discard}
                typingLabel={typingLabel}
                threadKey={conversation?.id ?? 'new'}
                label="Conversation with support"
                empty={
                  <>
                    <strong>Hi {user.username}, how can we help?</strong>
                    <span>Ask about the API Hub, your plan or billing. Our replies will show up right here.</span>
                  </>
                }
              />
              <ChatComposer
                inputRef={inputRef}
                placeholder="Write a message…"
                onSend={(body) => { setWantSocket(true); thread.sendMessage(body) }}
                onTyping={(on) => proxySend({ type: 'typing', is_typing: on })}
              />
            </>
          )}
        </section>
      )}
      <button
        ref={launcherRef}
        className="chat-launcher"
        onClick={() => (open ? closePanel() : openPanel())}
        aria-expanded={open}
        aria-controls="support-chat-panel"
        aria-label={unread > 0 ? `Support chat, ${unread} unread` : 'Support chat'}
      >
        <svg viewBox="0 0 24 24" width="24" height="24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M21 12a8 8 0 0 1-11.6 7.1L3 21l1.9-5.4A8 8 0 1 1 21 12z" />
        </svg>
        {unread > 0 && <span className="chat-badge">{unread > 9 ? '9+' : unread}</span>}
      </button>
    </div>
  )
}
