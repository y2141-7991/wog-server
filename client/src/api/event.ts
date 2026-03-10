import type { EventCreateRequest, EventResponse } from '../types'

const EVENT_URL = '/api/v1/event'

export async function createEvent(data: EventCreateRequest): Promise<EventResponse> {
  const res = await fetch(EVENT_URL, {
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
