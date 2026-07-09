# YouTubeLiveChatViewer システム仕様書

本ドキュメントは、YouTubeのライブ配信コメントをリアルタイムで取得し、CeVIO AIでの読み上げやGemini AIでの自動要約・返答生成、およびOBS向けの配信用透過画面（Webサーバー）などを提供するコメントビューア「YouTubeLiveChatViewer」の仕様書です。

---

## 1. システム概要
本システムはRustで開発されたWindowsデスクトップアプリケーションです。
GUIには `eframe`/`egui` を採用し、非同期処理に `tokio`、COM接続によるCeVIO AI制御、YouTubeからのライブチャット取得、Gemini APIによるAI支援機能、およびOBS配信用オーバーレイ画面をホストする軽量Webサーバーを内蔵しています。

```
+-----------------------------------------------------------------+
|                    YouTubeLiveChatViewer (GUI)                  |
|  +------------------+  +------------------+  +------------------+  |
|  |   音声設定タブ   |  |   AI 設定タブ    |  |  配信 設定タブ   |  |
|  |   フィルタ設定   |  |   効果音設定     |  |  その他設定      |  |
|  +------------------+  +------------------+  +------------------+  |
+---------+-----------------------+---------------------+---------+
          |                       |                     |
          v                       v                     v
+-------------------+   +-------------------+   +-------------------+
|     cevio.rs      |   |     gemini.rs     |   |    web_server.rs  |
|  (COM / CeVIO AI) |   |   (Gemini API)    |   |  (OBS Overlay HTTP)|
+-------------------+   +-------------------+   +-------------------+
          ^                       ^                     | (API/HTML)
          | (読み上げ)             | (要約・返答)         v
          +-----------+-----------+             +-------------------+
                      |                         |     OBS Studio    |
            +-------------------+               | (ブラウザソース)  |
            |    youtube.rs     |               +-------------------+
            | (YouTube Chat)    |
            +-------------------+
```

---

## 2. モジュール構成と役割
プログラムは以下のモジュールで構成されています。

