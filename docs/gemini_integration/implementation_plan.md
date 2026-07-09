# Gemini連携によるコメント自動回答・読み上げ機能の実装計画

配信中のコメントログを定期的にGemini APIに送信し、AIによる回答を生成してCeVIO AIで読み上げる機能を追加します。
また、回答を個別に確認できるUIパネルを右側に追加します。

## ユーザー確認が必要な事項

> [!IMPORTANT]
> **Gemini APIキーの管理**
> アプリ上でAPIキーを入力・保存できるようにします（`settings.json`に保存されます）。APIキーの漏洩を防ぐため、配信画面などに映り込まないよう、入力欄はパスワード伏せ字（`•`）表示にすることを推奨します。

> [!NOTE]
> **Geminiのプロンプト（システム指示）**
> AIのキャラクター付けや回答の長さを調整するために、システムプロンプトの編集欄を設定画面に設けます。初期値として「あなたは配信のAIアシスタントです。リスナーからの複数のコメントに対して、配信を盛り上げるように短く100文字以内で回答してください」といった内容を設定します。

## 確定した動作仕様

> [!IMPORTANT]
> **コメントが無い期間と復帰時の動作**
> 設定したインターバルが経過した際、新規コメントが無い場合はGeminiへの送信をスキップします。
> スキップされた状態で待機中に、新たなコメントが1件でも到着した場合は、インターバル時間を待つことなく**即座に**Geminiへ送信して回答を得ます。
> 送信後は再びインターバルタイマーがリセットされ、一定時間送信が抑制されます。


## 提案される変更点

### 1. 設定情報の拡張 (`src/settings.rs`)
Gemini連携に必要な設定項目を追加します。

- [MODIFY] [settings.rs](file:///c:/Users/kohei/.gemini/antigravity/scratch/YouTubeLiveChatViewer/src/settings.rs)
  - `Settings` 構造体に以下を追加：
    - `gemini_api_key: String`
    - `gemini_enabled: bool`
    - `gemini_interval_secs: u32` (デフォルト: 30秒)
    - `gemini_system_prompt: String` (デフォルト値あり)
    - `gemini_history_limit: usize` (過去何件の会話コンテキストを保持するか)

### 2. Gemini API連携モジュールの新規作成 (`src/gemini.rs` [NEW])
Gemini APIを非同期で呼び出すロジックを実装します。

- [NEW] [gemini.rs](file:///c:/Users/kohei/.gemini/antigravity/scratch/YouTubeLiveChatViewer/src/gemini.rs)
  - Google AI Studio (Gemini 1.5/2.5 Flash等) のエンドポイントに対して `reqwest` で POST リクエストを送信する関数を実装。
  - リクエスト内容：
    - システムプロンプト（配信アシスタントとしての指示）
    - 取得したコメントの結合テキスト（例: 「ユーザーA: こんにちは\nユーザーB: 今日のゲームは何ですか？」）
  - レスポンスのパース処理。

### 3. コメント監視および制御ロジックの追加 (`src/app.rs`)
コメントの蓄積、インターバルの監視、Geminiへの送信処理を統合します。

- [MODIFY] [app.rs](file:///c:/Users/kohei/.gemini/antigravity/scratch/YouTubeLiveChatViewer/src/app.rs)
  - **状態の追加**:
    - `last_gemini_sent_time: std::time::Instant`（前回送信した時刻）
    - `last_processed_comment_index: usize`（前回送信した時点のコメントログのインデックス）
    - `gemini_responses: Vec<GeminiResponse>`（AIからの回答履歴を保持）
    - `is_waiting_gemini: bool`（リクエスト送信中かどうかのフラグ）
  - **イベントハンドリング**:
    - Geminiからの回答が得られた際、チャットイベント受信ループで受け取り、`gemini_responses` に追加。
    - 同時に、CeVIOの読み上げキュー (`CevioCommand::Speak`) に回答テキストを送信。
  - **タイマー制御**:
    - `update` メソッド内で、接続中かつGemini有効時、一定時間（`gemini_interval_secs`）経過しているか監視。
    - 経過しており、かつ新しいコメントが蓄積されていれば、バックグラウンドの非同期タスク（`tokio::spawn`）を起動してGemini APIを呼び出す。
  - **UIの変更**:
    - **左パネル（設定）**: Gemini連携のトグル、APIキー入力欄（伏せ字）、インターバル秒数のスライダー、システムプロンプトの入力欄を追加。
    - **右パネル（回答表示枠）**: `SidePanel::right("gemini_panel")` を新設。Geminiの最新の回答、および履歴をスクロール表示する枠を作成。

---

## 検証計画

### 自動テスト
- `cargo check` および `cargo build` でコンパイルエラーが無いことを確認。

### 手動検証
1. 起動後、設定パネルでダミーの（または有効な）APIキーを入力し、設定が `settings.json` に保存されることを確認。
2. YouTubeライブ（またはデバッグ用の過去ログ再生機能）に接続する。
3. 通常コメントが次々と取得され、リアルタイムにCeVIOで読み上げられることを確認。
4. 設定したインターバル（テスト用に10秒などに設定）経過後、蓄積されたコメントがGeminiに送信されることを確認（ログ出力等で確認）。
5. Geminiからの回答が右側の専用パネルに表示され、CeVIOで「回答の読み上げ」が行われることを確認。
6. 回答読み上げ後、再び通常のコメント読み上げが継続し、次のインターバルで同様のサイクルが繰り返されることを確認。
