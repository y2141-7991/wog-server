export interface User {
  id: string
  username: string
  email: string
  avatar_url: string | null
  is_online: boolean
}

export interface EventCreateRequest {
  title: string
  description?: string
  price: number
  capacity: number
  status: string
  start_time: string
  end_time: string
  location?: string
}

export interface EventResponse {
  id: string
  title: string
  description: string | null
  organizer_id: string
  price: number
  capacity: number
  registered_count: number
  status: string
  start_time: string
  end_time: string
  location: string | null
}

export interface PaginationMeta {
  next_page: number
  has_more: boolean
}

export interface EventListResponse {
  data: EventResponse[]
  pagination_meta: PaginationMeta
}
