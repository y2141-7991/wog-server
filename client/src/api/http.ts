/**
 * The one place that knows how requests are authenticated.
 *
 * The access token lives only in this module's memory: not in a cookie, not in
 * localStorage, so a reload forgets it and nothing another site sends can carry it.
 * Calls send it as `Authorization: Bearer`. When one comes back 401 the token is
 * probably just old, so we trade the refresh cookie (HttpOnly, scoped to
 * /api/v1/auth, invisible to scripts) for a new one and retry once. Calls that
 * fail together share a single refresh.
 */

export const API = import.meta.env.VITE_API_URL || ''
const AUTH = `${API}/api/v1/auth`

let accessToken: string | null = null
let refreshInFlight: Promise<string | null> | null = null
let onSessionLost: (() => void) | null = null

export const getAccessToken = () => accessToken
export const setAccessToken = (token: string | null) => {
  accessToken = token
}

/** Called when the session cannot be recovered: the refresh failed, so the user must sign in again. */
export function setSessionLostHandler(handler: () => void) {
  onSessionLost = handler
}

/**
 * Trade the refresh cookie for a new access token. `null` when there is no valid
 * cookie (signed out, logged out elsewhere, or a replayed token ended the session).
 * Concurrent callers get the same answer from one request.
 */
export function refreshAccessToken(): Promise<string | null> {
  refreshInFlight ??= (async () => {
    try {
      const res = await fetch(`${AUTH}/refresh`, { method: 'POST', credentials: 'include' })
      if (!res.ok) {
        accessToken = null
        return null
      }
      const body = (await res.json()) as { access_token?: string }
      accessToken = body.access_token ?? null
      return accessToken
    } catch {
      // Offline or the server is down: that says nothing about the session. Keep
      // the token we have and let the caller see its own failure.
      return accessToken
    } finally {
      refreshInFlight = null
    }
  })()
  return refreshInFlight
}

function withToken(init: RequestInit, token: string | null): RequestInit {
  if (!token) return init
  const headers = new Headers(init.headers)
  headers.set('Authorization', `Bearer ${token}`)
  return { ...init, headers }
}

/**
 * `fetch` for calls that need a signed-in user. Sends the Bearer token; on a 401
 * refreshes once and retries; if that fails too, reports the session lost and
 * returns the 401 so the caller can show its own error.
 */
export async function apiFetch(url: string, init: RequestInit = {}): Promise<Response> {
  const sent = accessToken
  const res = await fetch(url, withToken(init, sent))
  if (res.status !== 401) return res

  // Another request may have refreshed while this one was in flight: the token has
  // already changed, so just retry with it instead of rotating the session again.
  if (accessToken && accessToken !== sent) return fetch(url, withToken(init, accessToken))

  const fresh = await refreshAccessToken()
  if (!fresh) {
    onSessionLost?.()
    return res
  }
  // Retrying with the very token that was just refused would only fail again.
  if (fresh === sent) return res
  return fetch(url, withToken(init, fresh))
}

/** Ask the server to end this sign-in (revokes the refresh token), then forget the access token. */
export async function endSession(): Promise<void> {
  try {
    await fetch(`${AUTH}/logout`, { method: 'POST', credentials: 'include' })
  } finally {
    accessToken = null
  }
}
