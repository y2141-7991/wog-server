import { useEffect, useState } from 'react'
import type { User } from '../types'

type Status = 'verifying' | 'success' | 'error'

interface VerifyEmailPageProps {
  onVerify: (token: string) => Promise<User>
  onContinue: () => void
}

export function VerifyEmailPage({ onVerify, onContinue }: VerifyEmailPageProps) {
  const [status, setStatus] = useState<Status>('verifying')
  const [message, setMessage] = useState<string | null>(null)

  useEffect(() => {
    const token = new URLSearchParams(window.location.search).get('token')
    if (!token) {
      setStatus('error')
      setMessage('This verification link is missing its token.')
      return
    }

    onVerify(token)
      .then(() => setStatus('success'))
      .catch((err) => {
        setStatus('error')
        setMessage(err instanceof Error ? err.message : 'Something went wrong')
      })
    // Intentionally run once on mount, regardless of prop identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return (
    <div className="container">
      <h1>WoG</h1>
      <div className="form-card auth-card">
        {status === 'verifying' && <p>Verifying your email...</p>}
        {status === 'success' && (
          <>
            <p>Your email has been verified.</p>
            <button type="button" onClick={onContinue}>
              Continue
            </button>
          </>
        )}
        {status === 'error' && (
          <>
            <p className="error">{message}</p>
            <button type="button" onClick={onContinue}>
              Back to WoG
            </button>
          </>
        )}
      </div>
    </div>
  )
}
