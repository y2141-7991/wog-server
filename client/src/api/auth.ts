import type { User } from '../types'

const API = import.meta.env.VITE_API_URL || ''

const GOOGLE_LOGIN_URL = `${API}/api/v1/auth/oauth/google`
const AUTH_ME_URL = `${API}/api/v1/auth/me`
const LOGOUT_URL = `${API}/api/v1/auth/logout`

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
