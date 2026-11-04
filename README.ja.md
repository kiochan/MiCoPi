# MiCoPi

[English](README.md) · [简体中文](README.zh.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

アニメ・マンガのイラスト制作に向けた、Rust 製の軽量なネイティブペイントアプリです。描画の応答性を重視し、拡張可能で AI と連携できる実行コアを備えています。

現在は **Windows 10/11 x64** に対応しています。デスクトップ UI は **egui + egui_dock** と OpenGL、ペン入力は Windows Ink を使用します。開発初期のプロジェクトです。

**0.21.0:** [検証可能なイベント再生](docs/replay.md)を追加しました。筆圧、レイヤー、プラグイン操作を記録し、タイムライン再生、SHA-256 検証、プロジェクト内の履歴保存、記録ファイルと PNG 連番の書き出しに対応します。

[アトミック操作グループ](docs/atomic-operations.md)を追加しました。失敗時に変更を戻し、グループ全体を一つのイベントとして記録します。MCRPLY02 はメタデータ差分、タイル参照と XOR 圧縮を使用し、旧形式の読み込みも維持します。

![実際の MiCoPi ワークベンチ。画像は英語表示です](docs/screenshots/workbench.png)

ツールバーとキャンバスは別のモニターへ移動できる独立した OS ウィンドウで、フロート、四方向のドッキング、タブ化に対応します。各機能プラグインがウィンドウ定義と UI ソースを所有し、登録したアクションは検索とショートカット設定が可能です。[Plugin Windows](docs/plugin-windows.md) を参照してください。

ブラシパネルから拡張・縮小・ぼかし対応の連続領域・レイヤー全体の同色塗りつぶし、繰り返し・折り返し対応の線形・放射状グラデーション、筆圧設定とカーブ表示を利用できます。[描画ツール](docs/painting-tools.md)を参照してください。

キャンバスサイズ変更と全レイヤーの正確な切り抜きに対応し、レイヤー・ノードの識別情報、取り消しと検証可能な再生を維持します。[キャンバス編集](docs/canvas-editing.md)を参照してください。

[統一プラグインシステム](docs/plugin-system.md)は、バージョン、互換性、依存関係、読み込み状態を設定画面と自動化 API に公開します。有効化・無効化の計画は依存関係を解決し、保存後の再起動で反映されます。プラグイン独自の翻訳ファイルは互いに分離され、ホットリロードできます。

## 基本理念

**描画を最優先に。機能はプラグインで実装し、人と自動化は同じ実行コアを使う。**

ペンの各サンプルは型付きインターフェースで処理し、筆圧、手ぶれ補正、部分更新を維持します。プラグイン、MCP、headless は、スキーマ付きアクションと公開状態を通して同じ Session を操作します。画像ノードは非破壊で合成し、操作ワークフローは明示的に実行したときだけ動きます。

AI-ready は、発見・確認・検証できる連携仕様を意味します。クラウドへの常時接続や内蔵 AI アシスタントを必須にはしません。図入りの[設計理念](docs/philosophy.md)、[文書一覧](docs/README.md)、[ロードマップ](ROADMAP.md)、[ディレクトリ構成](docs/project-structure.md)をご覧ください。技術文書の基準言語は英語です。

## 主な機能

- 独立した DLL によるブラシ、消しゴム、スポイト、手のひら、ズーム。
- Windows Ink の筆圧、筆圧カーブ、手ぶれ補正、ブラシ輪郭カーソル。
- HSV カラーホイール、RGB 調整、スウォッチ、ブラシプリセット。
- レイヤー表示、不透明度、透明度保護、単色塗りつぶし、合成モード、タイル単位の取り消しとやり直し。
- ドッキング、フロート、タブグループに対応したパネル。間隔、寸法、テーマ、ショートカットを設定可能。
- 検索できる設定、JSON 編集、コマンドパレット、ショートカットの記録。
- 透明 PNG 書き出し、PNG/JPEG/BMP 読み込み、`.mcp` プロジェクト。
- レイヤー連動の非破壊画像グラフ、レイヤーごとのフィルター、自由合成とプラグイン登録の操作・状態ワークフロー。
- JSONL バッチ、MCP stdio、実行中の GUI への接続。
- 共通のプラグイン一覧、依存関係を考慮する設定、プラグインごとに分離した言語パック。

レイヤーの名前変更、複製（**Ctrl+J**）、並べ替え、削除と、反転、180° 回転、整数オフセットをサポートします。[レイヤー編集](docs/layer-editing.md)を参照してください。

下のレイヤーと結合（Ctrl+E）、表示レイヤーを結合（Ctrl+Shift+E）、表示内容を新規レイヤーに統合（Ctrl+Alt+Shift+E）、透明部分を保持する画像の統合に対応します。[Layer editing](docs/layer-editing.md) を参照してください。

現在は Session ごとに文書が 1 つです。Wintab、選択範囲、自由変形、レイヤーグループ、水彩エンジンは未実装です。ネイティブ DLL はホストと同じプロセス内で実行されます。

## ビルドと実行

Rust 1.88 以降、Visual Studio C++ Build Tools、Windows SDK が必要です。

```powershell
.\scripts\build.ps1 -PluginsOnly
cargo run --release --locked
.\scripts\build.ps1 -Test
```

`dist/MiCoPi.exe` を起動してください。同じ場所の `plugins/`、`locales/`、`licenses/` が必要です。利用者の環境に Rust は不要です。プロジェクト内のポータブルツールチェーンについては英語の[開発ガイド](docs/development.md)をご覧ください。

```powershell
New-Item -ItemType Directory -Force target/example-output | Out-Null
.\dist\micopi-cli.exe run examples/automation/draw.jsonl
.\dist\micopi-cli.exe actions
.\dist\micopi-cli.exe mcp
```

通常の MCP は独立したキャンバスを使います。`mcp --attach <profile>/session.json` で既存の GUI に接続できます。用途別のサンプルは [examples](examples/README.md) にあります。

## 表示言語

既定は英語で、簡体字中国語、日本語、韓国語に対応しています。**Ctrl+,** で設定を開き、`ui.language` を検索して言語を選び、保存してください。変更はすぐにプレビューされ、未保存のまま閉じると最後に保存した値に戻ります。

```json
{"version":1,"ui":{"language":"ja"}}
```

言語を切り替えてもアクション ID、設定キー、ノード端子名、ファイルパス、キャンバスの画素は変わりません。詳細は英語の[ローカライズガイド](docs/localization.md)を参照してください。

英語のみ実行ファイルに埋め込みます。他の言語は実行ファイルと同じ場所の `locales/` から読み込み、設定ディレクトリの `locales/` で上書きできます。JSON の変更は再ビルド・再起動なしで反映され、不正な編集時は直前の有効な翻訳を維持します。`--locales-dir <directory>` で基本カタログの場所を変更できます。

## MiCoPi の拡張

[Quick tools サンプル](examples/plugins/quick-tools/manifest.json)にはブラシプリセット、マクロ、多言語ボタンパネルが含まれます。パッケージ全体をプラグインディレクトリへコピーして再起動してください。`plugins.list` で状態・診断・機能の所有者を確認し、読み取り専用の `plugins.plan` で依存関係の変更を確認できます。

画素処理は[ネイティブ C ABI SDK](docs/native-plugins.md)で拡張します。機能 UI のソースはホストと一緒にコンパイルし、DLL の交換や機能定義の変更には再起動が必要です。実行、文書履歴、ウィンドウのライフサイクルはホストが管理します。[プラグインシステム](docs/plugin-system.md)と[プラグインウィンドウ](docs/plugin-windows.md)を参照してください。

## 構成と貢献

`src/` はホストと UI、`plugins/` はインストール可能なパッケージと独立したアルゴリズム、`crates/plugin-api/` は ABI SDK です。文書、画像、サンプル、開発ツールは `docs/`、`docs/screenshots/`、`examples/`、`scripts/` に分かれています。ビルド結果、配布ファイル、個人の作品は Git に含めません。

英語の[貢献ガイド](CONTRIBUTING.md)、[アーキテクチャ](docs/architecture.md)、[変更履歴](CHANGELOG.md)をご覧ください。英語 README を基準とし、翻訳版も同じ機能と制約を説明します。

コーディングエージェントは [AGENTS.md](AGENTS.md) も参照してください。メンテナー指定の Git 手順に従い、現在の委託作業は `main` に機能単位でコミットします。プッシュは明示的な依頼時のみ行い、ビルド成果物や個人設定はコミットしません。

## ライセンス

アプリのソースは [MIT](LICENSE) です。内蔵 Google Material アイコンは [Apache 2.0](assets/material-symbols/LICENSE) で、[通知](assets/material-symbols/NOTICE.md)を同梱します。配布物には依存ライブラリのライセンスが含まれます。
