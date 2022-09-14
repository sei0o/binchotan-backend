# binchotan

[日本語](https://github.com/eniehack/binchotan-backend/blob/main/README.ja.md)

## Install

targeted OSes: *nix (but tested only Linux)

### Requirements

* Rust

### Usage

1. Clone this repository
2. Set the environment variable `TWITTER_CLIENT_ID` and `TWITTER_CLIENT_SECRET` to your OAuth2 (not 1.0a) credentials, or use `.env`.
3. `cargo run`
4. Use the mock frontend to see it works.

### Run as a systemd unit(user unit)

1. Clone repository: `git clone https://github.com/sei0o/binchotan-backend.git && cd binchotan-backend`
1. Build: `cargo build --locked --release`
2. Install a binary: recomend `~/.local/bin` or `/usr/local/bin`
3. Install a`.service` file: `cp resources/binchotan.service ~/.local/share/systemd/user/binchotan.service`
4. Modify `~/.local/share/systemd/user/binchotan.service`
  * Open `~/.local/share/systemd/user/binchotan.service` in an editor.
  * Modify `ExecStart` option
  * Paste `TWITTER_CLIENT_ID` and `TWITTER_CLIENT_SECRET` from Twitter Developer Portal to `Environment` option
  * Modify `Environment`'s  other variable properly.
5. `systemctl daemon-reload`
6. `systemctl --user start binchotan`

## Configuration

### Environment variables

describe below options in `.env`file or envitonment variables.

* `BINCHOTAN_CONFIG_FILE`: specify a config file's path.
* `BINCHOTAN_TWITTER_CLIENT_ID`: OAuth 2.0 Client ID got from Twitter Developer Portal
* `BINCHOTAN_TWITTER_CLIENT_SECRET`: OAuth 2.0 Client Secret got from Twitter Developer Portal
* `BINCHOTAN_SOCKET_PATH`: specify socket's path using RPC connections.
* `BINCHOTAN_CACHE_PATH`: specify cache file's path 
* `BINCHOTAN_FILTER_DIR`: specify a directory's path where contains a filter

## Manage accounts

Please use [binchotan-frontend-sample](https://github.com/sei0o/binchotan-frontend-sample).

## Frontends

* [binchotan-frontend-sample](https://github.com/sei0o/binchotan-frontend-sample): frontend for manage accounts. CLI app.
* [minichotan](https://github.com/sei0o/minichotan): minimal ui, handles multiple accounts. desktop client.

## Contributing

* Fork it (https://gitlab.com/sei0o/binchotan-backend/fork)
* Create your feature branch (`git checkout -b my-new-feature`)
* Commit your changes (`git commit -am 'Add some feature'`)
* Push to the branch (`git push origin my-new-feature`)
* Create a new Pull Request
