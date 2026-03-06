import './App.css'
import { useAuth } from './hooks/useAuth'
import { LoginPage } from './pages/LoginPage'
import { ProfilePage } from './pages/ProfilePage'

function App() {
  const { user, loading, error, login, logout } = useAuth()

  if (loading) {
    return <div className="container"><p>Loading...</p></div>
  }

  if (!user) {
    return <LoginPage error={error} onLogin={login} />
  }

  return <ProfilePage user={user} onLogout={logout} />
}

export default App
