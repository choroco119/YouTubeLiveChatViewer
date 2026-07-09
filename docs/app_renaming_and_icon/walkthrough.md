# 名称変更とアイコン設定 ウォークスルー

## 実施した内容

### 1. アプリケーション名称の変更
- `Cargo.toml` の `package.name` および `[[bin]].name` を `youtube-live-chat-viewer` に変更しました。
- `src/main.rs` 内のウィンドウタイトルを `YouTubeLiveChatViewer` に変更しました。
- `README.md` 内の全ての記述（タイトル、実行ファイル名、ドキュメントパス）を新しい名称に統一しました。

### 2. アイコンの設定
- AIによりYouTube Liveをイメージしたアイコンを生成し、`assets/icon.png` としてプロジェクトに追加しました。
- `src/main.rs` に `image` クレートを使用したアイコン読み込み処理を追加し、起動時にタスクバーやウィンドウにアイコンが表示されるようにしました。

### 3. ドキュメントの整理
- `docs/youtube_live_comment_viewer` ディレクトリを `docs/youtube_live_chat_viewer` にリネームしました。
- ディレクトリ内の `implementation_plan.md`, `task.md`, `walkthrough.md` に含まれるアプリ名を全て `YouTubeLiveChatViewer` に更新しました。

## 検証結果
- `cargo check` により、依存関係の解決とプロジェクト名の更新が正常に行われることを確認しました。
- 全てのドキュメントリンクと名称が不整合なく更新されていることを目視で確認しました。

## 完了したファイル構成
- `Cargo.toml` (更新)
- `src/main.rs` (更新)
- `README.md` (更新)
- `assets/icon.png` (新規)
- `docs/youtube_live_chat_viewer/` (リネーム・内容更新)
- `docs/app_renaming_and_icon/` (本タスクの記録)
