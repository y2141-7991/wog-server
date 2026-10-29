import { useState } from 'react'
import { GoogleButton } from '../components/GoogleButton'

type Mode = 'login' | 'register'

interface LoginPageProps {
  error: string | null
  onLogin: () => void
  onLoginWithPassword: (email: string, password: string) => Promise<void>
  onRegister: (username: string, email: string, password: string) => Promise<void>
}

export function LoginPage({ error, onLogin, onLoginWithPassword, onRegister }: LoginPageProps) {
  const [mode, setMode] = useState<Mode>('login')
  const [formError, setFormError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const [showPassword, setShowPassword] = useState(false)

  function switchMode(next: Mode) {
    setMode(next)
    setFormError(null)
  }

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault()
    setFormError(null)
    setSubmitting(true)

    const form = new FormData(e.currentTarget)
    const email = form.get('email') as string
    const password = form.get('password') as string

    try {
      if (mode === 'login') {
        await onLoginWithPassword(email, password)
      } else {
        const username = form.get('username') as string
        await onRegister(username, email, password)
      }
    } catch (err) {
      setFormError(err instanceof Error ? err.message : 'Something went wrong')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="container">
      <h1>WoG</h1>
      {error && <p className="error">{error}</p>}

      <div className="form-card auth-card">
        <h2>{mode === 'login' ? 'Sign in' : 'Create account'}</h2>
        {formError && <p className="error">{formError}</p>}
        <form onSubmit={handleSubmit}>
          {mode === 'register' && (
            <label>
              Username
              <input name="username" autoComplete="username" required minLength={2} />
            </label>
          )}
          <label>
            {mode === 'login' ? 'Email or username' : 'Email'}
            {mode === 'login' ? (
              <input type="text" name="email" autoComplete="username" required />
            ) : (
              <input type="email" name="email" autoComplete="email" required />
            )}
          </label>
          <label>
            Password
            <div className="password-field">
              <input
                type={showPassword ? 'text' : 'password'}
                name="password"
                autoComplete={mode === 'login' ? 'current-password' : 'new-password'}
                required
                minLength={mode === 'register' ? 8 : undefined}
              />
              <button
                type="button"
                className="password-toggle"
                onClick={() => setShowPassword((s) => !s)}
                aria-label={showPassword ? 'Hide password' : 'Show password'}
              >
                {showPassword ? (
                  <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7Z" />
                    <circle cx="12" cy="12" r="3" />
                  </svg>
                ) : (
                  <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7Z" />
                    <circle cx="12" cy="12" r="3" />
                    <line x1="3" y1="21" x2="21" y2="3" />
                  </svg>
                )}
              </button>
            </div>
          </label>
          <button type="submit" className="auth-submit" disabled={submitting}>
            {submitting ? 'Please wait...' : mode === 'login' ? 'Sign in' : 'Create account'}
          </button>
        </form>
        <p className="auth-switch">
          {mode === 'login' ? (
            <>
              Don't have an account?{' '}
              <button type="button" className="link-btn" onClick={() => switchMode('register')}>
                Sign up
              </button>
            </>
          ) : (
            <>
              Already have an account?{' '}
              <button type="button" className="link-btn" onClick={() => switchMode('login')}>
                Sign in
              </button>
            </>
          )}
        </p>
      </div>

      <div className="auth-divider">
        <span>or</span>
      </div>

      <GoogleButton onClick={onLogin} />
    </div>
  )
}
