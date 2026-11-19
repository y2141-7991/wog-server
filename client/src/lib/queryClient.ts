import { QueryClient } from '@tanstack/react-query'

/**
 * One cache for everything fetched from the server (catalog, plan, usage, keys…).
 *
 * `staleTime` is how long a copy counts as fresh: a page that mounts inside that
 * window reuses it instead of refetching, which is what keeps the navbar badge,
 * dashboard and API page in step without each asking the server again.
 * Anything that changes the data on purpose (buying a plan, minting a key,
 * calling an API) invalidates the affected keys instead of waiting it out.
 */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: false,
      refetchOnWindowFocus: true,
    },
  },
})
