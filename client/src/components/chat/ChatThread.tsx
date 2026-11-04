import { useLayoutEffect, useRef } from 'react'
import type { ReactNode } from 'react'
import type { ChatMessage, ChatRole } from '../../types'
import { clockTime, dayKey, dayLabel, isPending } from './chatFormat'

interface Props {
  messages: ChatMessage[]
  /** Which side is looking: their own messages sit on the right. */
  selfRole: ChatRole
  hasMore: boolean
  loadingOlder: boolean
  onLoadOlder: () => void
  onRetry: (localId: string) => void
  onDiscard: (localId: string) => void
  /** E.g. "Support is typing…"; hidden when null. */
  typingLabel: string | null
  /** Shown when there are no messages. */
  empty: ReactNode
  /** Changes when a different conversation is shown, to jump to its newest message. */
  threadKey: string
  label: string
}

const NEAR_BOTTOM_PX = 80
const LOAD_MORE_AT_PX = 40

/**
 * Messages grouped by day. Stays pinned to the newest message unless the
 * reader has scrolled up, and keeps the reading position when older messages
 * are loaded above. Text is rendered as plain text, never as HTML.
 */
export function ChatThread({
  messages,
  selfRole,
  hasMore,
  loadingOlder,
  onLoadOlder,
  onRetry,
  onDiscard,
  typingLabel,
  empty,
  threadKey,
  label,
}: Props) {
  const box = useRef<HTMLDivElement>(null)
  const pinned = useRef(true)
  const anchor = useRef<{ height: number; top: number } | null>(null)
  const lastKey = useRef(threadKey)

  const last = messages[messages.length - 1]
  const lastIsMineAndNew = !!last && last.sender_role === selfRole && isPending(last)

  useLayoutEffect(() => {
    const el = box.current
    if (!el) return
    if (lastKey.current !== threadKey) {
      lastKey.current = threadKey
      pinned.current = true
    }
    if (anchor.current) {
      // Older messages were added above: keep what the reader was looking at.
      el.scrollTop = el.scrollHeight - anchor.current.height + anchor.current.top
      anchor.current = null
    } else if (pinned.current || lastIsMineAndNew) {
      el.scrollTop = el.scrollHeight
    }
  }, [messages, typingLabel, threadKey, lastIsMineAndNew])

  function onScroll() {
    const el = box.current
    if (!el) return
    pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_BOTTOM_PX
    if (el.scrollTop < LOAD_MORE_AT_PX && hasMore && !loadingOlder) {
      anchor.current = { height: el.scrollHeight, top: el.scrollTop }
      onLoadOlder()
    }
  }

  const lastSeenId = [...messages].reverse().find((m) => m.sender_role === selfRole && !isPending(m) && m.read_at)?.id

  return (
    <div ref={box} className="chat-thread" onScroll={onScroll} role="log" aria-live="polite" aria-relevant="additions" aria-label={label}>
      {loadingOlder && <p className="chat-note">Loading earlier messages…</p>}
      {hasMore && !loadingOlder && (
        <button className="btn-secondary btn-sm chat-older" onClick={() => { anchor.current = box.current ? { height: box.current.scrollHeight, top: box.current.scrollTop } : null; onLoadOlder() }}>
          Load earlier messages
        </button>
      )}
      {messages.length === 0 && !loadingOlder && <div className="chat-empty">{empty}</div>}
      {messages.map((m, i) => {
        const separator = i === 0 || dayKey(messages[i - 1].created_at) !== dayKey(m.created_at)
        const mine = m.sender_role === selfRole
        return (
          <div key={m.id}>
            {separator && <div className="chat-day"><span>{dayLabel(m.created_at)}</span></div>}
            <div className={`chat-msg ${mine ? 'chat-msg-own' : 'chat-msg-peer'}${m.state === 'failed' ? ' chat-msg-failed' : ''}`}>
              <p className="chat-body">{m.body}</p>
              <div className="chat-meta">
                {m.state === 'sending' ? (
                  <span>Sending…</span>
                ) : m.state === 'failed' ? (
                  <span className="chat-fail">
                    {m.error || 'Not sent'}{' '}
                    <button className="chat-link" onClick={() => onRetry(m.id)}>Retry</button>{' '}
                    <button className="chat-link" onClick={() => onDiscard(m.id)}>Discard</button>
                  </span>
                ) : (
                  <>
                    <time dateTime={m.created_at}>{clockTime(m.created_at)}</time>
                    {m.id === lastSeenId && <span> · Seen</span>}
                  </>
                )}
              </div>
            </div>
          </div>
        )
      })}
      {typingLabel && <p className="chat-typing" aria-live="polite">{typingLabel}</p>}
    </div>
  )
}
