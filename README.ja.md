# binchotan

十分な表現力を持つフィルタ機構を備えた Twitter クライアント

## Install

対象 OS: Unix 系(動作が確認できる OS は Linux のみ)

### 必要なパッケージ

* Rust

### とりあえず動かしてみる

1. リポジトリのクローン: `git clone https://github.com/sei0o/binchotan-backend.git && cd binchotan-backend`
2. 環境変数の設定
   * 環境変数 `BINCHOTAN_TWITTER_CLIENT_ID` と `BINCHOTAN_TWITTER_CLIENT_SECRET` を Twitter Developer Portal からペーストする
     * OAuth1.0a ではなく、OAuth2 の Client ID と Client Secret を用いる
   * または、`.env`ファイルを作成し、ペースト: `cp .env.template .env`
3. 実行: `cargo run`
4. frontend からアカウント設定を行う

### systemd unit(user unit)として動作させる 

1. リポジトリの clone: `git clone https://github.com/sei0o/binchotan-backend.git && cd binchotan-backend`
1. build: `cargo build --locked --release`
2. バイナリの配置: `~/.local/bin`または`/usr/local/bin`に配置する
3. `.service`ファイルの配置: `cp resources/binchotan.service ~/.local/share/systemd/user/binchotan.service`
4. `~/.local/share/systemd/user/binchotan.service`の修正
  * `~/.local/share/systemd/user/binchotan.service`をエディタで開く
  * `.service`ファイルの`ExecStart`にあるバイナリの絶対パスと、配置したバイナリの絶対パスが一致しているか確認する。していなければ修正する。
  * `.service`ファイルの`Environment`にある`BINCHOTAN_TWITTER_CLIENT_ID`と`BINCHOTAN_TWITTER_CLIENT_SECRET`にペーストする
  * その他`Environment`も適宜修正する
5. `systemctl daemon-reload`
6. `systemctl --user start binchotan`

### ArchLinux

[AUR](https://aur.archlinux.org/packages/binchotan-backend-git)から入手できます。

## 設定

### 環境変数

環境変数、または`.env`ファイルに以下の項目を記述します

* `BINCHOTAN_CONFIG_FILE`: binchotan の設定ファイルのパスを指定します
* `BINCHOTAN_TWITTER_CLIENT_ID`: Twitter Developer Portal から入手した OAuth 2.0 Client ID を指定します
* `BINCHOTAN_TWITTER_CLIENT_SECRET`: Twitter Developer Portal から入手する OAuth 2.0 Client Secret を指定します
* `BINCHOTAN_SOCKET_PATH`: RPC で用いる unix domain socket のパスを指定します
* `BINCHOTAN_CACHE_PATH`: キャッシュファイルの場所を指定します
* `BINCHOTAN_FILTER_DIR`: Filter が入っているディレクトリを指定します

## アカウントの管理

[binchotan-frontend-sample](https://github.com/sei0o/binchotan-frontend-sample)を用いて設定します。

## フロントエンド

* [binchotan-frontend-sample](https://github.com/sei0o/binchotan-frontend-sample): アカウントの設定などを行うフロントエンド。CLI アプリケーション。
* [minichotan](https://github.com/sei0o/minichotan): 複数アカウントを運用できるミニマルなフロントエンド。デスクトップアプリケーション。
