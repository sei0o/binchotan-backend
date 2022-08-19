# プロトコル

フロントエンドとバックエンドの間の通信は、JSON-RPC v2によって行います。フロントエンドがJSON-RPCのクライアント、バックエンドはサーバとして機能します。

バックエンドはフロントエンドからのリクエストに応じて、フロントエンドの代わりにTwitter APIから情報を取得します。取得した情報はJSON-RPCのレスポンスとして返却されます。バックエンドにおける処理に応じて、リクエストはプレーンリクエスト (plain/pass-through requests) とフィルタリング付きリクエスト (filtered requests) の2種類に分類されます。

## プレーンリクエスト

プレーンリクエストは、特にフィルタなどの処理が必要ないエンドポイントを呼ぶときに使います。バックエンドはフロントエンドからの情報に認証情報を付加してから、そのままTwitter APIに転送し、得たレスポンスをそのままフロントエンドに返却します。

```json
// プレーンリクエスト
{
  "jsonrpc": "2.0",
  "method": "v0.plain_request", // pass through, proxy
  "params": {
    "method": "GET",
    "endpoint": "/lists/tweets",
    "api_params": {
        ...
    }
  },
  "id": "hogehoge" // リクエストごとに異なるidを使用する
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
  "id": "hogehoge" // リクエストと同じidが付与される
}
```

## フィルタリング付きリクエスト

タイムラインに関連するいくつかのエンドポイントについては、専用のRPCメソッドにリクエストすることで、フィルタを通した情報を取得することができます。レスポンスの形式はプレーンリクエストの場合と同様です。

```json
// ホームタイムラインを取得するリクエスト
{
  "jsonrpc": "2.0",
  "method": "v0.home_timeline",
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

## エラー

リクエストの処理中に何らかのエラーが発生した場合には、次のように `error` オブジェクトを含むレスポンスを返します。

```json
// 存在しないメソッドに対するリクエスト
{
  "jsonrpc": "2.0",
  "method": "v0.this_endpoint_is_not_available",
  "id": "foobar"
}

// レスポンス
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32601,
    "data": null,
    "message": "This method does not exist"
  },
  "id": "foobar"
}
```

エラーコード (code) の定義は次表の通りです。

| code   | 説明                                                  |
| ------ | ----------------------------------------------------- |
| -32700 | JSONのパースに失敗しました。                          |
| -32600 | リクエストの形式が誤っています。                      |
| -32601 | メソッドが存在しません。                              |
| -32602 | メソッドに与えるパラメータが間違っています。          |
| -32603 | JSON-RPC内部のエラー（未使用）                        |
| -32000 | バックエンド内部のエラー                              |
| -32001 | Twitter APIがエラーコード（4xx, 5xx）を返却しました。 |
| -32002 | Lua関連のエラーです。                                 |
| -32099 | バックエンドで発生したその他のエラーです。            |