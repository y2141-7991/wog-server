import type { User } from '../types'

interface ProfilePageProps {
  user: User
  onLogout: () => void
}

export function ProfilePage({ user, onLogout }: ProfilePageProps) {
  return (
    <div className="container">
      <div className="profile">
        <h1>Welcome, {user.username}</h1>
        <p className="user-id">ID: {user.id}</p>
        <button onClick={onLogout}>Sign out</button>
      </div>
    </div>
  )
}
