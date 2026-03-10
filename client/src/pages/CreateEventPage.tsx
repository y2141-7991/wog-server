import { useState } from 'react'
import { createEvent } from '../api/event'
import type { EventCreateRequest } from '../types'

interface CreateEventPageProps {
  onBack: () => void
}

export function CreateEventPage({ onBack }: CreateEventPageProps) {
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)

  async function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault()
    setError(null)
    setSubmitting(true)

    const form = new FormData(e.currentTarget)
    const data: EventCreateRequest = {
      title: form.get('title') as string,
      description: (form.get('description') as string) || undefined,
      price: Number(form.get('price')),
      capacity: Number(form.get('capacity')),
      status: form.get('status') as string,
      start_time: new Date(form.get('start_time') as string).toISOString(),
      end_time: new Date(form.get('end_time') as string).toISOString(),
      location: (form.get('location') as string) || undefined,
    }

    try {
      await createEvent(data)
      onBack()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Something went wrong')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="page">
      <div className="form-card">
        <h2>Create Event</h2>
        {error && <p className="error">{error}</p>}
        <form onSubmit={handleSubmit}>
          <label>
            Title
            <input name="title" required />
          </label>
          <label>
            Description
            <textarea name="description" rows={3} />
          </label>
          <div className="form-row">
            <label>
              Price
              <input name="price" type="number" min="0" step="0.01" defaultValue="0" required />
            </label>
            <label>
              Capacity
              <input name="capacity" type="number" min="1" defaultValue="100" required />
            </label>
          </div>
          <div className="form-row">
            <label>
              Start
              <input name="start_time" type="datetime-local" required />
            </label>
            <label>
              End
              <input name="end_time" type="datetime-local" required />
            </label>
          </div>
          <label>
            Location
            <input name="location" />
          </label>
          <label>
            Status
            <select name="status" defaultValue="OPEN">
              <option value="DRAFT">Draft</option>
              <option value="OPEN">Open</option>
            </select>
          </label>
          <div className="form-actions">
            <button type="button" className="btn-secondary" onClick={onBack}>Cancel</button>
            <button type="submit" disabled={submitting}>
              {submitting ? 'Creating...' : 'Create Event'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
