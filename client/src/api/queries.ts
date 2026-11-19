import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query'
import { createOrder, fetchOrder, fetchOrders, fetchPlans, fetchSubscription } from './billing'
import { readCatalog, writeCatalog } from './catalogCache'
import { createKey, fetchApi, fetchApis, fetchKeys, fetchUsage, revokeKey } from './marketplace'
import type { MarketplaceApi, PlanSlug } from '../types'

/** Every cache key in one place, so invalidation cannot drift from the hooks. */
export const qk = {
  catalog: ['catalog'] as const,
  api: (slug: string) => ['api', slug] as const,
  keys: ['keys'] as const,
  usage: (days: number) => ['usage', days] as const,
  usageAll: ['usage'] as const,
  subscription: ['subscription'] as const,
  plans: ['plans'] as const,
  orders: ['orders'] as const,
}

// The catalog is public and changes rarely. The copy kept in localStorage is
// handed to the cache as already-stale data: a reload (F5) paints it at once and
// the fetch below replaces it (stale-while-revalidate).
export function useCatalog() {
  return useQuery({
    queryKey: qk.catalog,
    queryFn: async () => {
      const apis = await fetchApis()
      writeCatalog(apis)
      return apis
    },
    initialData: () => readCatalog() ?? undefined,
    initialDataUpdatedAt: 0,
  })
}

export function useApi(slug: string) {
  const qc = useQueryClient()
  return useQuery({
    queryKey: qk.api(slug),
    queryFn: () => fetchApi(slug),
    // Open instantly from whichever catalog copy we already have.
    initialData: () => (qc.getQueryData<MarketplaceApi[]>(qk.catalog) ?? readCatalog())?.find((a) => a.slug === slug),
    initialDataUpdatedAt: 0,
  })
}

export const useKeys = () => useQuery({ queryKey: qk.keys, queryFn: fetchKeys })

export const useUsage = (days = 30, enabled = true) =>
  useQuery({ queryKey: qk.usage(days), queryFn: () => fetchUsage(days), enabled })

export const useSubscription = () => useQuery({ queryKey: qk.subscription, queryFn: fetchSubscription })

export const usePlans = () => useQuery({ queryKey: qk.plans, queryFn: fetchPlans })

export const useOrders = () => useQuery({ queryKey: qk.orders, queryFn: fetchOrders })

export function useCreateKey() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (name: string) => createKey(name),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.keys }),
  })
}

export function useRevokeKey() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => revokeKey(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.keys }),
  })
}

export function useCreateOrder() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (plan: PlanSlug) => createOrder(plan),
    onSuccess: () => invalidateBilling(qc),
  })
}

export const resumeOrder = (id: string) => fetchOrder(id)

/** A payment settled, was cancelled, or a plan changed: plan, orders and quota all move. */
export function invalidateBilling(qc: QueryClient) {
  return Promise.all([
    qc.invalidateQueries({ queryKey: qk.subscription }),
    qc.invalidateQueries({ queryKey: qk.orders }),
    qc.invalidateQueries({ queryKey: qk.usageAll }),
  ])
}
