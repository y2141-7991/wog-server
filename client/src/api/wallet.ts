import type { TransferCreditsResponse, WalletResponse } from '../types'

const API = import.meta.env.VITE_API_URL || ''

const WALLET_URL = `${API}/api/v1/wallet`
const TRANSFER_URL = `${API}/api/v1/wallet/transfer`

export async function fetchWallet(): Promise<WalletResponse> {
  const res = await fetch(WALLET_URL, { credentials: 'include' })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(body.message || 'Could not load wallet')
  }
  return body as WalletResponse
}

/**
 * Send `amount` credits directly to another user's wallet (by username or
 * email). Purely an in-app balance move -- this is the only way credits
 * move between accounts.
 */
export async function transferCredits(
  recipient: string,
  amount: string,
): Promise<TransferCreditsResponse> {
  const res = await fetch(TRANSFER_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ recipient, amount }),
    credentials: 'include',
  })
  const body = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(body.message || 'Could not send credits')
  }
  return body as TransferCreditsResponse
}
