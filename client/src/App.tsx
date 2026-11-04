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
import { ForgotPasswordPage } from './pages/ForgotPasswordPage'
import { ResetPasswordPage } from './pages/ResetPasswordPage'
import { PendingVerificationPage } from './pages/PendingVerificationPage'
import { AdminSupportPage } from './pages/AdminSupportPage'
import { SupportChatWidget } from './components/SupportChatWidget'
import { useAdminSupport } from './hooks/useAdminSupport'

type Page =
  | { name: 'home' }
  | { name: 'api'; slug: string }
  | { name: 'dashboard' }
  | { name: 'admin'; userId?: string }
  | { name: 'admin-support' }
  | { name: 'profile' }
  | { name: 'change-password' }
  | { name: 'verify-email' }
  | { name: 'forgot-password' }
  | { name: 'reset-password' }

function pathToPage(): Page {
  const path = window.location.pathname
  if (path === '/me') return { name: 'profile' }
  if (path === '/change-password') return { name: 'change-password' }
  if (path === '/dashboard') return { name: 'dashboard' }
  if (path === '/admin/support') return { name: 'admin-support' }
  if (path === '/admin') return { name: 'admin', userId: new URLSearchParams(window.location.search).get('user') ?? undefined }
  if (path === '/verify-email') return { name: 'verify-email' }
  if (path === '/forgot-password') return { name: 'forgot-password' }
  if (path === '/reset-password') return { name: 'reset-password' }
  if (path.startsWith('/apis/')) return { name: 'api', slug: decodeURIComponent(path.slice('/apis/'.length)) }
  return { name: 'home' }
}

function pageToPath(p: Page): string {
  switch (p.name) {
    case 'profile': return '/me'
    case 'change-password': return '/change-password'
    case 'dashboard': return '/dashboard'
    case 'admin': return p.userId ? `/admin?user=${encodeURIComponent(p.userId)}` : '/admin'
    case 'admin-support': return '/admin/support'
    case 'forgot-password': return '/forgot-password'
    case 'reset-password': return '/reset-password'
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

  // Admins keep one chat socket for the whole session (the nav badge is live everywhere).
  const adminChat = useAdminSupport(
    !!user && user.role === 'admin' && user.email_verified && !user.must_change_password,
  )

  const navigate = useCallback((p: Page) => {
    window.history.pushState(null, '', pageToPath(p))
    setPage(p)
  }, [])

  const goHome = () => navigate({ name: 'home' })

  if (page.name === 'verify-email') {
    return <VerifyEmailPage onVerify={verifyEmail} onContinue={goHome} />
  }

  // Reachable signed in or out: the emailed link may be opened in either state.
  if (page.name === 'forgot-password') {
    return <ForgotPasswordPage onBack={goHome} />
  }
  if (page.name === 'reset-password') {
    // A full load, so a stale signed-in state cannot survive the reset.
    return <ResetPasswordPage onForgot={() => navigate({ name: 'forgot-password' })} onDone={() => window.location.assign('/')} />
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
        onForgotPassword={() => navigate({ name: 'forgot-password' })}
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
        onHome={goHome}
        onDashboard={() => navigate({ name: 'dashboard' })}
        onAdmin={user.role === 'admin' ? () => navigate({ name: 'admin' }) : undefined}
        onSupport={user.role === 'admin' ? () => navigate({ name: 'admin-support' }) : undefined}
        supportUnread={adminChat.totalUnread}
        onProfile={() => navigate({ name: 'profile' })}
        planRefreshKey={page.name}
      />
      <main>
        {page.name === 'home' && (
          <HubPage query={query} onQuery={setQuery} onOpen={(slug) => navigate({ name: 'api', slug })} />
        )}
        {page.name === 'api' && (
          <ApiDetailPage slug={page.slug} onBack={goHome} onDashboard={() => navigate({ name: 'dashboard' })} />
        )}
        {page.name === 'dashboard' && <DashboardPage onUpgrade={() => navigate({ name: 'profile' })} />}
        {page.name === 'admin' && user.role === 'admin' && <AdminPage currentUserId={user.id} initialUserId={page.userId} />}
        {page.name === 'admin-support' && user.role === 'admin' && (
          <AdminSupportPage
            support={adminChat}
            currentUserId={user.id}
            onOpenUser={(userId) => navigate({ name: 'admin', userId })}
          />
        )}
        {page.name === 'profile' && (
          <ProfilePage user={user} onLogout={logout} onChangePassword={() => navigate({ name: 'change-password' })} />
        )}
        {page.name === 'change-password' && (
          <ChangePasswordPage onChange={changePassword} onBack={() => navigate({ name: 'profile' })} />
        )}
      </main>
      {user.role !== 'admin' && <SupportChatWidget user={user} onAuthLost={() => window.location.reload()} />}
    </>
  )
}

export default App
