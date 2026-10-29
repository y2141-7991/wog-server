import { useState } from 'react'

interface ChangePasswordPageProps {
  onChange: (currentPassword: string, newPassword: string) => Promise<void>
  /** Set while the account is required to pick a new password before using the app. */
  forced?: boolean
  onLogout?: () => void
  onBack?: () => void
}

export function ChangePasswordPage({ onChange, forced = false, onLogout, onBack }: ChangePasswordPageProps) {
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [confirm, setConfirm] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [done, setDone] = useState(false)
  const [saving, setSaving] = useState(false)

  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault()
    setError(null)
    setDone(false)
    if (next !== confirm) {
      setError('The new passwords do not match')
      return
    }
    setSaving(true)
    try {
      await onChange(current, next)
      setCurrent('')
      setNext('')
      setConfirm('')
      setDone(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not change the password')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="container">
      <div className="form-card auth-card">
        <h2>{forced ? 'Choose a new password' : 'Change password'}</h2>
        {forced && <p className="text-muted">You are using the initial password. Set your own to continue.</p>}
        {error && <p className="error">{error}</p>}
        {done && <p className="success">Password updated.</p>}
        <form onSubmit={submit}>
          <label>
            Current password
            <input type="password" value={current} onChange={(e) => setCurrent(e.target.value)} autoComplete="current-password" required />
          </label>
          <label>
            New password (8+ characters)
            <input type="password" value={next} onChange={(e) => setNext(e.target.value)} autoComplete="new-password" minLength={8} required />
          </label>
          <label>
            Confirm new password
            <input type="password" value={confirm} onChange={(e) => setConfirm(e.target.value)} autoComplete="new-password" minLength={8} required />
          </label>
          <button type="submit" className="auth-submit" disabled={saving}>
            {saving ? 'Saving...' : 'Change password'}
          </button>
        </form>
        <p className="auth-switch">
          {onBack && <button type="button" className="link-btn" onClick={onBack}>Back to profile</button>}
          {forced && onLogout && <button type="button" className="link-btn" onClick={onLogout}>Sign out</button>}
        </p>
      </div>
    </div>
  )
}
