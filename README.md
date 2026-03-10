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
DATABASE_URL=postgres://user:password@localhost:5432/wog
SERVER_HOST=0.0.0.0
SERVER_PORT=3000
CLIENT_URL=http://localhost:5173
JWT_SECRET=your-secret-key
JWT_EXPIRATION_HOURS=24
GOOGLE_CLIENT_ID=your-google-client-id
GOOGLE_CLIENT_SECRET=your-google-client-secret
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
