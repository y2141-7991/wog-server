import { SubscriptionSection } from '../components/SubscriptionSection'
import type { User } from '../types'

interface ProfilePageProps {
  user: User
  onLogout: () => void
  onChangePassword: () => void
}

export function ProfilePage({ user, onLogout, onChangePassword }: ProfilePageProps) {
  return (
    <div className="detail">
      <div className="profile profile-head">
        <h2>{user.username}</h2>
        {user.avatar_url && <img src={user.avatar_url} alt={user.username} />}
        <p className="text-muted">{user.email}</p>
        <div className="profile-actions">
          <button className="btn-secondary" onClick={onChangePassword}>Change password</button>
          <button className="btn-secondary" onClick={onLogout}>Sign out</button>
        </div>
      </div>
      <SubscriptionSection />
    </div>
  )
}
