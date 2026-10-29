# WoG Server

API marketplace built with Rust (Axum) and React: sign up, create an API key, and call metered TikTok data APIs through a gateway. Admins get a page showing every user's usage.

## Tech Stack

- **Backend:** Rust, Axum, SQLx, PostgreSQL, gRPC (tonic) to the crawler
- **Frontend:** React 19, TypeScript, Vite
- **Auth:** email + password, Google OAuth 2.0 (PKCE), JWT

## Prerequisites

- Rust (latest stable)
- Node.js
- PostgreSQL
- The `social-track-pipeline` repo, which provides the crawler the gateway calls (see [Crawler](#crawler-social-track-pipeline))

## Setup

1. Create an empty database, then a `.env` file in the repo root:

```env
DATABASE_URL=postgres://user:password@localhost:5432/wog_db
SERVER_HOST=127.0.0.1
SERVER_PORT=3000
CLIENT_URL=http://localhost:5173
REST_API_URL=http://localhost:5173

JWT_SECRET=
REFRESH_TOKEN_SECRET=
ACCESS_TOKEN_EXPIRE_MINUTES=15
REFRESH_TOKEN_EXPIRE_MINUTES=10080

GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=

# Crawler (see below). Both sides must use the same secret.
UPSTREAM_URL=http://127.0.0.1:50051
UPSTREAM_SECRET=

# Optional: SMTP for the email-verification message
SMTP_HOST=
SMTP_PORT=587
SMTP_USERNAME=
SMTP_PASSWORD=
SMTP_FROM_EMAIL=
SMTP_FROM_NAME=WoG

RUST_LOG=info,sqlx::query=info
```

2. Run the backend. Migrations in `migrations/` are embedded into the binary and applied on startup, so rebuild after changing them:

```sh
cargo run --bin server
```

3. Run the frontend (in a separate terminal):

```sh
cd client
npm install
npm run dev
```

The API runs on `http://localhost:3000` and the client on `http://localhost:5173`. The Vite dev server proxies `/api/*` and `/gateway/*` to the backend.

> The schema and the API catalog are seeded by `0001_init.sql`. While there is no deployed database, that file is edited in place; after pulling a change to it, drop and recreate the dev database. SQLx refuses to start on a database that was migrated with an earlier version of the file.

## API Docs

Visit `http://localhost:3000/scalar` for the interactive API documentation.

## Default admin

On startup, if no admin exists, the server creates one:

| username | password |
| -------- | -------- |
| `admin`  | `admin`  |

(Its email is `admin@wog.local`; you can sign in with the username or the email.) **Change the password right away** from the profile page (click your avatar, then *Change password*). `admin` / `admin` is only suitable for local development.

If a regular account already uses the username `admin` or that email, the server logs a warning and does not create the admin.

## Admin page

Admins see an **Admin** link in the navbar (`/admin`):

- **Usage:** requests, error rate and latency over the last 7, 30 or 90 days; a daily request chart; traffic per API; a searchable, paginated table of every user. Click a user for their per-API usage, daily chart and API keys.
- **Audit log:** role changes and the admin bootstrap.
- Admins can promote or demote other users from the user detail view. You cannot change your own role, and the last admin cannot be demoted.

Admin status is read from the database on every request, so a demotion takes effect immediately. The backend routes live under `/api/v1/admin`.

## API Marketplace (TikTok data)

WoG exposes a metered gateway at `GET /gateway/v1/tiktok/{post,comments,search,profile-posts}`, authenticated with an `X-API-Key` header. Create keys on the dashboard page; the API detail page has a **Test endpoint** form and cURL / Python examples.

| Endpoint        | Parameters                         | Notes                                   |
| --------------- | ---------------------------------- | --------------------------------------- |
| `post`          | `post_id` (required)               | Details and stats of one video          |
| `comments`      | `post_id` (required), `cursor`     | 20 comments per page                    |
| `search`        | `keyword` (required), `cursor`     | 10 posts per page                       |
| `profile-posts` | `user_id` (required)               | Recent videos; `user_id` is numeric     |

### Pagination

`comments` and `search` return one page per call plus `next_cursor` and `has_more`. Leave `cursor` out for the first page and pass the previous response's `next_cursor` to get the next one; `next_cursor` is `null` on the last page.

```sh
curl -H "X-API-Key: YOUR_API_KEY" \
  "http://localhost:3000/gateway/v1/tiktok/search?keyword=%23CS2"

curl -H "X-API-Key: YOUR_API_KEY" \
  "http://localhost:3000/gateway/v1/tiktok/search?keyword=%23CS2&cursor=10"
```

```python
import requests

response = requests.get(
    "http://localhost:3000/gateway/v1/tiktok/search",
    headers={"X-API-Key": "YOUR_API_KEY"},
    params={"keyword": "#CS2"},
)
print(response.json())
```

### Limits and metering

Each API has a per-minute rate limit and a monthly quota (shown on its detail page). A call is counted with its HTTP status; a status of 400 or above counts as an error in the admin usage numbers. Failures that are not the caller's doing (the crawler is down or cannot reach TikTok) return 502 and are not counted.

## Crawler (`social-track-pipeline`)

Gateway calls are forwarded over gRPC to the `tiktok_gateway` service in the `social-track-pipeline` repo. Start it with the same environment as the Celery workers:

```sh
UPSTREAM_SECRET=<the same string> python -m tiktok_gateway.server   # GRPC_ADDR defaults to 127.0.0.1:50051
```

- Set `UPSTREAM_URL` and `UPSTREAM_SECRET` in the WoG `.env`, and the same `UPSTREAM_SECRET` in the gateway's environment. Calls without the secret are rejected.
- The contract is `crates/wog-infras/proto/crawler.proto`; keep it in sync with `social-track-pipeline/tiktok_gateway/crawler.proto`.
- Ctrl-C or SIGTERM stops the gateway gracefully (in-flight crawls get a few seconds to finish).

## Project layout

- `crates/wog-api`: Axum routes and the server binary
- `crates/wog-infras`: services, repositories, models, the gRPC crawler client
- `crates/wog-config`: request/response DTOs
- `crates/wog-middleware`: app state and the auth / admin extractors
- `crates/wog-oauth`: Google OAuth
- `client`: React app
- `migrations`: SQL schema and catalog seed
