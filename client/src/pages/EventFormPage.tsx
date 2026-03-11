import { useState } from 'react'
import { createEvent, updateEvent } from '../api/event'
import type { EventCreateRequest, EventResponse } from '../types'

function toLocalDatetime(iso: string): string {
  const d = new Date(iso)
  d.setMinutes(d.getMinutes() - d.getTimezoneOffset())
  return d.toISOString().slice(0, 16)
}

interface EventFormPageProps {
  event?: EventResponse
  onBack: () => void
}

export function EventFormPage({ event, onBack }: EventFormPageProps) {
  const isEdit = !!event
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
      if (isEdit) {
        await updateEvent(event.id, data)
      } else {
        await createEvent(data)
      }
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
        <h2>{isEdit ? 'Edit Event' : 'Create Event'}</h2>
        {error && <p className="error">{error}</p>}
        <form onSubmit={handleSubmit}>
          <label>
            Title
            <input name="title" defaultValue={event?.title} required />
          </label>
          <label>
            Description
            <textarea name="description" rows={3} defaultValue={event?.description ?? ''} />
          </label>
          <div className="form-row">
            <label>
              Price
              <input name="price" type="number" min="0" step="0.01" defaultValue={event?.price ?? 0} required />
            </label>
            <label>
              Capacity
              <input name="capacity" type="number" min="1" defaultValue={event?.capacity ?? 100} required />
            </label>
          </div>
          <div className="form-row">
            <label>
              Start
              <input name="start_time" type="datetime-local" defaultValue={event ? toLocalDatetime(event.start_time) : ''} required />
            </label>
            <label>
              End
              <input name="end_time" type="datetime-local" defaultValue={event ? toLocalDatetime(event.end_time) : ''} required />
            </label>
          </div>
          <label>
            Location
            <input name="location" defaultValue={event?.location ?? ''} />
          </label>
          <label>
            Status
            <select name="status" defaultValue={event?.status ?? 'OPEN'}>
              <option value="DRAFT">Draft</option>
              <option value="OPEN">Open</option>
              <option value="CLOSED">Closed</option>
              <option value="CANCELLED">Cancelled</option>
            </select>
          </label>
          <div className="form-actions">
            <button type="button" className="btn-secondary" onClick={onBack}>Cancel</button>
            <button type="submit" disabled={submitting}>
              {submitting ? 'Saving...' : isEdit ? 'Save Changes' : 'Create Event'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
