import { useEffect, useRef } from 'react'
import type { User } from '../types'

interface NavbarProps {
  user: User
  query: string
  onQuery: (q: string) => void
  onHome: () => void
  onDashboard: () => void
  /** Only given to admins. */
  onAdmin?: () => void
  onProfile: () => void
  /** Hidden on pages where searching APIs makes no sense (e.g. the profile). */
  showSearch?: boolean
}

export function Navbar({ user, query, onQuery, onHome, onDashboard, onAdmin, onProfile, showSearch = true }: NavbarProps) {
  const input = useRef<HTMLInputElement>(null)

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        input.current?.focus()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  return (
    <nav className="navbar">
      <button className="navbar-title" onClick={onHome}>WoG <span>API Hub</span></button>
      {showSearch ? (
        <div className="searchbar">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
            <circle cx="11" cy="11" r="7" />
            <path d="M21 21l-4.3-4.3" />
          </svg>
          <input
            ref={input}
            value={query}
            onChange={(e) => onQuery(e.target.value)}
            placeholder="Search APIs"
            spellCheck={false}
          />
          <kbd>⌘K</kbd>
        </div>
      ) : (
        <div />
      )}
      <div className="navbar-right">
        <button className="nav-link" onClick={onDashboard}>Dashboard</button>
        {onAdmin && <button className="nav-link" onClick={onAdmin}>Admin</button>}
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
