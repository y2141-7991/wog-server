# WoG Server

Event management platform built with Rust (Axum) and React.

## Tech Stack

- **Backend:** Rust, Axum, SQLx, PostgreSQL
- **Frontend:** React 19, TypeScript, Vite
- **Auth:** Google OAuth 2.0 (PKCE), JWT

## Prerequisites

- Rust (latest stable)
- Node.js
- PostgreSQL

## Setup

1. Create a `.env` file:

```env
VIET_QR_CLIENT_ID=
VIET_QR_API_KEY=

GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=
CLIENT_URL=http://localhost:5173
JWT_SECRET=
REFRESH_TOKEN_SECRET=
ACCESS_TOKEN_EXPIRE_MINUTES=15
REFRESH_TOKEN_EXPIRE_MINUTES=10080

DATABASE_URL=
SERVER_HOST=127.0.0.1
SERVER_PORT=3000
REST_API_URL=http://localhost:5173

RUST_LOG=info,sqlx::query=info
```

2. Run the backend:

```sh
cargo run --bin server
```

3. Run the frontend (in a separate terminal):

```sh
cd client
npm install
npm run dev
```

The API runs on `http://localhost:3000` and the client on `http://localhost:5173`.

## API Docs

Visit `http://localhost:3000/scalar` for the interactive API documentation.
