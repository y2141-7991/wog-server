export interface User {
  id: string
  username: string
  email: string
  avatar_url: string | null
  is_online: boolean
  email_verified: boolean
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
  // rust_decimal::Decimal serializes as a JSON string (e.g. "12.50"), not a number.
  price: string
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

export interface EventFilter {
  status?: string
  location?: string
}

export interface WalletResponse {
  // rust_decimal::Decimal serializes as a JSON string (e.g. "12.50"), not a number.
  balance: string
  updated_at: string
}

export interface TransferCreditsResponse {
  wallet: WalletResponse
  recipient_username: string
}

export interface EventRegistrationResponse {
  id: string
  event_id: string
  user_id: string
  // rust_decimal::Decimal serializes as a JSON string (e.g. "12.50"), not a number.
  amount_paid: string
  status: string
  registered_at: string
  cancelled_at: string | null
}

export interface EventRegistrationActionResponse {
  registration: EventRegistrationResponse
  event: EventResponse
  wallet: WalletResponse | null
}
