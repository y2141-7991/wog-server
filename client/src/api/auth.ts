import type { User } from '../types'

const API = import.meta.env.VITE_API_URL || ''

const GOOGLE_LOGIN_URL = `${API}/api/v1/auth/oauth/google`
const AUTH_ME_URL = `${API}/api/v1/auth/me`
const LOGOUT_URL = `${API}/api/v1/auth/logout`
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

export async function fetchCurrentUser(): Promise<User | null> {
  const res = await fetch(AUTH_ME_URL, { credentials: 'include' })
  if (!res.ok) return null
  return res.json()
}

export async function logout(): Promise<void> {
  await fetch(LOGOUT_URL, { method: 'POST', credentials: 'include' })
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
  return (body as AuthResponse).user
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
  return (body as AuthResponse).user
}

export async function verifyEmail(token: string): Promise<User> {
  const res = await fetch(VERIFY_EMAIL_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ token }),
    credentials: 'include',
  })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(body.message || 'Invalid or expired verification link')
  }
  return (body as { user: User }).user
}

export async function resendVerification(): Promise<void> {
  const res = await fetch(RESEND_VERIFICATION_URL, {
    method: 'POST',
    credentials: 'include',
  })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(body.message || 'Could not send verification email')
  }
}

export async function changePassword(currentPassword: string, newPassword: string): Promise<User> {
  const res = await fetch(CHANGE_PASSWORD_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
    credentials: 'include',
  })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(body.message || 'Could not change the password')
  }
  return body as User
}
