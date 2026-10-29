import type {
  ApiKey,
  CreateApiKeyResponse,
  MarketplaceApi,
  UsageResponse,
} from '../types'

const API = import.meta.env.VITE_API_URL || ''
const BASE = `${API}/api/v1/marketplace`

async function request<T>(path: string, init: RequestInit, failure: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`, { credentials: 'include', ...init })
  if (res.status === 204) return undefined as T
  const body = await res.json().catch(() => ({}))
  if (!res.ok) throw new Error(body.message || failure)
  return body as T
}

export const fetchApis = () =>
  request<MarketplaceApi[]>('/apis', {}, 'Could not load the API catalog')

export const fetchApi = (slug: string) =>
  request<MarketplaceApi>(`/apis/${encodeURIComponent(slug)}`, {}, 'Could not load this API')

export const fetchKeys = () => request<ApiKey[]>('/keys', {}, 'Could not load your API keys')

export const createKey = (name: string) =>
  request<CreateApiKeyResponse>(
    '/keys',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name }),
    },
    'Could not create the API key',
  )

export const revokeKey = (id: string) =>
  request<void>(`/keys/${id}`, { method: 'DELETE' }, 'Could not revoke the API key')

export const fetchUsage = (days = 30) =>
  request<UsageResponse>(`/usage?days=${days}`, {}, 'Could not load usage')

/** Call a gateway endpoint with an API key, as an API customer would. */
export async function callGateway(
  gatewayPath: string,
  apiKey: string,
  params: Record<string, string>,
): Promise<{ status: number; body: unknown; ms: number }> {
  const query = new URLSearchParams(
    Object.entries(params).filter(([, v]) => v.trim() !== ''),
  ).toString()
  const started = performance.now()
  const res = await fetch(`${API}${gatewayPath}${query ? `?${query}` : ''}`, {
    headers: { 'X-API-Key': apiKey },
  })
  const body = await res.json().catch(() => null)
  return { status: res.status, body, ms: Math.round(performance.now() - started) }
}

// A key is only shown once at creation, so keep the last one for this
// browser tab to save pasting it into every test request.
const KEY_STORAGE = 'wog_api_key'

export function getSavedKey(): string {
  try {
    return sessionStorage.getItem(KEY_STORAGE) ?? ''
  } catch {
    return ''
  }
}

export function saveKey(key: string) {
  try {
    sessionStorage.setItem(KEY_STORAGE, key)
  } catch {
    // storage unavailable: the key just has to be pasted again
  }
}
