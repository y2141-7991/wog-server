import { useEffect } from 'react'
import { fetchInbox } from '../api/support'
import { publishAdminEvent, publishAdminReady, resetAdminChat, useAdminChatStore } from '../stores/adminChat'
import { useSupportSocket } from './useSupportSocket'

/**
 * Owns the admin's single chat socket for as long as they are signed in, so the nav
 * badge stays live on every page. Mount it once (App does). What it learns goes into
 * `useAdminChatStore`, which the navbar and the inbox page read; they never touch the
 * socket themselves. Also shows the unread count in the browser tab title.
 */
export function useAdminSupport(enabled: boolean) {
  const socket = useSupportSocket({
    enabled,
    onEvent: publishAdminEvent,
    onReady: () => {
      fetchInbox('open', '', 0)
        .then((inbox) => useAdminChatStore.setState({ totalUnread: inbox.total_unread }))
        .catch(() => {})
      publishAdminReady()
    },
  })

  useEffect(() => {
    useAdminChatStore.setState({ status: socket.status, send: socket.send })
  }, [socket.status, socket.send])

  useEffect(() => {
    if (!enabled) resetAdminChat()
  }, [enabled])

  const totalUnread = useAdminChatStore((s) => s.totalUnread)
  useEffect(() => {
    const base = document.title.replace(/^\(\d+\+?\) /, '')
    document.title = totalUnread > 0 ? `(${totalUnread > 99 ? '99+' : totalUnread}) ${base}` : base
    return () => {
      document.title = base
    }
  }, [totalUnread])
}
