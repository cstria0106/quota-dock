# QuotaDock

[English](README.md) | [한국어](README.ko.md) | [日本語](README.ja.md)

一个小型桌面设备，可在紧凑的 AMOLED 屏幕上显示 Claude 和 Codex 的剩余用量。

![QuotaDock display](docs/assets/quotadock-display.png)
![QuotaDock 韩文显示](docs/assets/quotadock-display-ko.png)

## 准备

- [ESP32-S3-Touch-AMOLED-1.64](https://www.waveshare.com/wiki/ESP32-S3-Touch-AMOLED-1.64) 开发板
- USB-C 线缆
- 2.4 GHz Wi-Fi 网络

## 安装

1. 从 [Releases](../../releases) 页面下载适用于你操作系统的最新版 QuotaDock 桌面应用。
2. 使用 USB-C 线缆将开发板连接到电脑。
3. 运行下载的应用。

## 使用方法

1. 打开应用并连接 ESP32-S3。应用会自动检测设备。
2. 如果是首次设置，请按照向导刷写固件并输入 Wi-Fi 信息。
3. 设置完成后，Claude 和 Codex 的用量会显示在设备屏幕上，应用会定期自动同步。

附加功能:

- 为每个提供方设置自定义图标图片

## 致谢

- 用量查询逻辑参考了 [CodexBar](https://github.com/steipete/codexbar)。

## 许可证

本项目基于 [MIT License](LICENSE) 发布。

Galmuri 字体基于 SIL Open Font License 1.1 单独授权，版权归字体作者所有。详情请参阅 [Galmuri font repository](https://github.com/quiple/galmuri)。
