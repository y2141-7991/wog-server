import './App.css'
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
import { useAuthStore } from './stores/auth'
import { useNavigationStore } from './stores/navigation'

function App() {
  const user = useAuthStore((s) => s.user)
  const loading = useAuthStore((s) => s.loading)
  const error = useAuthStore((s) => s.error)
  const { login, loginWithPassword, register, verifyEmail, resendVerification, changePassword, logout } = useAuthStore.getState()
  const page = useNavigationStore((s) => s.page)
  const navigate = useNavigationStore((s) => s.navigate)
  const goHome = useNavigationStore((s) => s.goHome)

  // Admins keep one chat socket for the whole session (the nav badge is live everywhere).
  useAdminSupport(!!user && user.role === 'admin' && user.email_verified && !user.must_change_password)

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
      <Navbar />
      <main>
        {page.name === 'home' && <HubPage />}
        {page.name === 'api' && (
          <ApiDetailPage slug={page.slug} onBack={goHome} onDashboard={() => navigate({ name: 'dashboard' })} />
        )}
        {page.name === 'dashboard' && <DashboardPage onUpgrade={() => navigate({ name: 'profile' })} />}
        {page.name === 'admin' && user.role === 'admin' && <AdminPage currentUserId={user.id} initialUserId={page.userId} />}
        {page.name === 'admin-support' && user.role === 'admin' && (
          <AdminSupportPage
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
