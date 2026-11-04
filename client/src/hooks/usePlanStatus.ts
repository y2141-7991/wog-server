import { useEffect, useState } from 'react'
import { fetchSubscription } from '../api/billing'
import { fetchUsage } from '../api/marketplace'

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
 * The user's plan and how close they are to its limit. Reloads whenever
 * `refreshKey` changes (e.g. on navigation), so a purchase shows up quickly.
 */
export function usePlanStatus(refreshKey: string): PlanStatus {
  const [status, setStatus] = useState<PlanStatus>({ planName: null, paid: false, ratio: 0 })

  useEffect(() => {
    let cancelled = false
    fetchSubscription()
      .then(async (s) => {
        if (s.plan && s.usage.quota) {
          return { planName: s.plan.name, paid: true, ratio: Math.min(1, s.usage.used / s.usage.quota) }
        }
        const usage = await fetchUsage(30)
        const ratio = Math.max(0, ...usage.by_api.map((a) => (a.monthly_quota ? a.used_this_month / a.monthly_quota : 0)))
        return { planName: 'Free', paid: false, ratio: Math.min(1, ratio) }
      })
      .then((next) => !cancelled && setStatus(next))
      .catch(() => {
        // The badge is a convenience; stay quiet if it cannot load.
      })
    return () => {
      cancelled = true
    }
  }, [refreshKey])

  return status
}
