import type { User } from '../types'

const GOOGLE_LOGIN_URL = '/api/v1/auth/oauth/google'
const AUTH_ME_URL = '/api/v1/auth/me'
const LOGOUT_URL = '/api/v1/auth/logout'

export async function fetchCurrentUser(): Promise<User | null> {
  const res = await fetch(AUTH_ME_URL)
  if (!res.ok) return null
  return res.json()
}

export async function logout(): Promise<void> {
  await fetch(LOGOUT_URL, { method: 'POST' })
}

export function getGoogleLoginUrl(): string {
  return GOOGLE_LOGIN_URL
}
