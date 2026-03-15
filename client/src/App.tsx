import { useState, useEffect, useCallback } from 'react'
import './App.css'
import { useAuth } from './hooks/useAuth'
import { Navbar } from './components/Navbar'
import { LoginPage } from './pages/LoginPage'
import { HomePage } from './pages/HomePage'
import { EventFormPage } from './pages/EventFormPage'
import { ProfilePage } from './pages/ProfilePage'
import type { EventResponse } from './types'

type Page = { name: 'home' } | { name: 'create-event' } | { name: 'edit-event'; event: EventResponse } | { name: 'profile' }

function pathToPage(): Page {
  const path = window.location.pathname
  if (path === '/me') return { name: 'profile' }
  if (path === '/events/new') return { name: 'create-event' }
  return { name: 'home' }
}

function App() {
  const { user, loading, error, login, logout } = useAuth()
  const [page, setPage] = useState<Page>(pathToPage)

  useEffect(() => {
    const onPop = () => setPage(pathToPage())
    window.addEventListener('popstate', onPop)
    return () => window.removeEventListener('popstate', onPop)
  }, [])

  const navigate = useCallback((p: Page) => {
    const path = p.name === 'profile' ? '/me'
      : p.name === 'create-event' ? '/events/new'
      : p.name === 'edit-event' ? `/events/${p.event.id}/edit`
      : '/'
    window.history.pushState(null, '', path)
    setPage(p)
  }, [])

  const goHome = () => navigate({ name: 'home' })

  if (loading) {
    return <div className="container"><p>Loading...</p></div>
  }

  if (!user) {
    return <LoginPage error={error} onLogin={login} />
  }


  return (
    <>
      <Navbar user={user} onLogout={logout} onProfile={() => navigate({ name: 'profile' })} onHome={goHome} />
      <main>
        {page.name === 'home' && (
          <HomePage
            onCreateEvent={() => navigate({ name: 'create-event' })}
            onEditEvent={(event) => navigate({ name: 'edit-event', event })}
          />
        )}
        {page.name === 'create-event' && <EventFormPage onBack={goHome} />}
        {page.name === 'edit-event' && <EventFormPage event={page.event} onBack={goHome} />}
        {page.name === 'profile' && <ProfilePage user={user} onLogout={logout} />}
      </main>
    </>
  )
}

export default App
