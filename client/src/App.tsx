import { useState } from 'react'
import './App.css'
import { useAuth } from './hooks/useAuth'
import { Navbar } from './components/Navbar'
import { LoginPage } from './pages/LoginPage'
import { ProfilePage } from './pages/ProfilePage'
import { CreateEventPage } from './pages/CreateEventPage'

type Page = 'home' | 'create-event'

function App() {
  const { user, loading, error, login, logout } = useAuth()
  const [page, setPage] = useState<Page>('home')

  if (loading) {
    return <div className="container"><p>Loading...</p></div>
  }

  if (!user) {
    return <LoginPage error={error} onLogin={login} />
  }

  return (
    <>
      <Navbar
        user={user}
        onLogout={logout}
        onCreateEvent={() => setPage('create-event')}
      />
      <main>
        {page === 'home' && <ProfilePage user={user} onLogout={logout} />}
        {page === 'create-event' && <CreateEventPage onBack={() => setPage('home')} />}
      </main>
    </>
  )
}

export default App
