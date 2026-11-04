import type { MarketplaceApi } from '../types'

// The catalog is public and changes rarely. Keep the last copy in the browser so a
// reload (F5) can paint it immediately while a fresh copy is fetched in the
// background (stale-while-revalidate).
const KEY = 'wog_catalog_v1'

export function readCatalog(): MarketplaceApi[] | null {
  try {
    const raw = localStorage.getItem(KEY)
    const parsed: unknown = raw ? JSON.parse(raw) : null
    return Array.isArray(parsed) ? (parsed as MarketplaceApi[]) : null
  } catch {
    return null
  }
}

export function writeCatalog(apis: MarketplaceApi[]) {
  try {
    localStorage.setItem(KEY, JSON.stringify(apis))
  } catch {
    // storage full or unavailable: the next load just fetches
  }
}
