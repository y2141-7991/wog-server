import { useEffect, useState } from 'react'
import { fetchEvents } from '../api/event'
import type { EventResponse } from '../types'

interface HomePageProps {
  onCreateEvent: () => void
}

export function HomePage({ onCreateEvent }: HomePageProps) {
  const [events, setEvents] = useState<EventResponse[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    fetchEvents()
      .then(setEvents)
      .finally(() => setLoading(false))
  }, [])

  return (
    <div className="page">
      <div className="page-header">
        <h2>My Events</h2>
        <button className="create-event-btn" onClick={onCreateEvent}>+ Create Event</button>
      </div>

      {loading && <p className="text-muted">Loading events...</p>}

      {!loading && events.length === 0 && (
        <div className="empty-state">
          <p>No events yet.</p>
          <button className="create-event-btn" onClick={onCreateEvent}>Create your first event</button>
        </div>
      )}

      {events.length > 0 && (
        <div className="event-list">
          {events.map(event => (
            <div key={event.id} className="event-card">
              <div className="event-card-header">
                <h3>{event.title}</h3>
                <span className={`event-status status-${event.status.toLowerCase()}`}>
                  {event.status}
                </span>
              </div>
              {event.description && <p className="event-desc">{event.description}</p>}
              <div className="event-meta">
                {event.location && <span>{event.location}</span>}
                <span>{new Date(event.start_time).toLocaleDateString()}</span>
                <span>{event.registered_count} / {event.capacity}</span>
                <span>${Number(event.price).toFixed(2)}</span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
