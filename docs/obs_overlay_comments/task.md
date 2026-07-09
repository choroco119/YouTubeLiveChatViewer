# 通常コメントOBS取り込みタスク

OBSオーバーレイにYouTube Liveの通常コメントを統合表示するための実装タスクリストです。

- [x] `web_server.rs` のデータ構造とAPIレスポンスの拡張
    - [x] `ObsComment` 構造体の追加
    - [x] `WebServerState` に `comments` ベクターを追加
    - [x] `/api/response` エンドポイントで `comments` をJSONとして返却
- [x] `app.rs` におけるコメント追加処理の追加
    - [x] `poll_chat_events` 内で新規コメントを `web_server_state` の `comments` に反映（最大50件保持）
    - [x] `connect` 時に `comments` 履歴をクリア
- [x] OBSオーバーレイHTML/CSS/JS of `web_server.rs` の改修
    - [x] HTMLに通常コメント用のコンテナ（`#chat-container`）を追加
    - [x] 2カラム（通常コメント用の左カラムと、Gemini回答用の右カラム）のCSSレイアウトの実装
    - [x] 通常コメント of 通常コメントのモダンなカードデザイン（Owner/Moderator/Member等の色分け・バッジ表示）
    - [x] JSポーリングによる新着コメント of JSポーリングによる新着コメントの差分追加および自動スクロールアニメーション
- [x] 検証
    - [x] `cargo check` および `cargo test` の実行
    - [x] 実際のコメント表示、スクロール、Gemini回答ポップアップの連動のブラウザ確認
