# VideoDownload

A fast, lightweight desktop app to download videos from YouTube, Facebook, TikTok, Instagram, and [1000+ sites](https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md) — powered by [yt-dlp](https://github.com/yt-dlp/yt-dlp).

**yt-dlp is bundled. Just install and use — no extra setup required.**

![License](https://img.shields.io/badge/license-MIT-blue)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)

---

## Features

- **1000+ sites**: YouTube, Facebook, TikTok, Instagram, Twitter/X, and more
- **Quality selection**: Best, 4K, 1080p, 720p, 480p, 360p
- **Audio-only**: MP3, M4A, Opus, WAV extraction
- **Download queue**: Multiple concurrent downloads (configurable limit)
- **Real-time progress**: Live speed, ETA, and progress bar
- **History**: Browse past downloads, reopen files directly
- **Light / dark mode**: System-aware, toggleable
- **Tiny installer**: ~15 MB — no Electron bloat

## Installation

Download the latest release from the [Releases page](../../releases):

| Platform | File |
|----------|------|
| Windows | `.msi` (recommended) or setup `.exe` |
| macOS | `.dmg` |
| Linux | `.AppImage` or `.deb` |

> **macOS note**: If Gatekeeper blocks the app, right-click → Open on first launch.

### Optional: ffmpeg

ffmpeg is needed to merge separate video+audio streams (1080p and above). Without it, the app falls back to the best pre-merged format available.

- **Windows**: `winget install ffmpeg`
- **macOS**: `brew install ffmpeg`
- **Linux**: `sudo apt install ffmpeg`

## Usage

1. Paste a video URL into the input field
2. Click **Fetch** to load available formats
3. Choose resolution / audio format
4. Click **Download**

For private Facebook videos or age-restricted YouTube content, configure cookies in **Settings** using `--cookies-from-browser`.

## Build from Source

```bash
git clone https://github.com/dajtvoxdev/youtube-download.git
cd youtube-download
npm install
npm run tauri dev      # development
npm run tauri build    # production build
```

**Requirements**: Rust 1.70+, Node.js 18+
- Windows: Visual Studio Build Tools (C++ workload)
- macOS: Xcode Command Line Tools
- Linux: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`

## Legal Notice

This tool is for personal use only. Users are responsible for complying with each platform's Terms of Service and applicable copyright law. Only download content you have the right to access.

## License

MIT

---

# VideoDownload (Tiếng Việt)

Ứng dụng desktop nhỏ gọn để tải video từ YouTube, Facebook, TikTok, Instagram và [1000+ trang](https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md).

**yt-dlp được đóng gói sẵn — chỉ cần cài đặt và dùng ngay, không cần cài thêm gì.**

## Tính năng

- **1000+ nguồn**: YouTube, Facebook, TikTok, Instagram, Twitter/X...
- **Chọn chất lượng**: Best, 4K, 1080p, 720p, 480p, 360p
- **Chỉ âm thanh**: MP3, M4A, Opus, WAV
- **Tải đồng thời**: Queue với giới hạn có thể cấu hình
- **Tiến trình thời gian thực**: Tốc độ, ETA, progress bar
- **Lịch sử**: Xem lại và mở file đã tải
- **Giao diện sáng / tối**
- **Installer nhỏ**: ~15 MB

## Cài đặt

Tải từ trang [Releases](../../releases):

| Nền tảng | File |
|----------|------|
| Windows | `.msi` (khuyến nghị) hoặc `.exe` |
| macOS | `.dmg` |
| Linux | `.AppImage` hoặc `.deb` |

### Tuỳ chọn: ffmpeg

ffmpeg cần thiết để ghép video+audio (chất lượng 1080p trở lên). Không có ffmpeg, app sẽ tự động chọn format đã ghép sẵn chất lượng tốt nhất.

## Lưu ý pháp lý

Công cụ này chỉ phục vụ mục đích cá nhân. Người dùng tự chịu trách nhiệm tuân thủ Terms of Service của các nền tảng và luật bản quyền. Chỉ tải nội dung bạn có quyền sử dụng.
