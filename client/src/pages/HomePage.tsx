import { useCallback, useEffect, useRef, useState } from 'react'
import { fetchEvents, fetchMyRegistration, joinEvent, leaveEvent } from '../api/event'
import { FilterBox } from '../components/FilterBox'
import type { EventFilter, EventRegistrationResponse, EventResponse } from '../types'

const EVENT_FILTER_FIELDS = [
  {
    key: 'status',
    label: 'Status',
    type: 'select' as const,
    options: [
      { value: 'OPEN', label: 'Open' },
      { value: 'DRAFT', label: 'Draft' },
      { value: 'CLOSED', label: 'Closed' },
      { value: 'CANCELLED', label: 'Cancelled' },
    ],
  },
  {
    key: 'location',
    label: 'Location',
    type: 'text' as const,
    placeholder: 'e.g. Ho Chi Minh',
  },
]

interface HomePageProps {
  currentUserId: string
  onCreateEvent: () => void
  onEditEvent: (event: EventResponse) => void
}

interface CachedPage {
  data: EventResponse[]
  hasMore: boolean
}

export function HomePage({ currentUserId, onCreateEvent, onEditEvent }: HomePageProps) {
  const [page, setPage] = useState(0)
  const [loading, setLoading] = useState(true)
  const [filters, setFilters] = useState<EventFilter>({})
  const [currentData, setCurrentData] = useState<CachedPage>({ data: [], hasMore: false })
  const cache = useRef<Map<string, CachedPage>>(new Map())
  const [registrations, setRegistrations] = useState<Record<string, EventRegistrationResponse | null>>({})
  const [actionBusy, setActionBusy] = useState<Record<string, boolean>>({})
  const [actionError, setActionError] = useState<Record<string, string>>({})

  const loadPage = useCallback((p: number, f: EventFilter) => {
    const key = `${p}:${JSON.stringify(f)}`
    const cached = cache.current.get(key)
    if (cached) {
      setCurrentData(cached)
      setLoading(false)
      return
    }

    setLoading(true)
    fetchEvents(p, 3, f).then((res) => {
      const entry = { data: res.data, hasMore: res.pagination_meta.has_more }
      cache.current.set(key, entry)
      setCurrentData(entry)
      setLoading(false)
    })
  }, [])

  useEffect(() => {
    loadPage(page, filters)
  }, [page, filters, loadPage])

  const handleFilter = (newFilters: EventFilter) => {
    if (JSON.stringify(newFilters) === JSON.stringify(filters)) return
    cache.current.clear()
    setPage(0)
    setFilters(newFilters)
  }

  const events = currentData.data

  useEffect(() => {
    events.forEach((event) => {
      if (event.organizer_id === currentUserId) return
      fetchMyRegistration(event.id).then((reg) => {
        setRegistrations((prev) => ({ ...prev, [event.id]: reg }))
      })
    })
  }, [events, currentUserId])

  const handleJoin = (eventId: string) => {
    setActionBusy((prev) => ({ ...prev, [eventId]: true }))
    setActionError((prev) => ({ ...prev, [eventId]: '' }))
    joinEvent(eventId)
      .then((res) => {
        setRegistrations((prev) => ({ ...prev, [eventId]: res.registration }))
        cache.current.clear()
        setCurrentData((prev) => ({
          ...prev,
          data: prev.data.map((e) => (e.id === eventId ? res.event : e)),
        }))
      })
      .catch((err: Error) => setActionError((prev) => ({ ...prev, [eventId]: err.message })))
      .finally(() => setActionBusy((prev) => ({ ...prev, [eventId]: false })))
  }

  const handleLeave = (eventId: string) => {
    setActionBusy((prev) => ({ ...prev, [eventId]: true }))
    setActionError((prev) => ({ ...prev, [eventId]: '' }))
    leaveEvent(eventId)
      .then((res) => {
        setRegistrations((prev) => ({ ...prev, [eventId]: null }))
        cache.current.clear()
        setCurrentData((prev) => ({
          ...prev,
          data: prev.data.map((e) => (e.id === eventId ? res.event : e)),
        }))
      })
      .catch((err: Error) => setActionError((prev) => ({ ...prev, [eventId]: err.message })))
      .finally(() => setActionBusy((prev) => ({ ...prev, [eventId]: false })))
  }

  return (
    <div className="page">
      <div className="page-header">
        <h2>My Events</h2>
        <div className="page-header-actions">
          <FilterBox fields={EVENT_FILTER_FIELDS} onApply={handleFilter} />
          <button className="create-event-btn" onClick={onCreateEvent}>+ Create Event</button>
        </div>
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
                {event.organizer_id === currentUserId ? (
                  <button className="btn-secondary btn-sm" onClick={() => onEditEvent(event)}>Edit</button>
                ) : registrations[event.id] ? (
                  <button
                    className="btn-secondary btn-sm"
                    disabled={actionBusy[event.id]}
                    onClick={() => handleLeave(event.id)}
                  >
                    {actionBusy[event.id] ? 'Leaving...' : 'Leave'}
                  </button>
                ) : (
                  <button
                    className="btn-primary btn-sm"
                    disabled={actionBusy[event.id] || event.status !== 'OPEN' || event.registered_count >= event.capacity}
                    onClick={() => handleJoin(event.id)}
                  >
                    {actionBusy[event.id]
                      ? 'Joining...'
                      : event.registered_count >= event.capacity
                        ? 'Full'
                        : Number(event.price) > 0
                          ? `Join · $${Number(event.price).toFixed(2)}`
                          : 'Join'}
                  </button>
                )}
              </div>
              {actionError[event.id] && <p className="event-action-error">{actionError[event.id]}</p>}
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
