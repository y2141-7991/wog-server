import { useCallback, useEffect, useRef, useState } from 'react'
import { fetchCurrentUser } from '../api/auth'
import { supportSocketUrl } from '../api/support'
import type { SocketStatus, SupportClientEvent, SupportServerEvent } from '../types'

const PING_EVERY_MS = 25_000
/** No frame at all for this long means the connection is dead even if the browser has not noticed. */
const SILENCE_LIMIT_MS = 70_000
const BACKOFF_BASE_MS = 1_000
const BACKOFF_MAX_MS = 30_000

interface Options {
  /** Connect while true; close (and stop reconnecting) when it turns false or on unmount. */
  enabled: boolean
  onEvent: (event: SupportServerEvent) => void
  /** Called on every `ready`, first connect and reconnects alike: refetch what you missed here. */
  onReady?: (info: { reconnect: boolean }) => void
  /** The server rejected our session; the caller should treat the user as signed out. */
  onAuthLost?: () => void
}

/** Exponential backoff with jitter: 1s, 2s, 4s ... capped at 30s, each scaled by 50-100%. */
export function backoffDelay(attempt: number, random = Math.random()): number {
  const base = Math.min(BACKOFF_MAX_MS, BACKOFF_BASE_MS * 2 ** attempt)
  return Math.round(base * (0.5 + random * 0.5))
}

/**
 * One chat WebSocket with typed events, heartbeat and automatic reconnects.
 * The session cookie authenticates it. Reconnects after a drop, when the
 * browser comes back online and when the tab becomes visible again.
 */
export function useSupportSocket({ enabled, onEvent, onReady, onAuthLost }: Options) {
  const [status, setStatus] = useState<SocketStatus>('idle')
  const socketRef = useRef<WebSocket | null>(null)

  // Always call the latest callbacks without re-running the connection effect.
  const handlers = useRef({ onEvent, onReady, onAuthLost })
  useEffect(() => {
    handlers.current = { onEvent, onReady, onAuthLost }
  })

  useEffect(() => {
    if (!enabled) return

    let stopped = false
    let attempt = 0
    let failedOpens = 0
    let everReady = false
    let retryTimer: ReturnType<typeof setTimeout> | undefined
    let heartbeat: ReturnType<typeof setInterval> | undefined
    let lastFrameAt = Date.now()

    const clearTimers = () => {
      clearTimeout(retryTimer)
      clearInterval(heartbeat)
    }

    const scheduleReconnect = () => {
      if (stopped) return
      clearTimeout(retryTimer)
      setStatus(navigator.onLine ? 'reconnecting' : 'offline')
      retryTimer = setTimeout(connect, backoffDelay(attempt++))
    }

    function connect() {
      if (stopped) return
      clearTimeout(retryTimer)
      setStatus(everReady ? 'reconnecting' : 'connecting')
      let gotReady = false
      let socket: WebSocket
      try {
        socket = new WebSocket(supportSocketUrl())
      } catch {
        scheduleReconnect()
        return
      }
      socketRef.current = socket

      socket.onopen = () => {
        lastFrameAt = Date.now()
        clearInterval(heartbeat)
        heartbeat = setInterval(() => {
          if (Date.now() - lastFrameAt > SILENCE_LIMIT_MS) {
            socket.close() // onclose schedules the reconnect
            return
          }
          if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'ping' }))
        }, PING_EVERY_MS)
      }

      socket.onmessage = (message) => {
        lastFrameAt = Date.now()
        let event: SupportServerEvent
        try {
          event = JSON.parse(String(message.data)) as SupportServerEvent
        } catch {
          return
        }
        if (event.type === 'ready') {
          gotReady = true
          attempt = 0
          failedOpens = 0
          setStatus('open')
          handlers.current.onEvent(event)
          handlers.current.onReady?.({ reconnect: everReady })
          everReady = true
          return
        }
        handlers.current.onEvent(event)
      }

      socket.onclose = () => {
        if (socketRef.current === socket) socketRef.current = null
        clearInterval(heartbeat)
        if (stopped) return
        if (!gotReady) {
          failedOpens += 1
          // A refused handshake looks the same as a dead server from here, so
          // ask the REST API whether the session itself is still good.
          if (failedOpens >= 2) {
            void fetchCurrentUser().then((user) => {
              if (!stopped && !user) {
                stopped = true
                clearTimers()
                setStatus('offline')
                handlers.current.onAuthLost?.()
              }
            }).catch(() => {})
          }
        }
        scheduleReconnect()
      }

      socket.onerror = () => {
        // onclose follows and does the work
      }
    }

    const reconnectNow = () => {
      if (stopped) return
      const s = socketRef.current
      if (s && (s.readyState === WebSocket.OPEN || s.readyState === WebSocket.CONNECTING)) return
      attempt = 0
      connect()
    }
    const onVisible = () => {
      if (document.visibilityState === 'visible') reconnectNow()
    }
    window.addEventListener('online', reconnectNow)
    document.addEventListener('visibilitychange', onVisible)

    connect()

    return () => {
      stopped = true
      clearTimers()
      window.removeEventListener('online', reconnectNow)
      document.removeEventListener('visibilitychange', onVisible)
      const s = socketRef.current
      socketRef.current = null
      if (s) {
        s.onclose = null
        s.close(1000)
      }
      setStatus('idle')
    }
  }, [enabled])

  /** Send one event; `false` when the socket is not open (nothing is queued). */
  const send = useCallback((event: SupportClientEvent): boolean => {
    const s = socketRef.current
    if (!s || s.readyState !== WebSocket.OPEN) return false
    s.send(JSON.stringify(event))
    return true
  }, [])

  return { status, send }
}
