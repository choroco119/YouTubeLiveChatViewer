# 設定パネルUIのタブ化最適化 実装計画

機能追加によって縦に長くなり、複雑化したアプリ左側の設定パネルを、タブ切り替え式にリファクタリングしてUIをスッキリと最適化するための実装計画です。

## ユーザー確認事項

> [!IMPORTANT]
> **提案するタブの構成**
> 左パネル上部に以下の6つのタブボタンを配置し、選択されたカテゴリのみを表示します。
> 1. **🎙️ 音声**: CeVIO AI 連携の有効化、ナレーター選択、音声パラメータ、感情パラメータの設定。
> 2. **🤖 AI**: Gemini 連携の有効化、APIキー、モデル選択、インターバル、システムプロンプトの設定。
> 3. **📺 配信**: OBS 配信表示連携の有効化、表示名、ポート番号の設定、配信URLの表示・コピー。
> 4. **🛡️ フィルタ**: NGワード、NGユーザーの設定、および辞書登録とユーザー管理の各ウィンドウを開くボタン。
> 5. **📢 効果音**: 反応キーワードと音量による効果音（SE）の新規追加と一覧表示。
> 6. **⚙️ その他**: 自動スクロールトグル、コメントクリア、ログの読み込みなどの補助機能。
> 
> このタブ構成および項目分けで進めてよろしいでしょうか？

## オープンクエスチョン

> [!NOTE]
> *   アプリ起動時に表示されるデフォルトのタブは「🎙️ 音声」でよろしいですか？（前回のタブ状態を保存する仕組みにすることも可能です）

---

## 提案する変更内容

### 1. アプリケーションの状態拡張とロジック分割

#### [MODIFY] [app.rs](file:///c:/Users/kohei/.gemini/antigravity/scratch/YouTubeLiveChatViewer/src/app.rs)
*   タブの状態を表す `SettingsTab` enum を定義します。
    ```rust
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SettingsTab {
        Voice,
        Ai,
        Obs,
        Filter,
        SoundEffect,
        Other,
    }
    ```
*   `App` 構造体に `active_tab: SettingsTab` フィールドを追加します。
*   `App::new` で `active_tab` を `SettingsTab::Voice`（デフォルト）に初期化します。
*   `draw_left_panel` メソッドをリファクタリングし、上部で `active_tab` を切り替えるための横並び選択UI（`ui.selectable_value`）を配置します。
*   表示ロジックの肥大化を防ぐため、各タブの内容を描画する以下のヘルパーメソッドに分割実装します：
    - `draw_voice_settings(&mut self, ui: &mut Ui)`
    - `draw_ai_settings(&mut self, ui: &mut Ui)`
    - `draw_obs_settings(&mut self, ui: &mut Ui)`
    - `draw_filter_settings(&mut self, ui: &mut Ui)`
    - `draw_se_settings(&mut self, ui: &mut Ui)`
    - `draw_other_settings(&mut self, ui: &mut Ui)`

---

## 検証計画

### 自動テスト
- `cargo check` および `cargo test` を実行し、UIの変更によってコンパイルエラーや退行が発生しないことを確認します。

### 手動検証
1. アプリケーションを起動します。
2. 左側設定パネルの上部に「🎙️ 音声」「🤖 AI」「📺 配信」「🛡️ フィルタ」「📢 効果音」「⚙️ その他」のタブが横に綺麗に並んで表示されることを確認します。
3. 各タブをクリックして表示を切り替え、表示された項目が期待通り（隠れたり崩れたりせず）に操作できることを確認します。
4. 設定を変更した際に（例: ナレーター変更やGemini有効化）、動作や設定保存がこれまでの実装通りに行われることを確認します。
