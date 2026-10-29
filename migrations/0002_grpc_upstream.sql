-- The gateway now reaches the crawler over gRPC, which addresses a method
-- by name instead of an HTTP path.
ALTER TABLE apis RENAME COLUMN upstream_path TO upstream_method;
UPDATE apis SET upstream_method = substring(upstream_method FROM '^/v1/(.*)$')
WHERE upstream_method LIKE '/v1/%';
