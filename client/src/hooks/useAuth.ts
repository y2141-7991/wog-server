import { useEffect, useState } from 'react'
import {
  fetchCurrentUser,
  getGoogleLoginUrl,
  loginWithPassword as loginWithPasswordApi,
  changePassword as changePasswordApi,
  logout as logoutApi,
  registerWithPassword as registerWithPasswordApi,
  resendVerification as resendVerificationApi,
  verifyEmail as verifyEmailApi,
} from '../api/auth'
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

  const loginWithPassword = async (email: string, password: string) => {
    const loggedInUser = await loginWithPasswordApi(email, password)
    setUser(loggedInUser)
  }

  const register = async (username: string, email: string, password: string) => {
    const newUser = await registerWithPasswordApi(username, email, password)
    setUser(newUser)
  }

  const verifyEmail = async (token: string) => {
    const verifiedUser = await verifyEmailApi(token)
    setUser(verifiedUser)
    return verifiedUser
  }

  const resendVerification = () => resendVerificationApi()

  const changePassword = async (currentPassword: string, newPassword: string) => {
    setUser(await changePasswordApi(currentPassword, newPassword))
  }

  const logout = () => {
    logoutApi().then(() => setUser(null))
  }

  return {
    user,
    loading,
    error,
    login,
    loginWithPassword,
    register,
    verifyEmail,
    resendVerification,
    changePassword,
    logout,
  }
}
