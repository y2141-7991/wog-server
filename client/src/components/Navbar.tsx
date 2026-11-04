import { NEAR_LIMIT, usePlanStatus } from '../hooks/usePlanStatus'
import type { User } from '../types'

interface NavbarProps {
  user: User
  onHome: () => void
  onDashboard: () => void
  /** Only given to admins. */
  onAdmin?: () => void
  /** Only given to admins: opens the support inbox. */
  onSupport?: () => void
  /** Unread customer messages, shown as a badge on the Support link. */
  supportUnread?: number
  onProfile: () => void
  /** Changes when the plan badge should reload (e.g. the current page). */
  planRefreshKey?: string
}

export function Navbar({ user, onHome, onDashboard, onAdmin, onSupport, supportUnread = 0, onProfile, planRefreshKey = '' }: NavbarProps) {
  const plan = usePlanStatus(planRefreshKey)
  return (
    <nav className="navbar">
      <button className="navbar-title" onClick={onHome}>WoG <span>API Hub</span></button>
      <div className="navbar-right">
        <button className="nav-link" onClick={onDashboard}>Dashboard</button>
        {onAdmin && <button className="nav-link" onClick={onAdmin}>Admin</button>}
        {onSupport && (
          <button className="nav-link" onClick={onSupport} aria-label={supportUnread > 0 ? `Support inbox, ${supportUnread} unread` : 'Support inbox'}>
            Support
            {supportUnread > 0 && <span className="chat-badge chat-badge-inline">{supportUnread > 99 ? '99+' : supportUnread}</span>}
          </button>
        )}
        {plan.planName && (
          <button
            className={`plan-pill${plan.paid ? ' plan-pill-paid' : ''}${plan.ratio >= NEAR_LIMIT ? ' plan-pill-hot' : ''}`}
            onClick={onProfile}
            title={plan.ratio >= NEAR_LIMIT ? `You have used ${Math.round(plan.ratio * 100)}% of your quota` : 'Manage your plan'}
          >
            {plan.planName}
            {plan.ratio >= NEAR_LIMIT && <span> · Upgrade</span>}
          </button>
        )}
        <button className="avatar-btn" onClick={onProfile} title={user.username}>
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
