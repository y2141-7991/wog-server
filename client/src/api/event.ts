import type { EventCreateRequest, EventListResponse, EventResponse } from '../types'

const API = import.meta.env.VITE_API_URL || ''
const EVENTS_URL = `${API}/api/v1/events`

export async function fetchEvents(page = 0, perPage = 3): Promise<EventListResponse> {
  const res = await fetch(`${EVENTS_URL}?page=${page}&per_page=${perPage}`, { credentials: 'include' })
  if (!res.ok) return { data: [], pagination_meta: { next_page: 0, has_more: false } }
  return res.json()
}

export async function updateEvent(id: string, data: EventCreateRequest): Promise<EventResponse> {
  const res = await fetch(`${EVENTS_URL}/${id}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
    credentials: 'include',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ message: 'Failed to update event' }))
    throw new Error(err.message)
  }
  return res.json()
}

export async function createEvent(data: EventCreateRequest): Promise<EventResponse> {
  const res = await fetch(EVENTS_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
    credentials: 'include',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ message: 'Failed to create event' }))
    throw new Error(err.message)
  }
  return res.json()
}
