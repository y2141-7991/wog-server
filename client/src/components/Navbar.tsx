import { NEAR_LIMIT, usePlanStatus } from '../hooks/usePlanStatus'
import { useAdminChatStore } from '../stores/adminChat'
import { useAuthStore } from '../stores/auth'
import { useNavigationStore } from '../stores/navigation'
import { BrandLogo } from './BrandLogo'

/**
 * Reads everything it shows from the stores: who is signed in, the plan badge (shared
 * queries), the admin's unread count and where the links go. It takes no props.
 */
export function Navbar() {
  const user = useAuthStore((s) => s.user)
  const navigate = useNavigationStore((s) => s.navigate)
  const goHome = useNavigationStore((s) => s.goHome)
  const supportUnread = useAdminChatStore((s) => s.totalUnread)
  const plan = usePlanStatus()
  if (!user) return null
  const isAdmin = user.role === 'admin'
  return (
    <nav className="navbar">
      <button className="navbar-title" onClick={goHome} aria-label="WoG API Hub, home">
        <BrandLogo size={30} />
        <b>WoG</b> <span>API Hub</span>
      </button>
      <div className="navbar-right">
        <button className="nav-link" onClick={() => navigate({ name: 'dashboard' })}>Dashboard</button>
        {isAdmin && <button className="nav-link" onClick={() => navigate({ name: 'admin' })}>Admin</button>}
        {isAdmin && (
          <button className="nav-link" onClick={() => navigate({ name: 'admin-support' })} aria-label={supportUnread > 0 ? `Support inbox, ${supportUnread} unread` : 'Support inbox'}>
            Support
            {supportUnread > 0 && <span className="chat-badge chat-badge-inline">{supportUnread > 99 ? '99+' : supportUnread}</span>}
          </button>
        )}
        {plan.planName && (
          <button
            className={`plan-pill${plan.paid ? ' plan-pill-paid' : ''}${plan.ratio >= NEAR_LIMIT ? ' plan-pill-hot' : ''}`}
            onClick={() => navigate({ name: 'profile' })}
            title={plan.ratio >= NEAR_LIMIT ? `You have used ${Math.round(plan.ratio * 100)}% of your quota` : 'Manage your plan'}
          >
            {plan.planName}
            {plan.ratio >= NEAR_LIMIT && <span> · Upgrade</span>}
          </button>
        )}
        <button className="avatar-btn" onClick={() => navigate({ name: 'profile' })} title={user.username}>
          {user.avatar_url ? (
            <img src={user.avatar_url} alt={user.username} />
          ) : (
            <span className="avatar-fallback">{user.username[0].toUpperCase()}</span>
          )}
        </button>
      </div>
    </nav>
  )
}
