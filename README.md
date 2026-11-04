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

# Optional: paid plans via SePay bank transfer (see "Subscriptions and payments").
# Leave these empty and the rest of the app works; buying a plan is just switched off.
SEPAY_WEBHOOK_API_KEY=
SEPAY_API_TOKEN=
SEPAY_BANK_NAME=
SEPAY_ACCOUNT_NUMBER=
SEPAY_ACCOUNT_HOLDER=
SEPAY_PAYMENT_CODE_PREFIX=WOG
ORDER_TTL_MINUTES=30

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

> Subscriptions and payments live in their own migration, `0003_subscriptions.sql` (idempotent, applies cleanly on a fresh database or on one already at `0002`), so they do not require recreating the database.
>
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
| `search`        | `keyword` (required), `cursor`, `search_id` | 10 posts per page              |
| `profile-posts` | `user_id` (required)               | Recent videos; `user_id` is numeric     |

### Pagination

`comments` and `search` return one page per call plus `next_cursor` and `has_more`; the server never fetches further pages by itself. Leave `cursor` out for the first page and pass the previous response's `next_cursor` to get the next one; `next_cursor` is `null` on the last page.

`search` also returns a `search_id` (TikTok's id of the search session). Send it back together with the cursor as the `search_id` parameter to continue the same search; it is `null` on the last page.

```sh
curl -H "X-API-Key: YOUR_API_KEY" \
  "http://localhost:3000/gateway/v1/tiktok/search?keyword=%23CS2"

curl -H "X-API-Key: YOUR_API_KEY" \
  "http://localhost:3000/gateway/v1/tiktok/search?keyword=%23CS2&cursor=10&search_id=<search_id of the previous response>"
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

## Subscriptions and payments

Users are on the **free** tier until they buy one of three plans (`basic`, `pro`, `ultra`). Plans are rows in the `plans` table, so prices, quotas and rate limits are edited with SQL, no deploy needed. The seeded numbers are **placeholders**:

| Plan  | Price (VND / 30 days) | Requests / month | Requests / minute |
| ----- | --------------------- | ---------------- | ----------------- |
| basic | 99,000                | 10,000           | 30                |
| pro   | 299,000               | 100,000          | 120               |
| ultra | 799,000               | 1,000,000        | 600               |

```sql
UPDATE plans SET price_vnd = 149000, monthly_quota = 20000 WHERE slug = 'basic';
```

### How limits apply at the gateway

- **No active subscription (free):** each API's own `rate_limit_per_minute` and `monthly_quota` apply, with the quota counted per API over the UTC calendar month. Nothing changed for existing users.
- **Active subscription:** the plan's numbers *replace* the per-API ones. The plan's `rate_limit_per_minute` applies per API key across all APIs, and `monthly_quota` is one pool shared by all APIs, counted from the start of the current billing period.
- A subscription whose period has ended is treated as free **at request time**, so a late sweeper never grants extra access. A background task (every 60 s) marks such rows `expired` and stale pending orders `expired`.

### Paying

`POST /api/v1/billing/orders {"plan": "pro"}` creates an order (30 minutes by default) with a short payment code such as `WOG7K2M9QXA`, and returns the bank details plus a QR URL that pre-fills the amount and content. The customer transfers **the exact amount with exactly that content**; SePay notifies `POST /api/v1/webhooks/sepay`, which activates the plan. The profile page shows the QR, a countdown, and polls the order status every 3 seconds.

Rules: the email must be verified; creating an order again returns the pending one; a plan can be bought again only in the last 7 days of the running period (it then extends from the period end); a lower plan is refused while a higher one is active; a higher plan is an upgrade that starts when paid (the old period is not refunded).

| Endpoint | Purpose |
| -------- | ------- |
| `GET /api/v1/billing/plans` | Public plan list |
| `GET /api/v1/billing/subscription` | Current plan, period end, usage vs quota |
| `POST /api/v1/billing/orders` | Start (or resume) a payment |
| `GET /api/v1/billing/orders`, `/orders/{id}`, `/orders/{id}/status` | History, payment details, cheap status poll |
| `POST /api/v1/billing/orders/{id}/cancel` | Cancel a pending order |
| `POST /api/v1/billing/orders/{id}/check` | "I've paid, check now": asks SePay for recent transfers (needs `SEPAY_API_TOKEN`; rate limited) |
| `POST /api/v1/webhooks/sepay` | SePay's webhook |
| `GET /api/v1/admin/payments/orders`, `/events` | Admin, read-only |

### Payment handling guarantees

- The only thing that activates a plan is a bank notification (webhook, or the reconciliation fallback). Nothing the browser sends can mark an order paid.
- One code path settles a transfer, in one DB transaction that locks the order row. A transfer id is `UNIQUE`, so redelivery (SePay retries up to 8 times) and two parallel deliveries activate exactly once.
- `transferAmount >= price` is required. **Underpaid:** recorded, order stays pending, flagged for review. **Overpaid:** activated, the excess is recorded. **Paid after expiry:** still activated (the money arrived). **Paid after cancel:** not applied, flagged for review. **Unknown code:** `200`, warning logged, stored. **Outgoing transfer:** ignored.
- Every delivery, whatever the outcome, is stored in `payment_events` with the raw payload. The admin page's *Payments* tab lists orders and these events, with a "needs review" filter for underpayments and unmatched money.
- `{"success": true}` is returned only after the transaction commits; a database error returns 5xx so SePay retries. A wrong key returns 401.

### Configuration

| Variable | Required | Meaning |
| -------- | -------- | ------- |
| `SEPAY_WEBHOOK_API_KEY` | yes | Secret SePay sends as `Authorization: Apikey <key>`; compared in constant time |
| `SEPAY_BANK_NAME` | yes | Bank shown to the customer and used in the QR (e.g. `Vietcombank`) |
| `SEPAY_ACCOUNT_NUMBER` | yes | Receiving account number |
| `SEPAY_ACCOUNT_HOLDER` | yes | Account holder name |
| `SEPAY_API_TOKEN` | no | SePay API token for reconciliation and the "check now" button; without it the webhook is the only way a payment arrives |
| `SEPAY_PAYMENT_CODE_PREFIX` | no | Default `WOG` (2-5 letters or digits) |
| `ORDER_TTL_MINUTES` | no | Default `30` |
| `SEPAY_QR_BASE_URL`, `SEPAY_API_BASE` | no | Override the QR image host (default `https://vietqr.app/img`) or the SePay API base (default `https://my.sepay.vn/userapi`) |

If any required variable is missing the server still starts, logs which ones are missing, and the billing endpoints that need SePay answer `503`. Plans and the free tier keep working.

### Setting up SePay

1. In the SePay dashboard link the bank account that matches `SEPAY_BANK_NAME` / `SEPAY_ACCOUNT_NUMBER`.
2. **Company configuration → Payment code structure:** prefix `WOG` (same as `SEPAY_PAYMENT_CODE_PREFIX`), suffix length min 8 / max 8, characters letters and digits. WoG also finds its code in the raw transfer content, so a missing `code` field from SePay is harmless.
3. **Webhooks → Add webhook:** URL `https://<your-host>/api/v1/webhooks/sepay`, event *money in*, authentication **API Key** with the same value as `SEPAY_WEBHOOK_API_KEY`, content type JSON.
4. Optional: create an API token (*API access*) and set `SEPAY_API_TOKEN`.

### Testing locally

SePay must reach your machine to deliver webhooks, so for real transfers expose the server with a tunnel and use that URL in step 3:

```sh
ngrok http 3000                       # or: cloudflared tunnel --url http://localhost:3000
```

Without a tunnel (or a bank), simulate SePay with the included script. Create an order in the UI (or via the API), then send the matching transfer:

```sh
scripts/simulate-sepay-webhook.sh WOG7K2M9QXA 299000            # exact payment
scripts/simulate-sepay-webhook.sh WOG7K2M9QXA 100000            # underpayment: stays pending, flagged
scripts/simulate-sepay-webhook.sh WOG7K2M9QXA 299000 --id 42    # replaying an id is a harmless duplicate
scripts/simulate-sepay-webhook.sh WOG7K2M9QXA 299000 --out      # outgoing transfer: ignored
scripts/simulate-sepay-webhook.sh WOG7K2M9QXA 299000 --key bad  # 401
```

It reads `SEPAY_WEBHOOK_API_KEY` from the environment or `.env`; `WOG_URL` (default `http://localhost:3000`) selects the server. The equivalent curl:

```sh
curl -X POST http://localhost:3000/api/v1/webhooks/sepay \
  -H "Authorization: Apikey $SEPAY_WEBHOOK_API_KEY" -H "Content-Type: application/json" \
  -d '{"id": 92704, "gateway": "Vietcombank", "transactionDate": "2026-10-08 14:02:37",
       "accountNumber": "0123456789", "code": null, "content": "WOG7K2M9QXA",
       "transferType": "in", "transferAmount": 299000, "accumulated": 0,
       "subAccount": null, "referenceCode": "FT123", "description": "WOG7K2M9QXA"}'
```

Tests: `DATABASE_URL=postgres://… cargo test` (the billing integration tests create a throwaway database per test, so the user needs `CREATEDB`).

### Not included yet

Auto-renewal, refunds, invoices / VAT, coupons. Other follow-ups: verify an HMAC signature if SePay offers one for your webhook type, restrict the webhook to SePay's IP ranges at the proxy, and move the reconciliation call to SePay's v2 API if the v1 `userapi` is retired.

## Support chat

Customers talk to admins through a chat bubble (bottom-right, signed-in non-admin users only). Admins answer from the inbox at **Support** in the nav (`/admin/support`). Messages travel over one WebSocket and are stored in Postgres first, then delivered.

**How it behaves**

- Every user has **one conversation**. Their first message creates it. An admin can close it; if the user writes again, the **same conversation is reopened**, so each user has one continuous thread.
- Users only ever receive events for their own conversation. Admins get an inbox update for every conversation and the messages of the conversations they have opened.
- Several tabs and several admins work at the same time. Presence (online dot) and unread counts are kept for the inbox and the nav badge.
- The widget loads the conversation with one REST call. It opens the WebSocket right away only if the user already has an open conversation, otherwise on first open. Admins stay connected while signed in so the badge is live.
- After a drop the client reconnects with exponential backoff and jitter (1s to 30s), fetches what it missed with `GET .../messages?after=<last id>`, and resends unacknowledged messages. A resend with the same `client_msg_id` is not stored twice.

**Configuration** (`.env`)

| Variable | Meaning |
| --- | --- |
| `CLIENT_URL`, `REST_API_URL` | Always allowed as WebSocket origins. |
| `ALLOWED_ORIGINS` | Extra allowed browser origins, comma separated, e.g. `http://localhost:5173,https://admin.example.com`. Empty by default. |

**Auth and origin.** The socket is authenticated by the same HttpOnly `token` cookie as every other route, so no token ever appears in a URL or a log (a ticket endpoint would add a moving part for no gain: the cookie is already sent on the same-origin handshake). `Authorization: Bearer <access token>` also works for non-browser clients. The handshake answers `401` without a valid session, `403` for an unverified account, a pending password change, or an `Origin` that is not allowed (cross-site WebSocket hijacking). Beyond the connection cap the upgrade succeeds, the server sends an `error` event with code `too_many_connections` and closes the socket. A missing `Origin` is allowed because only browsers always send it. Roles are read from the database, never from the token: admin-only events and every admin send are re-checked, and a demoted admin is disconnected within 30 seconds. The server closes a socket when its access token expires, and the client reconnects.

**Limits**

| Limit | Value |
| --- | --- |
| Message body | 1 to 2000 characters (counted as characters, trimmed) |
| Frame size | 16 KB; larger frames close the connection |
| `send_message` per connection | 10 per 10 s (`rate_limited` error, connection stays open) |
| All frames per connection | 60 per 10 s; 20 rate-limit hits in a row close it |
| Connections per account | 5 users, 10 admins |
| Heartbeat | server ping every 30 s, closed after 90 s of silence |
| Slow consumer | 256 queued frames per connection; beyond that it is closed (1013) |

**Single instance.** The hub (`wog_infras::support::hub`) keeps connections in this process's memory, so run **one server instance**. It sits behind the `SupportHub` trait: to scale out, implement it on Redis pub/sub or Postgres `LISTEN/NOTIFY` (publish to local connections and to the other instances; share presence) without touching the rest of the chat code.

**Protocol.** One JSON object per text frame with a `type` field.

Client to server:

| `type` | Fields | Notes |
| --- | --- | --- |
| `send_message` | `conversation_id?`, `body`, `client_msg_id` | Users omit `conversation_id`. Admins must send it. `client_msg_id` (1-64 chars) makes the send idempotent. |
| `typing` | `conversation_id?`, `is_typing` | Delivered to the other side only. |
| `mark_read` | `conversation_id?` | Marks the other side's messages read and clears your unread counter. |
| `ping` | | Answered with `pong`. |
| `join_conversation` / `leave_conversation` | `conversation_id` | Admin only: start or stop receiving that conversation's `message`, `typing` and `read` events. Join first, then fetch history over REST. Sending a reply also joins. |
| `close_conversation` / `reopen_conversation` / `assign_to_me` | `conversation_id` | Admin only. |

Server to client:

| `type` | Fields | Notes |
| --- | --- | --- |
| `ready` | `user_id`, `role`, `conversation_id` | First frame. Refetch missed messages here. |
| `message` | `message` | A stored message. Dedupe by `message.id` / `client_msg_id`. |
| `ack` | `client_msg_id`, `message_id`, `conversation_id`, `created_at`, `duplicate` | Your send is stored. |
| `typing` | `conversation_id`, `user_id`, `role`, `is_typing` | |
| `read` | `conversation_id`, `by`, `at` | The other side read up to now. |
| `conversation_updated` | `conversation`, `total_unread` | Inbox row (preview, unread counts, status, user, plan, online). `total_unread` is for admins' nav badge. |
| `presence` | `user_id`, `online` | Admins only. |
| `error` | `code`, `message`, `client_msg_id?` | Codes: `bad_request`, `forbidden`, `not_found`, `message_too_long`, `empty_message`, `invalid_client_msg_id`, `conversation_closed`, `rate_limited`, `too_many_connections`, `internal`. The connection stays open unless stated. |
| `pong` | | |

REST (all under `/api/v1/support`, see `/scalar`):

| Route | Who | Notes |
| --- | --- | --- |
| `GET /conversation?before&after&limit` | user | Own conversation (open, else the latest closed) and a page of messages; `conversation` is `null` before the first message. |
| `GET /conversations?status&q&page` | admin | Inbox, newest first; `status` = `open`, `closed` or `all`; `q` matches username or email; also returns `total_unread`. |
| `GET /conversations/{id}` | admin | One inbox row. |
| `GET /conversations/{id}/messages?before&after&limit` | owner or admin | Oldest first, `limit` 1-100 (default 50). Anyone else gets `404`. |
| `POST /conversations/{id}/close` and `/reopen` | admin | Idempotent; both sides are notified live. |

**Reverse proxy.** The socket is a normal HTTP upgrade on `/api/v1/support/ws`. Vite's dev proxy has `ws: true` for it. Caddy and Cloudflare Tunnel forward WebSockets as they are. With nginx add `proxy_http_version 1.1; proxy_set_header Upgrade $http_upgrade; proxy_set_header Connection "upgrade"; proxy_read_timeout 3600s;` for that location. Forward the original `Origin` header unchanged.

**Try it from a terminal.** Sign in as each person to get an access token, then start one `support_cli` per person (it prints every event as one line of JSON):

```bash
login() { curl -s localhost:3000/api/v1/auth/login -H 'content-type: application/json' \
  -d "{\"email\":\"$1\",\"password\":\"$2\"}" | jq -r .access_token; }
# Users must be verified: UPDATE users SET email_verified = true WHERE ...
cargo run -p wog-api --bin support_cli -- --token "$(login admin@wog.local <admin password>)"   # terminal 1
cargo run -p wog-api --bin support_cli -- --token "$(login alice@example.com <password>)"       # terminal 2
cargo run -p wog-api --bin support_cli -- --token "$(login bob@example.com <password>)"         # terminal 3
```

In terminal 2 type a message. The admin terminal prints a `conversation_updated` with the conversation id; in terminal 1 run `/join <id>` and `/to <id> hello alice`. Terminal 3 (bob) sees nothing of it. More commands are listed at the top of `crates/wog-api/src/bin/support_cli.rs` (`/typing`, `/read`, `/close`, `/raw {...}` for malformed input). The CLI speaks plain `ws://`; for `wss://` use `websocat -H "Authorization: Bearer $TOKEN" wss://host/api/v1/support/ws`.

**Tests.** `cargo test -p wog-api --test support_chat` starts the real server per test on an ephemeral port with its own database (`DATABASE_URL` must point at a Postgres where you can create databases).

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
- `crates/payment-client`: SePay client (QR URL, transaction lookup) behind a `PaymentProvider` trait
- `crates/wog-api/src/bin/support_cli.rs`: terminal client for the support chat
- `scripts`: development helpers (SePay webhook simulator)
- `client`: React app
- `migrations`: SQL schema and catalog seed
