import { create } from 'zustand'

export type Page =
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

interface NavigationState {
  page: Page
  /** The hub's search box; kept here so it survives opening an API and coming back. */
  query: string
  navigate: (page: Page) => void
  goHome: () => void
  setQuery: (query: string) => void
}

export const useNavigationStore = create<NavigationState>()((set) => ({
  page: pathToPage(),
  query: '',
  navigate: (page) => {
    window.history.pushState(null, '', pageToPath(page))
    set({ page })
  },
  goHome: () => {
    window.history.pushState(null, '', '/')
    set({ page: { name: 'home' } })
  },
  setQuery: (query) => set({ query }),
}))

// Back/forward buttons change the URL without going through `navigate`.
// The store lives as long as the page does, so this listener is never removed.
window.addEventListener('popstate', () => useNavigationStore.setState({ page: pathToPage() }))
