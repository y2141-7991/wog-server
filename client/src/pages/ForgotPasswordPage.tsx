import { useState } from 'react'
import { forgotPassword } from '../api/auth'
import { BrandHeading } from '../components/BrandLogo'

export function ForgotPasswordPage({ onBack }: { onBack: () => void }) {
  const [sent, setSent] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault()
    setError(null)
    setSubmitting(true)
    const email = String(new FormData(e.currentTarget).get('email') ?? '')
    try {
      setSent(await forgotPassword(email))
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
        <h2>Forgot your password?</h2>
        {sent ? (
          <>
            <p role="status">{sent}</p>
            <p className="text-muted">The link works for 30 minutes. Check your spam folder if it does not arrive.</p>
          </>
        ) : (
          <>
            <p className="text-muted">Enter the email of your account and we will send you a link to choose a new password.</p>
            {error && <p className="error" role="alert">{error}</p>}
            <form onSubmit={handleSubmit}>
              <label>
                Email
                <input type="email" name="email" autoComplete="email" required autoFocus />
              </label>
              <button type="submit" className="auth-submit" disabled={submitting}>
                {submitting ? 'Please wait...' : 'Send reset link'}
              </button>
            </form>
          </>
        )}
        <p className="auth-switch">
          <button type="button" className="link-btn" onClick={onBack}>
            Back to sign in
          </button>
        </p>
      </div>
    </div>
  )
}
