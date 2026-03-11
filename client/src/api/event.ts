import type { EventCreateRequest, EventResponse } from '../types'

const EVENTS_URL = '/api/v1/events'

export async function fetchEvents(): Promise<EventResponse[]> {
  const res = await fetch(EVENTS_URL)
  if (!res.ok) return []
  const json = await res.json()
  return json.data
}

export async function updateEvent(id: string, data: EventCreateRequest): Promise<EventResponse> {
  const res = await fetch(`${EVENTS_URL}/${id}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
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
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ message: 'Failed to create event' }))
    throw new Error(err.message)
  }
  return res.json()
}
