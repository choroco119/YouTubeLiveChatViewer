# アプリケーション名称変更とアイコン設定の実装計画

本タスクでは、アプリケーションの名称を「YouTubeLiveChatViewer」に変更し、併せてデスクトップアイコンを設定します。これにはソースコード、設定ファイル、ドキュメントの包括的な更新が含まれます。

## ユーザーレビューが必要な事項

- **新しいアイコンの確認**: 生成されたアイコン（`app_icon_v1`）が意図に沿っているかご確認ください。
- **名称の統一性**: コード内の内部識別子（パッケージ名）も `youtube-live-chat-viewer` に変更しますが、問題ないでしょうか。

## 変更内容

### 1. プロジェクト設定とソースコード

#### [MODIFY] [Cargo.toml](file:///c:/Users/kohei/.gemini/antigravity/scratch/YouTubeLiveChatViewer/Cargo.toml)
- パッケージ名を `youtube-live-chat-viewer` に変更。
- バイナリ名を `youtube-live-chat-viewer` に変更。
- アイコン読み込み用に `image` クレートを追加。

#### [MODIFY] [src/main.rs](file:///c:/Users/kohei/.gemini/antigravity/scratch/YouTubeLiveChatViewer/src/main.rs)
- ウィンドウタイトルを 「YouTubeLiveChatViewer」 に更新。
- アプリケーションアイコンを設定するロジックを追加。

#### [NEW] `assets/icon.png`
- 生成したアイコンをプロジェクト内に配置。

### 2. ドキュメント

#### [MODIFY] [README.md](file:///c:/Users/kohei/.gemini/antigravity/scratch/YouTubeLiveChatViewer/README.md)
- アプリ名、実行ファイル名、ドキュメントパスの記述を更新。

#### [RENAME] `docs/youtube_live_comment_viewer` -> `docs/youtube_live_chat_viewer`
- ディレクトリ名を新しいアプリ名に合わせて変更。

#### [MODIFY] `docs/youtube_live_chat_viewer/*.md`
- 各ドキュメント内のアプリ名表記を更新。

## 実行手順

1. `assets` ディレクトリを作成し、アイコン画像を配置する。
2. `Cargo.toml` を更新し、依存関係とパッケージ名を変更する。
3. `src/main.rs` を更新し、タイトル変更とアイコン設定を実装する。
4. `README.md` と既存の `docs` 内のドキュメントを全て更新する。
5. `docs/youtube_live_comment_viewer` ディレクトリをリネームする。
6. 本タスクの完了記録として `walkthrough.md` を作成する。

## 検証計画

### 手動確認
- `cargo run` で起動し、ウィンドウタイトルが正しく変更されていることを確認。
- タスクバーやウィンドウ左上のアイコンが表示されていることを確認。
- `README.md` や `docs` 内のリンクが正しく機能し、表記が統一されていることを確認。
