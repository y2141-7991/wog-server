import { useCallback, useEffect, useRef, useState } from 'react'
import {
  closeConversation,
  fetchConversation,
  fetchInbox,
  fetchMessages,
  reopenConversation,
} from '../api/support'
import { ChatComposer } from '../components/chat/ChatComposer'
import { ChatThread } from '../components/chat/ChatThread'
import { lastStoredId } from '../components/chat/chatFormat'
import { useChatThread } from '../hooks/useChatThread'
import type { AdminSupport } from '../hooks/useAdminSupport'
import { timeAgo } from '../lib/format'
import type { SupportConversation, SupportServerEvent } from '../types'

type Filter = 'open' | 'closed' | 'all'

interface Props {
  support: AdminSupport
  currentUserId: string
  onOpenUser: (userId: string) => void
}

function Avatar({ c }: { c: Pick<SupportConversation, 'username' | 'avatar_url'> }) {
  return c.avatar_url ? (
    <img className="inbox-avatar" src={c.avatar_url} alt="" />
  ) : (
    <span className="inbox-avatar inbox-avatar-fallback" aria-hidden="true">{c.username[0]?.toUpperCase()}</span>
  )
}

function matches(row: SupportConversation, filter: Filter, query: string): boolean {
  if (filter !== 'all' && row.status !== filter) return false
  const q = query.trim().toLowerCase()
  return !q || row.username.toLowerCase().includes(q) || row.email.toLowerCase().includes(q)
}

const byRecent = (a: SupportConversation, b: SupportConversation) =>
  new Date(b.last_message_at).getTime() - new Date(a.last_message_at).getTime()

