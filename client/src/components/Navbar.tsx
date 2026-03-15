import type { User } from '../types'

interface NavbarProps {
  user: User
  onLogout: () => void
  onProfile: () => void
  onHome: () => void
}

export function Navbar({ user, onProfile, onHome }: NavbarProps) {
  return (
    <nav className="navbar">
      <span className="navbar-title" onClick={onHome} style={{ cursor: 'pointer' }}>WoG</span>
      <div className="navbar-right">
        <button className="avatar-btn" onClick={onProfile}>
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
