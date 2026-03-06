import { GoogleButton } from '../components/GoogleButton'

interface LoginPageProps {
  error: string | null
  onLogin: () => void
}

export function LoginPage({ error, onLogin }: LoginPageProps) {
  return (
    <div className="container">
      <h1>WoG</h1>
      {error && <p className="error">{error}</p>}
      <GoogleButton onClick={onLogin} />
    </div>
  )
}
