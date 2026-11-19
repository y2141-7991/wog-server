import { create } from 'zustand'
import type { SocketStatus, SupportClientEvent, SupportServerEvent } from '../types'

type Listener = (event: SupportServerEvent) => void
type ReadyListener = () => void

interface AdminChatState {
  /** Connection state of the admin's chat socket. */
  status: SocketStatus
  /** Unread customer messages across every conversation: the nav badge and the tab title. */
  totalUnread: number
  /** Send one event on the socket; `false` when it is not open. Set by `useAdminSupport`. */
  send: (event: SupportClientEvent) => boolean
}

export const useAdminChatStore = create<AdminChatState>()(() => ({
  status: 'idle',
  totalUnread: 0,
  send: () => false,
}))

// Event subscribers are plain callbacks, not state: nothing renders from them, so they
// live beside the store instead of inside it.
const listeners = new Set<Listener>()
const readyListeners = new Set<ReadyListener>()

/** Listen to every socket event (and to each `ready`); returns the unsubscribe function. */
export function subscribeAdminChat(listener: Listener, onReady?: ReadyListener) {
  listeners.add(listener)
  if (onReady) readyListeners.add(onReady)
  return () => {
    listeners.delete(listener)
    if (onReady) readyListeners.delete(onReady)
  }
}

/** Called by the socket owner for each incoming event. */
export function publishAdminEvent(event: SupportServerEvent) {
  if (event.type === 'conversation_updated' && event.total_unread != null) {
    useAdminChatStore.setState({ totalUnread: event.total_unread })
  }
  listeners.forEach((l) => l(event))
}

/** Called by the socket owner on every `ready` (first connect and reconnects). */
export function publishAdminReady() {
  readyListeners.forEach((l) => l())
}

/** Back to "nothing happened yet", e.g. when the admin signs out. */
export function resetAdminChat() {
  useAdminChatStore.setState({ status: 'idle', totalUnread: 0 })
}
