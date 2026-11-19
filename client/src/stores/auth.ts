import { create } from 'zustand'
import {
  changePassword as changePasswordApi,
  exchangeOAuthCode,
  getGoogleLoginUrl,
  loginWithPassword as loginWithPasswordApi,
  logout as logoutApi,
  registerWithPassword as registerWithPasswordApi,
  restoreSession,
  resendVerification as resendVerificationApi,
  verifyEmail as verifyEmailApi,
} from '../api/auth'
import { getAccessToken, setSessionLostHandler } from '../api/http'
import { queryClient } from '../lib/queryClient'
import type { User } from '../types'

interface AuthState {
  /** The signed-in user, or `null` when signed out (or before `init` finishes). */
  user: User | null
  /** True until the first "who am I?" answer arrives. */
  loading: boolean
  error: string | null

  /**
   * Pick the session up at startup (see main.tsx): finish a Google sign-in if the page
   * was opened by its redirect, otherwise trade the refresh cookie for an access token.
   */
  init: () => Promise<void>
  /** Replace the user after a server call that returns a fresh copy. */
  setUser: (user: User | null) => void
  login: () => void
  loginWithPassword: (email: string, password: string) => Promise<void>
  register: (username: string, email: string, password: string) => Promise<void>
  verifyEmail: (token: string) => Promise<User>
  resendVerification: () => Promise<void>
  changePassword: (currentPassword: string, newPassword: string) => Promise<void>
  logout: () => void
}

export const useAuthStore = create<AuthState>()((set) => ({
  user: null,
  loading: true,
  error: null,

  init: async () => {
    const params = new URLSearchParams(window.location.search)
    const code = params.get('code')
    if (params.get('error') || code) {
      // Neither the error nor the one-time code should stay in the address bar.
      window.history.replaceState({}, '', '/')
    }
    if (params.get('error')) {
      set({ error: 'OAuth login failed. Please try again.' })
    }
    try {
      if (code) {
        try {
          set({ user: await exchangeOAuthCode(code) })
          return
        } catch (e) {
          set({ error: e instanceof Error ? e.message : 'Google sign-in failed. Please try again.' })
        }
      }
      set({ user: await restoreSession() })
    } finally {
      set({ loading: false })
    }
  },

  setUser: (user) => set({ user }),

  login: () => {
    window.location.href = getGoogleLoginUrl()
  },

  loginWithPassword: async (email, password) => {
    set({ user: await loginWithPasswordApi(email, password) })
  },

  register: async (username, email, password) => {
    set({ user: await registerWithPasswordApi(username, email, password) })
  },

  verifyEmail: async (token) => {
    const user = await verifyEmailApi(token)
    // The link may be opened in a browser that is not signed in; only a live session
    // (one holding an access token) should show the app.
    if (getAccessToken()) set({ user })
    return user
  },

  resendVerification: () => resendVerificationApi(),

  changePassword: async (currentPassword, newPassword) => {
    set({ user: await changePasswordApi(currentPassword, newPassword) })
  },

  logout: () => {
    // Forget locally whether or not the server could be told.
    void logoutApi().finally(() => signedOut())
  },
}))

function signedOut() {
  // Whatever was fetched belongs to the person who just left.
  queryClient.clear()
  useAuthStore.setState({ user: null })
}

// A request that cannot be saved by a refresh means the session is over (logged out in
// another tab, ended by a password reset, expired): show the sign-in page.
setSessionLostHandler(signedOut)
