import { useEffect, useState } from 'react'
import { fetchCurrentUser, getGoogleLoginUrl, logout as logoutApi } from '../api/auth'
import type { User } from '../types'

export function useAuth() {
  const [user, setUser] = useState<User | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const params = new URLSearchParams(window.location.search)
    if (params.get('error')) {
      setError('OAuth login failed. Please try again.')
      window.history.replaceState({}, '', '/')
    }

    fetchCurrentUser()
      .then(setUser)
      .finally(() => setLoading(false))
  }, [])

  const login = () => {
    window.location.href = getGoogleLoginUrl()
  }

  const logout = () => {
    logoutApi().then(() => setUser(null))
  }

  return { user, loading, error, login, logout }
}