export function AdminSupportPage({ support, currentUserId, onOpenUser }: Props) {
  const [filter, setFilter] = useState<Filter>('open')
  const [search, setSearch] = useState('')
  const [query, setQuery] = useState('')
  const [rows, setRows] = useState<SupportConversation[]>([])
  const [total, setTotal] = useState(0)
  const [page, setPage] = useState(0)
  const [listState, setListState] = useState<{ loading: boolean; error: string | null }>({ loading: true, error: null })
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [selected, setSelected] = useState<SupportConversation | null>(null)
  const [paneError, setPaneError] = useState<string | null>(null)
  const [paneLoading, setPaneLoading] = useState(false)

  const { send, status } = support
  const thread = useChatThread({ role: 'admin', selfId: currentUserId, conversationId: selectedId ?? undefined, send })
  const { reset, mergeIncoming, clear, flush, handleEvent } = thread

  // Debounce the search box.
  useEffect(() => {
    const t = setTimeout(() => setQuery(search), 300)
    return () => clearTimeout(t)
  }, [search])

  const loadList = useCallback(
    (pageToLoad: number, append: boolean) => {
      return fetchInbox(filter, query, pageToLoad)
        .then((inbox) => {
          setRows((cur) => {
            if (!append) return inbox.conversations
            const seen = new Set(cur.map((r) => r.id))
            return [...cur, ...inbox.conversations.filter((r) => !seen.has(r.id))]
          })
          setTotal(inbox.total)
          setPage(pageToLoad)
          setListState({ loading: false, error: null })
        })
        .catch((e) => setListState({ loading: false, error: e instanceof Error ? e.message : 'Could not load the inbox' }))
    },
    [filter, query],
  )

  useEffect(() => {
    let cancelled = false
    fetchInbox(filter, query, 0)
      .then((inbox) => {
        if (cancelled) return
        setRows(inbox.conversations)
        setTotal(inbox.total)
        setPage(0)
        setListState({ loading: false, error: null })
      })
      .catch((e) => !cancelled && setListState({ loading: false, error: e instanceof Error ? e.message : 'Could not load the inbox' }))
    return () => {
      cancelled = true
    }
  }, [filter, query])

  /** Fetch what the open conversation is missing, and tell the user it was read. */
  const sync = useCallback(
    async (id: string, firstLoad: boolean) => {
      try {
        if (firstLoad) {
          const [conv, page] = await Promise.all([fetchConversation(id), fetchMessages(id, { limit: 50 })])
          setSelected(conv)
          reset(page)
        } else {
          const after = lastStoredId(thread.messages)
          const page = await fetchMessages(id, after ? { after, limit: 100 } : { limit: 50 })
          mergeIncoming(page.messages)
          setSelected(await fetchConversation(id))
        }
        setPaneError(null)
        send({ type: 'mark_read', conversation_id: id })
      } catch (e) {
        setPaneError(e instanceof Error ? e.message : 'Could not load the conversation')
      } finally {
        setPaneLoading(false)
      }
    },
    [reset, mergeIncoming, send, thread.messages],
  )

  function select(row: SupportConversation) {
    if (selectedId === row.id) return
    if (selectedId) send({ type: 'leave_conversation', conversation_id: selectedId })
    clear()
    setSelectedId(row.id)
    setSelected(row)
    setPaneError(null)
    setPaneLoading(true)
    // Join before fetching so nothing is missed in between; the merge dedupes.
    send({ type: 'join_conversation', conversation_id: row.id })
    void sync(row.id, true)
  }

  function back() {
    if (selectedId) send({ type: 'leave_conversation', conversation_id: selectedId })
    clear()
    setSelectedId(null)
    setSelected(null)
  }

  // Live events. The handlers read the latest state through this ref.
  const live = useRef<{ onEvent: (e: SupportServerEvent) => void; onReady: () => void }>({ onEvent: () => {}, onReady: () => {} })
  useEffect(() => {
    live.current = {
      onEvent: (e) => {
        handleEvent(e)
        if (e.type === 'conversation_updated') {
          const c = e.conversation
          setRows((cur) => {
            const exists = cur.some((r) => r.id === c.id)
            if (!matches(c, filter, query)) return exists ? cur.filter((r) => r.id !== c.id) : cur
            return (exists ? cur.map((r) => (r.id === c.id ? c : r)) : [c, ...cur]).sort(byRecent)
          })
          if (c.id === selectedId) setSelected(c)
        } else if (e.type === 'presence') {
          const patch = (r: SupportConversation) => (r.user_id === e.user_id ? { ...r, online: e.online } : r)
          setRows((cur) => cur.map(patch))
          setSelected((cur) => (cur ? patch(cur) : cur))
        } else if (e.type === 'message' && e.message.sender_role === 'user' && e.message.conversation_id === selectedId) {
          if (document.visibilityState === 'visible') send({ type: 'mark_read', conversation_id: selectedId })
        }
      },
      onReady: () => {
        void loadList(0, false)
        if (selectedId) {
          send({ type: 'join_conversation', conversation_id: selectedId })
          void sync(selectedId, false).then(flush)
        }
      },
    }
  })
  useEffect(
    () => support.subscribe((e) => live.current.onEvent(e), () => live.current.onReady()),
    [support],
  )

  async function toggleStatus() {
    if (!selected) return
    setPaneError(null)
    try {
      const updated = selected.status === 'open' ? await closeConversation(selected.id) : await reopenConversation(selected.id)
      setSelected(updated)
    } catch (e) {
      setPaneError(e instanceof Error ? e.message : 'Could not change the conversation')
    }
  }

  const assignedToMe = selected?.assigned_admin_id === currentUserId
  const connected = status === 'open'
  const hasMoreRows = rows.length < total

  return (
    <div className={`inbox${selected ? ' inbox-has-selection' : ''}`}>
      <aside className="inbox-list" aria-label="Conversations">
        <div className="inbox-list-head">
          <h1>Support</h1>
          <span className={`chat-dot chat-dot-${connected ? 'ok' : status === 'offline' ? 'bad' : 'wait'}`} role="img" aria-label={connected ? 'Live' : 'Reconnecting'} title={connected ? 'Live' : 'Reconnecting…'} />
        </div>
        <div className="chips">
          {(['open', 'closed', 'all'] as Filter[]).map((f) => (
            <button key={f} className={`chip${filter === f ? ' chip-active' : ''}`} onClick={() => setFilter(f)}>
              {f[0].toUpperCase() + f.slice(1)}
            </button>
          ))}
        </div>
        <input
          type="search"
          className="inbox-search"
          placeholder="Search name or email"
          aria-label="Search conversations"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        {listState.error && <p className="error">{listState.error}</p>}
        {listState.loading && <p className="text-muted">Loading…</p>}
        {!listState.loading && !listState.error && rows.length === 0 && (
          <p className="text-muted">{query ? 'No conversations match your search.' : filter === 'open' ? 'No open conversations. Nice and quiet.' : 'No conversations here yet.'}</p>
        )}
        <ul className="inbox-rows">
          {rows.map((r) => (
            <li key={r.id}>
              <button className={`inbox-row${r.id === selectedId ? ' inbox-row-active' : ''}`} onClick={() => select(r)} aria-current={r.id === selectedId}>
                <span className="inbox-avatar-wrap">
                  <Avatar c={r} />
                  <span className={`chat-dot chat-dot-${r.online ? 'ok' : 'off'} inbox-presence`} role="img" aria-label={r.online ? 'Online' : 'Offline'} />
                </span>
                <span className="inbox-row-main">
                  <span className="inbox-row-top">
                    <strong>{r.username}</strong>
                    <time className="text-muted" dateTime={r.last_message_at}>{timeAgo(r.last_message_at)}</time>
                  </span>
                  <span className="inbox-row-preview">{r.last_message_preview || 'No messages yet'}</span>
                </span>
                {r.admin_unread_count > 0 && <span className="chat-badge chat-badge-inline" aria-label={`${r.admin_unread_count} unread`}>{r.admin_unread_count > 99 ? '99+' : r.admin_unread_count}</span>}
                {r.status === 'closed' && <span className="chip chip-sm">Closed</span>}
              </button>
            </li>
          ))}
        </ul>
        {hasMoreRows && (
          <button className="btn-secondary btn-sm" onClick={() => void loadList(page + 1, true)}>Load more</button>
        )}
      </aside>

      <section className="inbox-pane" aria-label="Conversation">
        {!selected ? (
          <div className="chat-empty inbox-placeholder">
            <strong>Select a conversation</strong>
            <span>Replies you send appear for the customer instantly.</span>
          </div>
        ) : (
          <>
            <header className="inbox-pane-head">
              <button className="btn-secondary btn-sm inbox-back" onClick={back}>← Inbox</button>
              <Avatar c={selected} />
              <div className="inbox-pane-who">
                <strong>{selected.username}</strong>
                <span className="text-muted">{selected.email}</span>
              </div>
              <span className="chip chip-sm" title="Current plan">{selected.plan_name ?? 'Free'}</span>
              <span className="text-muted inbox-online">
                <span className={`chat-dot chat-dot-${selected.online ? 'ok' : 'off'}`} aria-hidden="true" /> {selected.online ? 'Online' : 'Offline'}
              </span>
              <span className="inbox-spacer" />
              <button className="btn-secondary btn-sm" onClick={() => onOpenUser(selected.user_id)}>View user</button>
              <button className="btn-secondary btn-sm" disabled={assignedToMe} onClick={() => send({ type: 'assign_to_me', conversation_id: selected.id })}>
                {assignedToMe ? 'Assigned to you' : 'Assign to me'}
              </button>
              <button className="btn-primary btn-sm" onClick={() => void toggleStatus()}>
                {selected.status === 'open' ? 'Close' : 'Reopen'}
              </button>
            </header>
            {paneError && <p className="error">{paneError}</p>}
            {!connected && <p className="chat-banner" role="status">Reconnecting… live updates resume automatically.</p>}
            {paneLoading && <p className="chat-note">Loading…</p>}
            <ChatThread
              messages={thread.messages}
              selfRole="admin"
              hasMore={thread.hasMore}
              loadingOlder={thread.loadingOlder}
              onLoadOlder={() => void thread.loadOlder((before) => fetchMessages(selected.id, { before, limit: 30 }))}
              onRetry={thread.retry}
              onDiscard={thread.discard}
              typingLabel={thread.peerTyping ? `${selected.username} is typing…` : null}
              threadKey={selected.id}
              label={`Conversation with ${selected.username}`}
              empty={<span>No messages yet.</span>}
            />
            <ChatComposer
              key={selected.id}
              placeholder={selected.status === 'open' ? `Reply to ${selected.username}…` : 'Reopen the conversation to reply'}
              disabled={selected.status !== 'open'}
              onSend={thread.sendMessage}
              onTyping={(on) => send({ type: 'typing', conversation_id: selected.id, is_typing: on })}
            />
          </>
        )}
      </section>
    </div>
  )
}