| モジュール名 | ファイルパス | 主な役割 |
| :--- | :--- | :--- |
| `main` | [main.rs](file:///C:/Users/kohei/.gemini/antigravity/scratch/youtube_live_chat_viewer/src/main.rs) | エントリーポイント。Tokioランタイム起動、アプリウィンドウ初期化、アイコン読み込み。 |
| `app` | [app.rs](file:///C:/Users/kohei/.gemini/antigravity/scratch/youtube_live_chat_viewer/src/app.rs) | メインUIおよびイベントループ。設定データの保持、タブ切り替え、各スレッド/タスクとのメッセージング制御。 |
| `cevio` | [cevio.rs](file:///C:/Users/kohei/.gemini/antigravity/scratch/youtube_live_chat_viewer/src/cevio.rs) | Windows COMを用いたCeVIO AI（Talker）の制御。話者一覧/感情リスト取得、非同期読み上げキュー管理。 |
| `youtube` | [youtube.rs](file:///C:/Users/kohei/.gemini/antigravity/scratch/youtube_live_chat_viewer/src/youtube.rs) | 配信URLまたは動画IDからチャット取得トークンを取得し、ポーリングによって新規コメントを非同期回収。 |
| `gemini` | [gemini.rs](file:///C:/Users/kohei/.gemini/antigravity/scratch/youtube_live_chat_viewer/src/gemini.rs) | Gemini APIを介した自動テキスト生成。指数バックオフによる429エラーハンドリング。 |
| `web_server` | [web_server.rs](file:///C:/Users/kohei/.gemini/antigravity/scratch/youtube_live_chat_viewer/src/web_server.rs) | 内蔵Webサーバー。OBSブラウザソース用の透過HTML画面（`/index.html`）およびデータAPI（`/api/response`）を提供。 |
| `settings` | [settings.rs](file:///C:/Users/kohei/.gemini/antigravity/scratch/youtube_live_chat_viewer/src/settings.rs) | 各種設定情報（`settings.json`）およびリスナーメモ（`user_notes.json`）の永続化とロード。 |
| `text_filter` | [text_filter.rs](file:///C:/Users/kohei/.gemini/antigravity/scratch/youtube_live_chat_viewer/src/text_filter.rs) | コメント本文のクリーンアップ（ユーザー辞書の適用、URLの省略、草の変換、文字数制限など）。 |
| `dictionary` | [dictionary.rs](file:///C:/Users/kohei/.gemini/antigravity/scratch/youtube_live_chat_viewer/src/dictionary.rs) | 読み替え辞書（`dictionary.json`）の管理とテキスト置換処理。 |

---

## 3. 主要機能の仕様

### ① YouTubeチャット取得機能 (`youtube.rs`)
- **動画IDの抽出**: 標準URL、短縮URL（`youtu.be`）、埋め込み、ショート、ライブ配信URLから動画IDを抽出。
- **接続トークン（Continuation）の取得**: 対象配信ページのHTMLから `ytInitialData` 内のチャット取得用トークンおよびAPIキー、`visitorData` を自動解析。
- **コメント回収**: `get_live_chat` APIを約2秒間隔でポーリング。重複回避用のキャッシュ（ハッシュセット）を保持。
- **リスナー情報判別**: 配信者（Owner）、モデレーター（Moderator）、メンバー（Member）のバッジ情報を検出。

### ② CeVIO AI読み上げ機能 (`cevio.rs`)
- **COM連携**: `CeVIO.Talk.RemoteService2.Talker2`（またはV40）のCOMオブジェクトをインスタンス化して操作。
- **パラメータ制御**: 話者（キャスト）、音量、速さ、高さ、声質（Alpha）、抑揚（ToneScale）、および各キャスト特有の感情パラメータの動的適用。
- **キュー管理とスキップ**: 読み上げが追いつかない場合の遅延対策として、キュー件数が指定の閾値（`skip_threshold`）を超えた場合に古いメッセージを自動スキップする機能を搭載。

### ③ Gemini AI自動返答・要約機能 (`gemini.rs` / `app.rs`)
- **コンテキスト付与**: 取得した「配信タイトル」をプロンプトのコンテキストとしてGemini APIに自動追加。
- **自動返答生成**: 設定された時間間隔（デフォルト30秒）で、その間に受信した複数のコメントをまとめ、配信を盛り上げる回答を生成。生成された回答はCeVIO AIで自動読み上げ。
- **堅牢な通信**: `429 Too Many Requests` エラー回避のため、指数バックオフ付きリトライ（最大3回）を実行。
- **モデル設定**: `gemini-2.5-flash` など任意のモデルを指定可能。

### ④ OBS配信用オーバーレイ画面 (`web_server.rs`)
- **内蔵Webサーバー**: `tokio::net::TcpListener` を用いた超軽量なHTTPサーバーがアプリ起動中に稼働（デフォルトポート：`3000`）。
- **透過デザイン**: OBSブラウザソース（CSS透過設定）に適したフォント（M PLUS Rounded 1c）と透過背景。
- **画面レイアウト**:
  - **上部**: Gemini AIの回答（吹き出し形式、フェードアウト＆フェードインアニメーション付き）。Gemini機能が無効な場合は自動で非表示化し、領域を解放。
  - **下部**: 通常コメントのスクロールフィード（直近最大30件表示、下から上に自動スクロール）。
- **ユーザー種別の可視化**: 配信者（👑金）、モデレーター（🔧水色）、メンバー（🟢緑）の名前色分けとアイコンバッジ。

### ⑤ リスナーメモ管理機能 (`settings.rs`)
- **チャンネルID紐付け**: チャンネルID（`author_id`）に紐付けて、ユーザー名と自由記述のメモ（「初見」「常連」など）を記録。
- **別ファイル管理**: 設定とは独立して `user_notes.json` に保存されるため、設定初期化等の影響を受けない。

### ⑥ 効果音（SE）演出機能 (`app.rs` / `settings.rs`)
- **キーワードトリガー**: コメント内に特定の文字列パターンが含まれる場合、指定された音声ファイル（mp3/wav等）を `rodio` を使用して自動再生。
- **音量個別設定**: 効果音ごとに再生音量を設定可能。

### ⑦ 読み替え辞書・NGフィルタ機能 (`text_filter.rs` / `dictionary.rs`)
- **NGワード/NGユーザー**: NGワードは指定文字列（「不適切な表現」など）に置換。NGユーザーのコメントは読み上げおよび表示をスキップ。
- **正規表現対応ユーザー辞書**: 特定の表記揺れや読み方の補正を正規表現または通常置換で定義し、CeVIO AIに渡す前に自動適用。

### ⑧ ログ管理機能 (`app.rs` / `settings.rs`)
- **自動保存（Markdown形式）**: 配信中の全コメントをタイムスタンプ、ユーザーID、ユーザー名、本文と共にMarkdown形式（`.md`）で自動記録。
- **出力先フォルダの設定**: デフォルトの `logs/` フォルダのほか、UI上（その他設定タブ）から任意のカスタムフォルダを保存先として選択・設定可能。
- **過去ログ閲覧（下位互換対応）**: 保存されたMarkdown（`.md`）および従来のテキスト（`.txt`）の過去ログファイルを読み込み、当時のコメントを再現可能。

---

## 4. データファイル構造

### ① 設定ファイル `settings.json`
```json
{
  "video_id": "動画IDまたはURL",
  "narrator": "CeVIO話者名",
  "profiles": {
    "話者名": {
      "speed": 50,
      "pitch": 50,
      "volume": 50,
      "alpha": 50,
      "intonation": 50,
      "emotions": {
        "感情名": 50
      }
    }
  },
  "tts_enabled": true,
  "skip_threshold": 5,
  "ng_words": ["NG単語1", "NG単語2"],
  "ng_users": ["チャンネルID1"],
  "max_read_length": 100,
  "read_more_text": "以下略",
  "ng_replacement_text": "不適切な表現",
  "se_entries": [
    {
      "pattern": "再生キーワード",
      "file_path": "C:\\path\\to\\sound.mp3",
      "volume": 0.5
    }
  ],
  "gemini_api_key": "AIzaSy...",
  "gemini_enabled": false,
  "gemini_interval_secs": 30,
  "gemini_system_prompt": "システムプロンプト...",
  "gemini_model": "gemini-2.5-flash",
  "obs_server_enabled": true,
  "obs_server_port": 3000,
  "gemini_name": "AIアシスタント",
  "log_dir": "C:\\path\\to\\logs"
}
```

### ② リスナーメモ `user_notes.json`
```json
{
  "チャンネルID": {
    "name": "ユーザー名",
    "note": "メモ内容"
  }
}
```

### ③ 読み替え辞書 `dictionary.json`
```json
[
  {
    "pattern": "置換対象",
    "replacement": "置換後",
    "is_regex": false
  }
]
```

---

## 5. 動作要件・開発環境
- **OS**: Windows 10 / 11 (64bit)
- **開発言語**: Rust (edition 2021)
- **前提条件**:
  - [CeVIO AI](https://cevio.jp/) がインストールされ、COM連携ライブラリが利用可能なこと。
  - インターネット接続（YouTubeおよびGemini API用）。

---

## 6. 改訂履歴
- **2026-07-09**:
  - ログの Markdown（`.md`）形式での保存に対応。
  - 設定UIおよび `settings.json`（`log_dir`フィールド）によるログ保存先フォルダのカスタム設定仕様を追加。
  - 過去ログ（`.txt` / `.md`）再生の下位互換対応を追加。
  - フォルダ名変更（`youtube_live_chat_viewer`）に伴うドキュメント内のファイルパスリンクを更新。
