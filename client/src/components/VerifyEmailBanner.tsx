import { useState } from 'react'

type Status = 'idle' | 'sending' | 'sent' | 'error'

interface VerifyEmailBannerProps {
  onResend: () => Promise<void>
}

export function VerifyEmailBanner({ onResend }: VerifyEmailBannerProps) {
  const [status, setStatus] = useState<Status>('idle')

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
    <div className="verify-banner">
      <span>Please verify your email address.</span>
      {status === 'sent' ? (
        <span className="verify-banner-sent">Verification email sent.</span>
      ) : (
        <button
          type="button"
          className="link-btn"
          onClick={handleResend}
          disabled={status === 'sending'}
        >
          {status === 'sending' ? 'Sending...' : status === 'error' ? 'Try again' : 'Resend email'}
        </button>
      )}
    </div>
  )
}
