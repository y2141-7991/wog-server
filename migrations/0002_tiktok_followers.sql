-- ══════════════════════════════════════════════════════════════
-- Catalog: TikTok followers and following lists (gateway methods
-- `followers` and `following`). Both page with an opaque cursor.
-- ══════════════════════════════════════════════════════════════
INSERT INTO apis (slug, name, description, provider, category, gateway_path, upstream_method, params, example_response, rate_limit_per_minute, monthly_quota) VALUES
(
  'tiktok-followers',
  'TikTok Followers',
  'Accounts that follow a TikTok user, newest first, 20 per page, with the total follower count. Pass next_cursor as cursor to get the next page.',
  'TikTok',
  'Social',
  '/gateway/v1/tiktok/followers',
  'followers',
  '[{"name":"user_id","type":"string","required":true,"description":"Numeric TikTok user ID (the author.id of any post)","example":"7434378166225060920"},{"name":"cursor","type":"string","required":false,"description":"Page position: the next_cursor of the previous response (omit for the first page)","example":"eyJtYXhfY3Vyc29yIjoxNzk1MTQ1MTA4fQ=="}]',
  '{"user_id":"7434378166225060920","followers":[{"uid":"6800000000000000000","unique_id":"someone","nickname":"Someone","follower_count":120,"following_count":80,"aweme_count":14}],"total":45569,"next_cursor":"eyJtYXhfY3Vyc29yIjoxNzk1MTQ1MTA4fQ==","has_more":true}',
  10, 50
),
(
  'tiktok-following',
  'TikTok Following',
  'Accounts a TikTok user follows, newest first, 20 per page, with the total count. Pass next_cursor as cursor to get the next page.',
  'TikTok',
  'Social',
  '/gateway/v1/tiktok/following',
  'following',
  '[{"name":"user_id","type":"string","required":true,"description":"Numeric TikTok user ID (the author.id of any post)","example":"7434378166225060920"},{"name":"cursor","type":"string","required":false,"description":"Page position: the next_cursor of the previous response (omit for the first page)","example":"eyJtYXhfY3Vyc29yIjoxNzM0MTcyMjM3fQ=="}]',
  '{"user_id":"7434378166225060920","following":[{"uid":"6800000000000000001","unique_id":"creator","nickname":"Creator","follower_count":250000,"following_count":40,"aweme_count":310}],"total":3,"next_cursor":null,"has_more":false}',
  10, 50
)
ON CONFLICT (slug) DO NOTHING;
