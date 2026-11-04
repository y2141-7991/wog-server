import { useCallback, useEffect, useRef, useState } from 'react'
import { fetchInbox } from '../api/support'
import type { SupportServerEvent } from '../types'
import { useSupportSocket } from './useSupportSocket'

type Listener = (event: SupportServerEvent) => void
type ReadyListener = () => void

/**
 * The admin's single chat socket, kept for as long as they are signed in so the
 * nav badge stays live on every page. The inbox page subscribes to its events.
 * Also shows the unread count in the browser tab title.
 */
export function useAdminSupport(enabled: boolean) {
  const [totalUnread, setTotalUnread] = useState(0)
  const listeners = useRef(new Set<Listener>())
  const readyListeners = useRef(new Set<ReadyListener>())

  const onEvent = (e: SupportServerEvent) => {
    if (e.type === 'conversation_updated' && e.total_unread != null) setTotalUnread(e.total_unread)
    listeners.current.forEach((l) => l(e))
  }

  const onReady = () => {
    fetchInbox('open', '', 0)
      .then((inbox) => setTotalUnread(inbox.total_unread))
      .catch(() => {})
    readyListeners.current.forEach((l) => l())
  }

  const socket = useSupportSocket({ enabled, onEvent, onReady })

  useEffect(() => {
    const base = document.title.replace(/^\(\d+\+?\) /, '')
    document.title = totalUnread > 0 ? `(${totalUnread > 99 ? '99+' : totalUnread}) ${base}` : base
    return () => {
      document.title = base
    }
  }, [totalUnread])

  /** Listen to every socket event (and to each `ready`); returns the unsubscribe function. */
  const subscribe = useCallback((listener: Listener, onReadyListener?: ReadyListener) => {
    listeners.current.add(listener)
    if (onReadyListener) readyListeners.current.add(onReadyListener)
    return () => {
      listeners.current.delete(listener)
      if (onReadyListener) readyListeners.current.delete(onReadyListener)
    }
  }, [])

  return { status: socket.status, send: socket.send, totalUnread, subscribe }
}

export type AdminSupport = ReturnType<typeof useAdminSupport>
