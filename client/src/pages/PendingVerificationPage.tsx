import { useState } from 'react'
import type { User } from '../types'

type ResendStatus = 'idle' | 'sending' | 'sent' | 'error'

interface PendingVerificationPageProps {
  user: User
  onResend: () => Promise<void>
  onLogout: () => void
}

export function PendingVerificationPage({ user, onResend, onLogout }: PendingVerificationPageProps) {
  const [status, setStatus] = useState<ResendStatus>('idle')

  async function handleResend() {
    setStatus('sending')
    try {
      await onResend()
      setStatus('sent')
    } catch {
      setStatus('error')
    }
  }

  return (
    <div className="container">
      <h1>WoG</h1>
      <div className="form-card auth-card">
        <h2>Check your email</h2>
        <p>
          We sent a verification link to <strong>{user.email}</strong>. Open it to finish
          setting up your account.
        </p>
        {status === 'sent' ? (
          <p className="verify-sent">Verification email sent.</p>
        ) : (
          <button type="button" onClick={handleResend} disabled={status === 'sending'}>
            {status === 'sending' ? 'Sending...' : status === 'error' ? 'Try again' : 'Resend email'}
          </button>
        )}
        <p className="auth-switch">
          Wrong account?{' '}
          <button type="button" className="link-btn" onClick={onLogout}>
            Sign out
          </button>
        </p>
      </div>
    </div>
  )
}
