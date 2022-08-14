# プロトコル

フロントエンドとバックエンドの間の通信は、JSON-RPC v2によって行います。フロントエンドがJSON-RPCのクライアント、バックエンドはサーバとして機能します。

バックエンドはフロントエンドからのリクエストに応じて、フロントエンドの代わりにTwitter APIから情報を取得します。取得した情報はJSON-RPCのレスポンスとして返却されます。バックエンドにおける処理に応じて、リクエストはプレーンリクエスト (plain/pass-through requests) とフィルタリング付きリクエスト (filtered requests) の2種類に分類されます。

## プレーンリクエスト

プレーンリクエストは、特にフィルタなどの処理が必要ないエンドポイントを呼ぶときに使います。バックエンドはフロントエンドからの情報に認証情報を付加してから、そのままTwitter APIに転送し、得たレスポンスをそのままフロントエンドに返却します。

```json
// プレーンリクエスト
{
  "jsonrpc": "2.0",
  "method": "binchotan.v0.plain_request", // pass through, proxy
  "params": {
    "method": "GET",
    "endpoint": "/lists/tweets",
    "api_params": {
        ...
    }
  },
  "id": "hogehoge" // arbitrary string
}

// レスポンス
{
  "jsonrpc": "2.0",
  "result": {
    "meta": {
      "api_calls_remaining": 24,
      "api_calls_reset": 18,
    },
    "body": { // Twitter API からのレスポンスがそのまま入る
      "data": {
        ...
      },
      "meta": {
        ...
      }
    }
  },
  "id": "hogehoge"
}
```

## フィルタリング付きリクエスト

タイムラインに関連するいくつかのエンドポイントについては、専用のRPCメソッドにリクエストすることで、フィルタを通した情報を取得することができます。レスポンスの形式はプレーンリクエストの場合と同様です。

```json
// ホームタイムラインを取得するリクエスト
{
  "jsonrpc": "2.0",
  "method": "binchotan.v0.home_timeline",
  "params": {
      "max_results": 1024,
  },
  "id": "hogehoge"
}

// レスポンス
{
  "jsonrpc": "2.0",
  "result": {
    "meta": {
      "api_calls_remaining": 24,
      "api_calls_reset": 18,
    },
    "body": { // Twitter API からのレスポンスがそのまま入る
      "data": {
        ...
      },
      "meta": {
        ...
      }
    }
  },
  "id": "hogehoge"
}
```