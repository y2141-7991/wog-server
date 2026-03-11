import { useState, useRef, useEffect } from 'react'
import type { User } from '../types'

interface NavbarProps {
  user: User
  onLogout: () => void
}

export function Navbar({ user, onLogout }: NavbarProps) {
  const [open, setOpen] = useState(false)
  const menuRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    document.addEventListener('mousedown', handleClick)
    return () => document.removeEventListener('mousedown', handleClick)
  }, [])

  return (
    <nav className="navbar">
      <span className="navbar-title">WoG</span>
      <div className="navbar-right">
        <div className="avatar-menu" ref={menuRef}>
          <button className="avatar-btn" onClick={() => setOpen(!open)}>
            {user.avatar_url ? (
              <img src={user.avatar_url} alt={user.username} />
            ) : (
              <span className="avatar-fallback">{user.username[0].toUpperCase()}</span>
            )}
          </button>
          {open && (
            <div className="avatar-dropdown">
              <div className="dropdown-header">
                {user.avatar_url && <img src={user.avatar_url} alt={user.username} />}
                <div>
                  <div className="dropdown-name">{user.username}</div>
                  <div className="dropdown-email">{user.email}</div>
                </div>
              </div>
              <hr />
              <button className="dropdown-item" onClick={onLogout}>Sign out</button>
            </div>
          )}
        </div>
      </div>
    </nav>
  )
}
