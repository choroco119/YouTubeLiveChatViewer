# UIタブ化最適化タスク

左側設定パネルのUIをタブ化し、リファクタリングするためのタスクリストです。

- [x] `app.rs` でのタブ状態（enum）の追加
    - [x] `SettingsTab` enum を定義
    - [x] `App` 構造体に `active_tab` フィールドを追加
    - [x] `App::new` にて `active_tab` を `Voice` に初期化
- [x] `draw_left_panel` の分割リファクタリング
    - [x] `draw_left_panel` 内にタブ選択UIを実装
    - [x] CeVIO設定を `draw_voice_settings` メソッドに分割
    - [x] Gemini設定を `draw_ai_settings` メソッドに分割
    - [x] OBS設定を `draw_obs_settings` メソッドに分割
    - [x] NG/辞書/ユーザー設定を `draw_filter_settings` メソッドに分割
    - [x] 効果音設定を `draw_se_settings` メソッドに分割
    - [x] 表示/その他設定を `draw_other_settings` メソッドに分割
    - [x] 各タブの切り替え描画ロジックを統合
- [x] 検証
    - [x] `cargo check` および `cargo test` の実行
    - [x] 各タブの切り替え、設定変更が正しく動作することのUI手動検証
