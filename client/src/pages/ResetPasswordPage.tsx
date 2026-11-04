import { useEffect, useState } from 'react'
import { resetPassword } from '../api/auth'
import { BrandHeading } from '../components/BrandLogo'

const MIN_LENGTH = 8

/**
 * The page behind the emailed link (`/reset-password?token=...`). The token is
 * read once and then removed from the address bar, so it does not linger in
 * the browser history or leak through a screenshot or a shared URL.
 */
export function ResetPasswordPage({ onForgot, onDone }: { onForgot: () => void; onDone: () => void }) {
  const [token] = useState(() => new URLSearchParams(window.location.search).get('token') ?? '')
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const [done, setDone] = useState(false)

  useEffect(() => {
    if (window.location.search) window.history.replaceState(null, '', window.location.pathname)
  }, [])

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault()
    setError(null)
    const form = new FormData(e.currentTarget)
    const password = String(form.get('password') ?? '')
    const confirm = String(form.get('confirm') ?? '')
    if (password.length < MIN_LENGTH) {
      setError(`Password must be at least ${MIN_LENGTH} characters`)
      return
    }
    if (password !== confirm) {
      setError('The two passwords do not match')
      return
    }

    setSubmitting(true)
    try {
      await resetPassword(token, password)
      setDone(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Something went wrong')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="container">
      <BrandHeading />
      <div className="form-card auth-card">
        <h2>{done ? 'Password updated' : 'Choose a new password'}</h2>
        {done ? (
          <>
            <p role="status">Your password was updated and you were signed out everywhere. Sign in with the new one.</p>
            <div className="auth-actions">
              <button type="button" className="auth-submit" onClick={onDone}>
                Go to sign in
              </button>
            </div>
          </>
        ) : !token ? (
          <>
            <p className="error" role="alert">This reset link is missing its token.</p>
            <div className="auth-actions">
              <button type="button" className="auth-submit" onClick={onForgot}>
                Get a new link
              </button>
            </div>
          </>
        ) : (
          <>
            {error && <p className="error" role="alert">{error}</p>}
            <form onSubmit={handleSubmit}>
              <label>
                New password
                <input type="password" name="password" autoComplete="new-password" required minLength={MIN_LENGTH} autoFocus />
              </label>
              <label>
                Repeat the new password
                <input type="password" name="confirm" autoComplete="new-password" required minLength={MIN_LENGTH} />
              </label>
              <button type="submit" className="auth-submit" disabled={submitting}>
                {submitting ? 'Please wait...' : 'Update password'}
              </button>
            </form>
            {error?.toLowerCase().includes('link') && (
              <p className="auth-switch">
                <button type="button" className="link-btn" onClick={onForgot}>
                  Get a new link
                </button>
              </p>
            )}
          </>
        )}
      </div>
    </div>
  )
}
