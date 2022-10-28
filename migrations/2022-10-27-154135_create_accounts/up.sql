create table accounts (
  id serial primary key,
  twitter_id text unique not null,
  access_token text unique not null,
  refresh_token text unique not null,
  session_key text unique,
  owned_by integer references accounts (id)
);