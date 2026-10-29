import { useState, useEffect, useCallback } from 'react'
import './App.css'
import { useAuth } from './hooks/useAuth'
import { Navbar } from './components/Navbar'
import { LoginPage } from './pages/LoginPage'
import { HubPage } from './pages/HubPage'
import { ApiDetailPage } from './pages/ApiDetailPage'
import { AdminPage } from './pages/AdminPage'
import { ChangePasswordPage } from './pages/ChangePasswordPage'
import { DashboardPage } from './pages/DashboardPage'
import { ProfilePage } from './pages/ProfilePage'
import { VerifyEmailPage } from './pages/VerifyEmailPage'
import { PendingVerificationPage } from './pages/PendingVerificationPage'

type Page =
  | { name: 'home' }
  | { name: 'api'; slug: string }
  | { name: 'dashboard' }
  | { name: 'admin' }
  | { name: 'profile' }
  | { name: 'change-password' }
  | { name: 'verify-email' }

function pathToPage(): Page {
  const path = window.location.pathname
  if (path === '/me') return { name: 'profile' }
  if (path === '/change-password') return { name: 'change-password' }
  if (path === '/dashboard') return { name: 'dashboard' }
  if (path === '/admin') return { name: 'admin' }
  if (path === '/verify-email') return { name: 'verify-email' }
  if (path.startsWith('/apis/')) return { name: 'api', slug: decodeURIComponent(path.slice('/apis/'.length)) }
  return { name: 'home' }
}

function pageToPath(p: Page): string {
  switch (p.name) {
    case 'profile': return '/me'
    case 'change-password': return '/change-password'
    case 'dashboard': return '/dashboard'
    case 'admin': return '/admin'
    case 'api': return `/apis/${encodeURIComponent(p.slug)}`
    default: return '/'
  }
}

function App() {
  const { user, loading, error, login, loginWithPassword, register, verifyEmail, resendVerification, changePassword, logout } = useAuth()
  const [page, setPage] = useState<Page>(pathToPage)
  const [query, setQuery] = useState('')

  useEffect(() => {
    const onPop = () => setPage(pathToPage())
    window.addEventListener('popstate', onPop)
    return () => window.removeEventListener('popstate', onPop)
  }, [])

  const navigate = useCallback((p: Page) => {
    window.history.pushState(null, '', pageToPath(p))
    setPage(p)
  }, [])

  const goHome = () => navigate({ name: 'home' })

  function search(q: string) {
    setQuery(q)
    if (page.name !== 'home') goHome()
  }

  if (page.name === 'verify-email') {
    return <VerifyEmailPage onVerify={verifyEmail} onContinue={goHome} />
  }

  if (loading) {
    return <div className="container"><p>Loading...</p></div>
  }

  if (!user) {
    return (
      <LoginPage
        error={error}
        onLogin={login}
        onLoginWithPassword={loginWithPassword}
        onRegister={register}
      />
    )
  }

  if (!user.email_verified) {
    return <PendingVerificationPage user={user} onResend={resendVerification} onLogout={logout} />
  }

  if (user.must_change_password) {
    return <ChangePasswordPage forced onChange={changePassword} onLogout={logout} />
  }

  return (
    <>
      <Navbar
        user={user}
        query={query}
        onQuery={search}
        onHome={goHome}
        onDashboard={() => navigate({ name: 'dashboard' })}
        onAdmin={user.role === 'admin' ? () => navigate({ name: 'admin' }) : undefined}
        onProfile={() => navigate({ name: 'profile' })}
        showSearch={page.name === 'home' || page.name === 'api'}
      />
      <main>
        {page.name === 'home' && (
          <HubPage query={query} onQuery={setQuery} onOpen={(slug) => navigate({ name: 'api', slug })} />
        )}
        {page.name === 'api' && (
          <ApiDetailPage slug={page.slug} onBack={goHome} onDashboard={() => navigate({ name: 'dashboard' })} />
        )}
        {page.name === 'dashboard' && <DashboardPage />}
        {page.name === 'admin' && user.role === 'admin' && <AdminPage currentUserId={user.id} />}
        {page.name === 'profile' && (
          <ProfilePage user={user} onLogout={logout} onChangePassword={() => navigate({ name: 'change-password' })} />
        )}
        {page.name === 'change-password' && (
          <ChangePasswordPage onChange={changePassword} onBack={() => navigate({ name: 'profile' })} />
        )}
      </main>
    </>
  )
}

export default App
