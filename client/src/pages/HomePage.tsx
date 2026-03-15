import { useCallback, useEffect, useRef, useState } from 'react'
import { fetchEvents } from '../api/event'
import type { EventResponse } from '../types'

interface HomePageProps {
  onCreateEvent: () => void
  onEditEvent: (event: EventResponse) => void
}

interface CachedPage {
  data: EventResponse[]
  hasMore: boolean
}

export function HomePage({ onCreateEvent, onEditEvent }: HomePageProps) {
  const [page, setPage] = useState(0)
  const [loading, setLoading] = useState(true)
  const [currentData, setCurrentData] = useState<CachedPage>({ data: [], hasMore: false })
  const cache = useRef<Map<number, CachedPage>>(new Map())

  const loadPage = useCallback((p: number) => {
    const cached = cache.current.get(p)
    if (cached) {
      setCurrentData(cached)
      setLoading(false)
      return
    }

    setLoading(true)
    fetchEvents(p).then((res) => {
      const entry = { data: res.data, hasMore: res.pagination_meta.has_more }
      cache.current.set(p, entry)
      setCurrentData(entry)
      setLoading(false)
    })
  }, [])

  useEffect(() => {
    loadPage(page)
  }, [page, loadPage])

  const events = currentData.data

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
              <div className="event-actions">
                <button className="btn-secondary btn-sm" onClick={() => onEditEvent(event)}>Edit</button>
              </div>
            </div>
          ))}
        </div>
      )}

      {!loading && events.length > 0 && (
        <div className="pagination">
          <button disabled={page === 0} onClick={() => setPage(p => p - 1)}>Previous</button>
          <span>Page {page + 1}</span>
          <button disabled={!currentData.hasMore} onClick={() => setPage(p => p + 1)}>Next</button>
        </div>
      )}
    </div>
  )
}
