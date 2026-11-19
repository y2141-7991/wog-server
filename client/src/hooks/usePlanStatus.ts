import { useSubscription, useUsage } from '../api/queries'

/** How full a quota must be before we suggest upgrading. */
export const NEAR_LIMIT = 0.8

export interface PlanStatus {
  /** `null` until loaded. */
  planName: string | null
  paid: boolean
  /** The fullest quota, 0..1: the plan's pool, or the busiest API on the free tier. */
  ratio: number
}

/**
 * The user's plan and how close they are to its limit, derived from the shared
 * subscription and usage queries, so the navbar and the dashboard always agree.
 * The badge is a convenience; while either query is missing or failed it stays empty.
 */
export function usePlanStatus(): PlanStatus {
  const subscription = useSubscription()
  const s = subscription.data
  const paid = !!(s?.plan && s.usage.quota)
  // Free tier: the fullest per-API quota is the busiest one. Only fetched when needed.
  const usage = useUsage(30, !!s && !paid)

  if (!s) return { planName: null, paid: false, ratio: 0 }
  if (s.plan && s.usage.quota) {
    return { planName: s.plan.name, paid: true, ratio: Math.min(1, s.usage.used / s.usage.quota) }
  }
  if (!usage.data) return { planName: null, paid: false, ratio: 0 }
  const ratio = Math.max(0, ...usage.data.by_api.map((a) => (a.monthly_quota ? a.used_this_month / a.monthly_quota : 0)))
  return { planName: 'Free', paid: false, ratio: Math.min(1, ratio) }
}
