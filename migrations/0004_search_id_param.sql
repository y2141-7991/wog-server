-- The search API pages with two values, not one: the `cursor` and the
-- `search_id` of the previous response. The client passes both back to get the
-- next page; the server no longer pages on its own.
--
-- Idempotent: safe on a fresh database (after 0001) and when run twice.
UPDATE apis
SET params = params || '[{"name":"search_id","type":"string","required":false,"description":"Search session id: the search_id of the previous response (omit for the first page)","example":"20261030143012ABCDEF0123456789AB"}]'::jsonb,
    description = 'Search TikTok videos by keyword or hashtag, 10 posts per page. Pass next_cursor as cursor and search_id from the response to get the next page.',
    example_response = jsonb_set(
        COALESCE(example_response, '{}'::jsonb),
        '{search_id}',
        '"20261030143012ABCDEF0123456789AB"'
    ),
    updated_at = NOW()
WHERE slug = 'tiktok-search'
  AND NOT (params @> '[{"name":"search_id"}]'::jsonb);
