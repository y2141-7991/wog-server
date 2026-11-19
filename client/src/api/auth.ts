import type { User } from '../types'
import { API, apiFetch, endSession, refreshAccessToken, setAccessToken } from './http'

const GOOGLE_LOGIN_URL = `${API}/api/v1/auth/oauth/google`
const AUTH_ME_URL = `${API}/api/v1/auth/me`
const EXCHANGE_URL = `${API}/api/v1/auth/oauth/exchange`
const LOGIN_URL = `${API}/api/v1/auth/login`
const REGISTER_URL = `${API}/api/v1/auth/register`
const VERIFY_EMAIL_URL = `${API}/api/v1/auth/verify-email`
const CHANGE_PASSWORD_URL = `${API}/api/v1/auth/change-password`
const RESEND_VERIFICATION_URL = `${API}/api/v1/auth/resend-verification`

interface AuthResponse {
  user: User
  access_token: string
  refresh_token: string
}

/** The signed-in user, or `null`. A 401 here has already tried a refresh (see `apiFetch`). */
export async function fetchCurrentUser(): Promise<User | null> {
  const res = await apiFetch(AUTH_ME_URL)
  if (!res.ok) return null
  return res.json()
}

/**
 * Pick the session back up after a reload: the refresh cookie (if there is one) buys an
 * access token, and the token identifies the user. `null` when signed out.
 */
export async function restoreSession(): Promise<User | null> {
  if (!(await refreshAccessToken())) return null
  return fetchCurrentUser()
}

export const logout = endSession

/** Keep the access token from a sign-in response in memory and hand back the user. */
function startFrom(body: AuthResponse): User {
  setAccessToken(body.access_token)
  return body.user
}

export function getGoogleLoginUrl(): string {
  return GOOGLE_LOGIN_URL
}

export async function loginWithPassword(email: string, password: string): Promise<User> {
  const res = await fetch(LOGIN_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password }),
    credentials: 'include',
  })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(body.message || 'Invalid email or password')
  }
  return startFrom(body as AuthResponse)
}

export async function registerWithPassword(username: string, email: string, password: string): Promise<User> {
  const res = await fetch(REGISTER_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, email, password }),
    credentials: 'include',
  })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(body.message || 'Could not create account')
  }
  return startFrom(body as AuthResponse)
}

/** Swap the one-time code from the Google sign-in redirect for a session. */
export async function exchangeOAuthCode(code: string): Promise<User> {
  const res = await fetch(EXCHANGE_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ code }),
    credentials: 'include',
  })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(body.message || 'Google sign-in failed. Please try again.')
  }
  return startFrom(body as AuthResponse)
}

export async function verifyEmail(token: string): Promise<User> {
  const res = await fetch(VERIFY_EMAIL_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ token }),
  })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(body.message || 'Invalid or expired verification link')
  }
  return (body as { user: User }).user
}

export async function resendVerification(): Promise<void> {
  const res = await apiFetch(RESEND_VERIFICATION_URL, { method: 'POST' })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(body.message || 'Could not send verification email')
  }
}

export async function changePassword(currentPassword: string, newPassword: string): Promise<User> {
  const res = await apiFetch(CHANGE_PASSWORD_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
  })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(body.message || 'Could not change the password')
  }
  return body as User
}

const FORGOT_PASSWORD_URL = `${API}/api/v1/auth/forgot-password`
const RESET_PASSWORD_URL = `${API}/api/v1/auth/reset-password`

/** Ask for a reset link. The answer is the same whether or not the email is registered. */
export async function forgotPassword(email: string): Promise<string> {
  const res = await fetch(FORGOT_PASSWORD_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email }),
  })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(body.message || 'Could not send the reset link')
  }
  return (body as { message: string }).message
}

/** Set a new password with the token from the emailed link. Drops any stale session in this browser. */
export async function resetPassword(token: string, newPassword: string): Promise<void> {
  const res = await fetch(RESET_PASSWORD_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ token, new_password: newPassword }),
  })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(body.message || 'Could not reset the password')
  }
  // Every older session is void now; drop this browser's refresh cookie and token too.
  await endSession().catch(() => undefined)
}
