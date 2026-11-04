import { useCallback, useEffect, useRef, useState } from 'react'
import { cancelOrder, checkOrder, fetchOrderStatus } from '../api/billing'
import { formatCountdown, formatVnd } from '../lib/format'
import type { OrderPayment, OrderStatus } from '../types'

const POLL_MS = 3000

interface PaymentModalProps {
  order: OrderPayment
  onClose: () => void
  /** Called once the payment is confirmed, so the page can refresh. */
  onPaid: () => void
  /** Called when the order is canceled or the user wants a fresh one. */
  onChanged: () => void
  onRetry: () => void
}

function CopyRow({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  const [copied, setCopied] = useState(false)

  async function copy() {
    try {
      await navigator.clipboard.writeText(value)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      // clipboard unavailable (insecure context): the value is still selectable
    }
  }

  return (
    <div className="pay-row">
      <div className="pay-row-text">
        <span className="text-muted">{label}</span>
        <strong className={mono ? 'pay-mono' : undefined}>{value}</strong>
      </div>
      <button className="btn-secondary btn-sm" onClick={() => void copy()} aria-label={`Copy ${label}`}>
        {copied ? 'Copied' : 'Copy'}
      </button>
    </div>
  )
}

export function PaymentModal({ order, onClose, onPaid, onChanged, onRetry }: PaymentModalProps) {
  const [status, setStatus] = useState<OrderStatus>(order.status)
  const [now, setNow] = useState(() => Date.now())
  const [error, setError] = useState<string | null>(null)
  const [checking, setChecking] = useState(false)
  const [note, setNote] = useState<string | null>(null)
  const [qrFailed, setQrFailed] = useState(false)
  const paidReported = useRef(false)

  const expiresAt = new Date(order.expires_at).getTime()
  const remaining = expiresAt - now
  const live = status === 'pending' && remaining > 0
  const effective: OrderStatus = status === 'pending' && remaining <= 0 ? 'expired' : status

  const apply = useCallback(
    (next: OrderStatus) => {
      setStatus(next)
      if (next === 'paid' && !paidReported.current) {
        paidReported.current = true
        onPaid()
      }
    },
    [onPaid],
  )

  // Countdown.
  useEffect(() => {
    if (!live) return
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [live])

  // Poll the server for the payment. Stops once the order is settled or has
  // run out of time; a late payment is still picked up on the next page load.
  useEffect(() => {
    if (!live) return
    let cancelled = false
    const timer = window.setInterval(() => {
      fetchOrderStatus(order.id)
        .then((s) => !cancelled && apply(s.status))
        .catch(() => {
          // a failed poll is retried on the next tick
        })
    }, POLL_MS)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [live, order.id, apply])

  // Close on Escape.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  async function checkNow() {
    setChecking(true)
    setError(null)
    setNote(null)
    try {
      const s = await checkOrder(order.id)
      apply(s.status)
      if (s.status === 'pending') {
        setNote('We have not received your transfer yet. Banks can take a minute; this page updates automatically.')
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not check the payment')
    } finally {
      setChecking(false)
    }
  }

  async function cancel() {
    setError(null)
    try {
      await cancelOrder(order.id)
      setStatus('canceled')
      onChanged()
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not cancel the order')
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label={`Pay for the ${order.plan_name} plan`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <h3>{order.plan_name} plan</h3>
          <button className="btn-secondary btn-sm" onClick={onClose} aria-label="Close">✕</button>
        </div>

        {effective === 'paid' && (
          <div className="pay-result">
            <p className="success">Payment received. Your {order.plan_name} plan is now active.</p>
            <button className="btn-primary" onClick={onClose}>Done</button>
          </div>
        )}

        {(effective === 'expired' || effective === 'canceled' || effective === 'failed') && (
          <div className="pay-result">
            <p className="error">
              {effective === 'canceled'
                ? 'This payment was canceled.'
                : 'This payment request expired. If you already transferred the money, it will still be applied once the bank notifies us.'}
            </p>
            {error && <p className="error">{error}</p>}
            <div className="profile-actions">
              <button className="btn-primary" onClick={onRetry}>Start a new payment</button>
              <button className="btn-secondary" onClick={onClose}>Close</button>
            </div>
          </div>
        )}

        {effective === 'pending' && (
          <>
            <div className="pay-layout">
              <div className="pay-qr">
                {qrFailed ? (
                  <div className="pay-qr-missing text-muted">QR code unavailable. Use the bank details instead.</div>
                ) : (
                  <img src={order.qr_url} alt="Bank transfer QR code" width={220} height={220} onError={() => setQrFailed(true)} />
                )}
                {!qrFailed && <span className="text-muted">Scan with your banking app</span>}
              </div>
              <div className="pay-details">
                <CopyRow label="Bank" value={order.bank_name} />
                <CopyRow label="Account number" value={order.account_number} mono />
                <CopyRow label="Account holder" value={order.account_holder} />
                <CopyRow label="Amount" value={formatVnd(order.amount_vnd)} />
                <CopyRow label="Transfer content" value={order.transfer_content} mono />
              </div>
            </div>

            <p className="pay-warning">
              Transfer the <strong>exact amount</strong> with <strong>exactly this content</strong>:{' '}
              <code>{order.transfer_content}</code>. A different amount or content cannot be matched
              to your order automatically.
            </p>

            <div className="pay-status">
              <span className="pay-spinner" aria-hidden="true" />
              <span>Waiting for your payment…</span>
              <strong className="pay-timer" aria-label="Time left">{formatCountdown(remaining)}</strong>
            </div>

            {note && <p className="text-muted">{note}</p>}
            {error && <p className="error">{error}</p>}

            <div className="profile-actions pay-actions">
              <button className="btn-primary" disabled={checking} onClick={() => void checkNow()}>
                {checking ? 'Checking…' : "I've paid, check now"}
              </button>
              <button className="btn-secondary" onClick={() => void cancel()}>Cancel order</button>
            </div>
          </>
        )}
      </div>
    </div>
  )
}
