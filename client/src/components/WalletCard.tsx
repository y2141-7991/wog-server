import { useEffect, useState } from 'react'
import { fetchWallet, transferCredits } from '../api/wallet'
import type { WalletResponse } from '../types'

type LoadState = 'loading' | 'ready' | 'error'

export function WalletCard() {
  const [wallet, setWallet] = useState<WalletResponse | null>(null)
  const [loadState, setLoadState] = useState<LoadState>('loading')

  const [transferRecipient, setTransferRecipient] = useState('')
  const [transferAmount, setTransferAmount] = useState('')
  const [transferring, setTransferring] = useState(false)
  const [transferError, setTransferError] = useState<string | null>(null)
  const [transferSuccess, setTransferSuccess] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    fetchWallet()
      .then((w) => {
        if (!cancelled) {
          setWallet(w)
          setLoadState('ready')
        }
      })
      .catch(() => {
        if (!cancelled) setLoadState('error')
      })
    return () => {
      cancelled = true
    }
  }, [])

  async function handleTransfer(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault()
    setTransferError(null)
    setTransferSuccess(null)

    const recipient = transferRecipient.trim()
    if (!recipient) {
      setTransferError("Enter the recipient's username or email")
      return
    }
    if (!transferAmount.trim() || Number(transferAmount) <= 0) {
      setTransferError('Enter an amount greater than zero')
      return
    }

    setTransferring(true)
    try {
      const result = await transferCredits(recipient, transferAmount.trim())
      setWallet(result.wallet)
      setTransferSuccess(`Sent to ${result.recipient_username}.`)
      setTransferRecipient('')
      setTransferAmount('')
    } catch (err) {
      setTransferError(err instanceof Error ? err.message : 'Could not send credits')
    } finally {
      setTransferring(false)
    }
  }

  return (
    <div className="form-card wallet-card">
      <h2>Wallet</h2>

      <p className="wallet-balance">
        {loadState === 'loading' && 'Loading balance...'}
        {loadState === 'error' && 'Could not load your wallet balance.'}
        {loadState === 'ready' && wallet && `${Number(wallet.balance).toFixed(2)} credits`}
      </p>

      <div className="wallet-transfer">
        <h3>Send credits to another user</h3>
        <p className="wallet-hint">
          Moves credits straight from your balance to theirs inside the app.
        </p>
        {transferError && <p className="error">{transferError}</p>}
        {transferSuccess && <p className="verify-sent">{transferSuccess}</p>}
        <form onSubmit={handleTransfer}>
          <label>
            Recipient username or email
            <input
              value={transferRecipient}
              onChange={(e) => setTransferRecipient(e.target.value)}
              placeholder="username or email"
              spellCheck={false}
              autoComplete="off"
            />
          </label>
          <label>
            Amount (credits)
            <input
              value={transferAmount}
              onChange={(e) => setTransferAmount(e.target.value)}
              placeholder="0.00"
              inputMode="decimal"
              autoComplete="off"
            />
          </label>
          <button type="submit" disabled={transferring}>
            {transferring ? 'Sending...' : 'Send credits'}
          </button>
        </form>
      </div>
    </div>
  )
}
