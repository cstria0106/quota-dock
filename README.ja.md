# QuotaDock

[English](README.md) | [한국어](README.ko.md) | [简体中文](README.zh.md)

Claude と Codex の残り使用量を、コンパクトな AMOLED ディスプレイに表示する小さなデスクガジェットです。

![QuotaDock display](docs/assets/quotadock-display.png)
![QuotaDock 韓国語表示](docs/assets/quotadock-display-ko.png)

## 必要なもの

- [ESP32-S3-Touch-AMOLED-1.64](https://www.waveshare.com/wiki/ESP32-S3-Touch-AMOLED-1.64) ボード
- USB-C ケーブル
- 2.4 GHz Wi-Fi ネットワーク

## インストール

1. [Releases](../../releases) ページから、お使いの OS 向けの最新 QuotaDock デスクトップアプリをダウンロードします。
2. USB-C ケーブルでボードをコンピューターに接続します。
3. ダウンロードしたアプリを実行します。

## 使い方

1. アプリを開き、ESP32-S3 を接続します。アプリが自動的に検出します。
2. 初回セットアップの場合は、案内に従ってファームウェアを書き込み、Wi-Fi 情報を入力します。
3. セットアップが完了すると、Claude と Codex の使用量がデバイス画面に表示され、アプリが定期的に同期します。

追加機能:

- プロバイダーごとのカスタムアイコン画像

## クレジット

- 使用量取得ロジックは [CodexBar](https://github.com/steipete/codexbar) を参考にしています。

## ライセンス

このプロジェクトは [MIT License](LICENSE) の下で配布されています。

Galmuri フォントは SIL Open Font License 1.1 の下で別途ライセンスされています。著作権はフォント作者に帰属します。詳しくは [Galmuri font repository](https://github.com/quiple/galmuri) を参照してください。
