import { useState } from 'react'
import './App.css'
import { useAuth } from './hooks/useAuth'
import { Navbar } from './components/Navbar'
import { LoginPage } from './pages/LoginPage'
import { HomePage } from './pages/HomePage'
import { EventFormPage } from './pages/EventFormPage'
import type { EventResponse } from './types'

type Page = { name: 'home' } | { name: 'create-event' } | { name: 'edit-event'; event: EventResponse }

function App() {
  const { user, loading, error, login, logout } = useAuth()
  const [page, setPage] = useState<Page>({ name: 'home' })

  if (loading) {
    return <div className="container"><p>Loading...</p></div>
  }

  if (!user) {
    return <LoginPage error={error} onLogin={login} />
  }

  const goHome = () => setPage({ name: 'home' })

  return (
    <>
      <Navbar user={user} onLogout={logout} />
      <main>
        {page.name === 'home' && (
          <HomePage
            onCreateEvent={() => setPage({ name: 'create-event' })}
            onEditEvent={(event) => setPage({ name: 'edit-event', event })}
          />
        )}
        {page.name === 'create-event' && <EventFormPage onBack={goHome} />}
        {page.name === 'edit-event' && <EventFormPage event={page.event} onBack={goHome} />}
      </main>
    </>
  )
}

export default App
