# QuotaDock

[한국어](README.ko.md) | [日本語](README.ja.md) | [简体中文](README.zh.md)

A small desk gadget that shows your remaining Claude and Codex usage on a compact AMOLED display.

![QuotaDock display](docs/assets/quotadock-display.png)
![QuotaDock Korean display](docs/assets/quotadock-display-ko.png)

## Requirements

- [ESP32-S3-Touch-AMOLED-1.64](https://www.waveshare.com/wiki/ESP32-S3-Touch-AMOLED-1.64) board
- USB-C cable
- 2.4 GHz Wi-Fi network

## Installation

1. Download the latest QuotaDock desktop app for your operating system from the [Releases](../../releases) page.
2. Connect the board to your computer with a USB-C cable.
3. Run the downloaded app.

## Usage

1. Open the app and connect the ESP32-S3. The app will detect it automatically.
2. If this is your first setup, follow the guide to flash the firmware and enter your Wi-Fi information.
3. Once setup is complete, Claude and Codex usage appears on the device screen, and the app syncs it periodically.

Additional feature:

- Custom provider icon images

## Credits

- The usage lookup logic is based on [CodexBar](https://github.com/steipete/codexbar).

## License

This project is distributed under the [MIT License](LICENSE).

The Galmuri font is licensed separately under the SIL Open Font License 1.1. Copyright belongs to the font authors. See the [Galmuri font repository](https://github.com/quiple/galmuri) for details.
