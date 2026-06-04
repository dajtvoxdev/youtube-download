<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';

  // --- Types ---
  interface VideoInfo {
    url: string;
    title: string;
    thumbnail?: string;
    duration?: number;
    uploader?: string;
    max_height?: number;
  }

  interface DownloadOptions {
    url: string;
    format_id: string;
    output_dir: string;
    audio_only: boolean;
    audio_format: string;
    filename_template: string;
    title?: string;
  }

  interface ProgressPayload {
    job_id: string;
    status: string;
    percent: number;
    speed: string;
    eta: string;
    title?: string;
    error?: string;
  }

  interface DownloadState {
    id: string;
    title: string;
    status: string;
    percent: number;
    speed: string;
    eta: string;
    error?: string;
  }

  interface DownloadRecord {
    id: string;
    url: string;
    title?: string;
    output_path?: string;
    format_id: string;
    audio_only: boolean;
    status: string;
    error_msg?: string;
    started_at: string;
    finished_at?: string;
  }

  interface AppSettings {
    output_dir: string;
    ytdlp_path: string;
    max_concurrent: number;
    audio_format: string;
    filename_template: string;
    cookie_source: string;
  }

  const COOKIE_OPTIONS = [
    { label: 'Không dùng', value: '' },
    { label: 'Tự động (Chrome)', value: 'chrome' },
    { label: 'Tự động (Edge)', value: 'edge' },
    { label: 'Tự động (Firefox)', value: 'firefox' },
  ];

  // --- Constants ---
  const QUALITY_PRESETS = [
    { label: 'Best Quality', value: 'bestvideo+bestaudio/best', height: 9999 },
    { label: '4K (2160p)', value: 'bestvideo[height<=2160]+bestaudio/best[height<=2160]', height: 2160 },
    { label: '1080p', value: 'bestvideo[height<=1080]+bestaudio/best[height<=1080]', height: 1080 },
    { label: '720p', value: 'bestvideo[height<=720]+bestaudio/best[height<=720]', height: 720 },
    { label: '480p', value: 'bestvideo[height<=480]+bestaudio/best[height<=480]', height: 480 },
    { label: '360p', value: 'bestvideo[height<=360]+bestaudio/best[height<=360]', height: 360 },
  ];

  const QUALITY_PRESETS_NO_FFMPEG = [
    { label: 'Best Available', value: 'best', height: 9999 },
    { label: '1080p', value: 'best[height<=1080]', height: 1080 },
    { label: '720p', value: 'best[height<=720]', height: 720 },
    { label: '480p', value: 'best[height<=480]', height: 480 },
    { label: '360p', value: 'best[height<=360]', height: 360 },
  ];

  const AUDIO_FORMATS = ['mp3', 'm4a', 'opus', 'wav', 'aac'];

  // --- State ---
  let theme = $state<'dark' | 'light'>('dark');

  let url = $state('');
  let videoInfo = $state<VideoInfo | null>(null);
  let probing = $state(false);
  let probeError = $state('');

  let selectedQuality = $state('bestvideo+bestaudio/best');
  let audioOnly = $state(false);
  let audioFormat = $state('mp3');

  let settings = $state<AppSettings | null>(null);
  let showSettings = $state(false);
  let settingsForm = $state<AppSettings | null>(null);
  let ytdlpVersion = $state('');
  let savingSettings = $state(false);
  let ffmpegPath = $state<string | null>(null);

  let updateInfo = $state<{ current: string; latest: string; available: boolean } | null>(null);
  let checkingUpdate = $state(false);
  let updatingYtdlp = $state(false);
  let updateProgress = $state<{ percent: number; status: string } | null>(null);

  let appUpdateInfo = $state<{ current: string; latest: string; available: boolean; releaseUrl: string; releaseNotes: string } | null>(null);

  let downloads = $state<Record<string, DownloadState>>({});
  let history = $state<DownloadRecord[]>([]);
  let showHistory = $state(true);
  let startingDownload = $state(false);
  let downloadError = $state('');

  // Toast notifications
  interface Toast { id: number; msg: string; type: 'error' | 'info' }
  let toasts = $state<Toast[]>([]);
  let _toastId = 0;
  function showToast(msg: string, type: Toast['type'] = 'info') {
    const id = ++_toastId;
    toasts = [...toasts, { id, msg, type }];
    setTimeout(() => { toasts = toasts.filter(t => t.id !== id); }, 3500);
  }

  // Cache which file paths still exist (checked on open attempt)
  let missingFiles = $state<Set<string>>(new Set());

  let unlisten: UnlistenFn | undefined;

  // --- Derived ---
  let qualityPresets = $derived(ffmpegPath ? QUALITY_PRESETS : QUALITY_PRESETS_NO_FFMPEG);

  let availablePresets = $derived(
    videoInfo?.max_height
      ? qualityPresets.filter(p => p.height === 9999 || p.height <= (videoInfo?.max_height ?? 0))
      : [qualityPresets[0]]
  );

  let activeDownloads = $derived(
    Object.values(downloads).filter(d => ['queued', 'downloading', 'merging'].includes(d.status))
  );

  let recentFinished = $derived(
    Object.values(downloads).filter(d => ['finished', 'error', 'cancelled'].includes(d.status))
  );

  // Sync theme to <body>
  $effect(() => {
    document.body.dataset.theme = theme;
  });

  // --- Lifecycle ---
  onMount(async () => {
    // Restore theme from localStorage, fallback to system preference
    const saved = localStorage.getItem('vd-theme') as 'dark' | 'light' | null;
    if (saved) {
      theme = saved;
    } else if (window.matchMedia('(prefers-color-scheme: light)').matches) {
      theme = 'light';
    }
    document.body.dataset.theme = theme;

    settings = await invoke<AppSettings>('get_settings');
    audioFormat = settings.audio_format;
    history = await invoke<DownloadRecord[]>('get_history');
    ffmpegPath = await invoke<string | null>('get_ffmpeg_path');

    invoke<{ current_version: string; latest_version: string; update_available: boolean; release_url: string; release_notes: string }>('check_app_update')
      .then(info => {
        if (info.update_available) {
          appUpdateInfo = { current: info.current_version, latest: info.latest_version, available: true, releaseUrl: info.release_url, releaseNotes: info.release_notes };
        }
      })
      .catch(() => {});

    unlisten = await listen<ProgressPayload>('download-progress', (event) => {
      const p = event.payload;
      downloads = {
        ...downloads,
        [p.job_id]: {
          id: p.job_id,
          title: p.title || downloads[p.job_id]?.title || 'Downloading...',
          status: p.status,
          percent: p.percent,
          speed: p.speed,
          eta: p.eta,
          error: p.error,
        }
      };

      if (['finished', 'error', 'cancelled'].includes(p.status)) {
        refreshHistory();
        setTimeout(() => {
          const d = { ...downloads };
          delete d[p.job_id];
          downloads = d;
        }, 4000);
      }
    });

    listen<{ percent: number; status: string }>('ytdlp-update-progress', (event) => {
      updateProgress = event.payload;
    });
  });

  onDestroy(() => unlisten?.());

  // --- Functions ---
  function toggleTheme() {
    theme = theme === 'dark' ? 'light' : 'dark';
    localStorage.setItem('vd-theme', theme);
  }

  async function refreshHistory() {
    history = await invoke<DownloadRecord[]>('get_history');
  }

  async function probe() {
    const trimmed = url.trim();
    if (!trimmed) return;
    probing = true;
    probeError = '';
    videoInfo = null;
    selectedQuality = ffmpegPath ? 'bestvideo+bestaudio/best' : 'best';
    audioOnly = false;
    try {
      videoInfo = await invoke<VideoInfo>('probe_formats', { url: trimmed });
    } catch (e) {
      probeError = String(e);
    } finally {
      probing = false;
    }
  }

  function handleUrlKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') probe();
  }

  async function startDownload() {
    if (!videoInfo || !settings) return;
    startingDownload = true;
    downloadError = '';
    try {
      const opts: DownloadOptions = {
        url: videoInfo.url,
        format_id: audioOnly ? 'bestaudio' : selectedQuality,
        output_dir: settings.output_dir,
        audio_only: audioOnly,
        audio_format: audioOnly ? audioFormat : settings.audio_format,
        filename_template: settings.filename_template,
        title: videoInfo.title,
      };
      await invoke<string>('start_download', { opts });
      url = '';
      videoInfo = null;
      probeError = '';
    } catch (e) {
      downloadError = String(e);
    } finally {
      startingDownload = false;
    }
  }

  async function cancelDownload(jobId: string) {
    await invoke('cancel_download', { jobId });
  }

  async function pickOutputDir() {
    const dir = await invoke<string | null>('pick_output_dir');
    if (dir && settingsForm) settingsForm = { ...settingsForm, output_dir: dir };
  }

  async function pickCookieFile() {
    const path = await invoke<string | null>('pick_cookie_file');
    if (path && settingsForm) settingsForm = { ...settingsForm, cookie_source: path };
  }

  async function openSettings() {
    if (settings) {
      settingsForm = { ...settings };
      try {
        ytdlpVersion = await invoke<string>('check_ytdlp', { ytdlpPath: settingsForm.ytdlp_path });
      } catch {
        ytdlpVersion = 'Not found';
      }
      showSettings = true;
    }
  }

  async function checkYtdlp() {
    if (!settingsForm) return;
    try {
      ytdlpVersion = await invoke<string>('check_ytdlp', { ytdlpPath: settingsForm.ytdlp_path });
    } catch {
      ytdlpVersion = 'Not found';
    }
  }

  async function checkYtdlpUpdate() {
    checkingUpdate = true;
    updateInfo = null;
    try {
      const info = await invoke<{ current_version: string; latest_version: string; update_available: boolean }>('check_ytdlp_update');
      updateInfo = { current: info.current_version, latest: info.latest_version, available: info.update_available };
      if (!info.update_available) {
        showToast('yt-dlp đã là phiên bản mới nhất', 'info');
      }
    } catch (e) {
      showToast(String(e), 'error');
    } finally {
      checkingUpdate = false;
    }
  }

  async function startYtdlpUpdate() {
    updatingYtdlp = true;
    updateProgress = { percent: 0, status: 'Bắt đầu...' };
    try {
      const result = await invoke<string>('update_ytdlp');
      showToast(result, 'info');
      updateProgress = null;
      updateInfo = null;
      ytdlpVersion = await invoke<string>('check_ytdlp', { ytdlpPath: settingsForm!.ytdlp_path });
    } catch (e) {
      showToast(String(e), 'error');
    } finally {
      updatingYtdlp = false;
    }
  }

  async function checkForAppUpdate() {
    try {
      const info = await invoke<{ current_version: string; latest_version: string; update_available: boolean; release_url: string; release_notes: string }>('check_app_update');
      if (info.update_available) {
        appUpdateInfo = { current: info.current_version, latest: info.latest_version, available: true, releaseUrl: info.release_url, releaseNotes: info.release_notes };
      } else {
        showToast('Bạn đang dùng phiên bản mới nhất', 'info');
        appUpdateInfo = null;
      }
    } catch (e) {
      showToast(String(e), 'error');
    }
  }

  async function openReleasePage() {
    if (appUpdateInfo?.releaseUrl) {
      await invoke('open_release_page', { url: appUpdateInfo.releaseUrl });
    }
  }

  async function saveSettings() {
    if (!settingsForm) return;
    savingSettings = true;
    try {
      await invoke('save_settings', { settings: settingsForm });
      settings = { ...settingsForm };
      showSettings = false;
    } catch (e) {
      console.error(e);
    } finally {
      savingSettings = false;
    }
  }

  async function clearHistory() {
    await invoke('clear_history');
    history = [];
  }

  async function openFolder(path: string) {
    await invoke('open_folder', { path });
  }

  async function openFile(path: string) {
    try {
      await invoke('open_file', { path });
    } catch (e) {
      const msg = String(e);
      showToast(msg, 'error');
      // Mark as missing so icon changes
      missingFiles = new Set([...missingFiles, path]);
    }
  }

  function formatDuration(secs: number): string {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = Math.floor(secs % 60);
    if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
    return `${m}:${String(s).padStart(2, '0')}`;
  }

  function formatDate(iso: string): string {
    return new Date(iso).toLocaleDateString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit'
    });
  }

  function statusColor(status: string): string {
    return ({
      queued: '#94a3b8', downloading: '#6366f1', merging: '#f59e0b',
      finished: '#22c55e', error: '#ef4444', cancelled: '#64748b',
    } as Record<string, string>)[status] ?? '#94a3b8';
  }

  function statusLabel(status: string): string {
    return ({
      queued: 'Đang chờ', downloading: 'Đang tải', merging: 'Đang ghép',
      finished: 'Hoàn thành', error: 'Lỗi', cancelled: 'Đã huỷ',
    } as Record<string, string>)[status] ?? status;
  }
</script>

<!-- Settings Modal -->
{#if showSettings && settingsForm}
<div class="modal-overlay" onclick={() => (showSettings = false)} onkeydown={() => {}} role="dialog" aria-modal="true" tabindex="-1">
  <div class="modal" onclick={(e) => e.stopPropagation()} onkeydown={() => {}} role="none">
    <div class="modal-header">
      <h2>Cài đặt</h2>
      <button class="icon-btn" onclick={() => (showSettings = false)}>✕</button>
    </div>
    <div class="modal-body">
      <div class="form-group">
        <label for="s-output-dir">Thư mục lưu file</label>
        <div class="input-row">
          <input id="s-output-dir" type="text" bind:value={settingsForm.output_dir} readonly class="flex-1" />
          <button class="btn-secondary" onclick={pickOutputDir}>Chọn</button>
        </div>
      </div>

      <div class="form-group">
        <label for="s-ytdlp-path">Đường dẫn yt-dlp</label>
        <div class="input-row">
          <input id="s-ytdlp-path" type="text" bind:value={settingsForm.ytdlp_path} class="flex-1" />
          <button class="btn-secondary" onclick={checkYtdlp}>Kiểm tra</button>
        </div>
        {#if ytdlpVersion}
          <span class="hint" class:hint-ok={ytdlpVersion !== 'Not found'} class:hint-err={ytdlpVersion === 'Not found'}>
            {ytdlpVersion === 'Not found' ? '✕ Không tìm thấy' : `✓ Phiên bản ${ytdlpVersion}`}
          </span>
        {/if}
        <div class="ytdlp-update-section">
          {#if !updateInfo && !updatingYtdlp}
            <button class="btn-ghost" onclick={checkYtdlpUpdate} disabled={checkingUpdate}>
              {checkingUpdate ? 'Đang kiểm tra...' : 'Kiểm tra bản cập nhật'}
            </button>
          {/if}
          {#if updateInfo}
            {#if updateInfo.available}
              <div class="update-banner">
                <span>Có bản mới: <strong>v{updateInfo.latest}</strong> (hiện tại: v{updateInfo.current})</span>
                <button class="btn-primary btn-sm" onclick={startYtdlpUpdate} disabled={updatingYtdlp}>
                  {updatingYtdlp ? 'Đang cập nhật...' : 'Cập nhật ngay'}
                </button>
              </div>
            {:else}
              <span class="hint hint-ok">✓ yt-dlp đã là phiên bản mới nhất (v{updateInfo.latest})</span>
            {/if}
          {/if}
          {#if updateProgress}
            <div class="update-progress">
              <div class="update-progress-bar" style="width: {updateProgress.percent}%"></div>
              <span class="update-progress-text">{updateProgress.status}</span>
            </div>
          {/if}
        </div>
      </div>

      <div class="form-group">
        <label>FFmpeg</label>
        <span class="hint" class:hint-ok={ffmpegPath !== null} class:hint-err={ffmpegPath === null}>
          {#if ffmpegPath}
            ✓ Đã tìm thấy: <span class="mono">{ffmpegPath.split(/[/\\]/).pop()}</span>
          {:else}
            ✕ Không tìm thấy ffmpeg — video sẽ chỉ tải được chất lượng thấp hơn (không merge được video+audio riêng)
          {/if}
        </span>
      </div>

      <div class="form-group">
        <label for="s-cookies">Cookies (cho Facebook, cần đăng nhập)</label>
        <div class="input-row">
          <select id="s-cookies" bind:value={settingsForm.cookie_source} class="flex-1">
            {#each COOKIE_OPTIONS as opt}
              <option value={opt.value}>{opt.label}</option>
            {/each}
          </select>
          <button class="btn-secondary" onclick={pickCookieFile}>Chọn file</button>
        </div>
        <span class="hint">Đăng nhập Facebook trên Chrome, rồi chọn "Tự động (Chrome)". Nếu Chrome đang mở → đóng Chrome rồi tải lại, hoặc export cookies.txt bằng extension <a href="https://chromewebstore.google.com/detail/get-cookiestxt-locally/cclelndahbckbenkjhflpdbgdldlbecc" target="_blank" rel="noopener">Get cookies.txt LOCALLY</a>.</span>
        {#if settingsForm.cookie_source.startsWith('file:')}
          <span class="hint hint-ok">✓ Đã chọn: {settingsForm.cookie_source.slice(5).split(/[/\\]/).pop()}</span>
        {/if}
      </div>

      <div class="form-group">
        <label for="s-concurrency">Số download đồng thời</label>
        <select id="s-concurrency" bind:value={settingsForm.max_concurrent}>
          {#each [1, 2, 3, 4, 5] as n}
            <option value={n}>{n}</option>
          {/each}
        </select>
      </div>

      <div class="form-group">
        <label for="s-audio-fmt">Định dạng audio mặc định</label>
        <select id="s-audio-fmt" bind:value={settingsForm.audio_format}>
          {#each AUDIO_FORMATS as f}
            <option value={f}>{f.toUpperCase()}</option>
          {/each}
        </select>
      </div>

      <div class="form-group">
        <label for="s-filename">Template tên file</label>
        <input id="s-filename" type="text" bind:value={settingsForm.filename_template} />
        <span class="hint">Ví dụ: %(title)s.%(ext)s</span>
      </div>
    </div>
    <div class="modal-footer">
      <button class="btn-secondary" onclick={() => (showSettings = false)}>Huỷ</button>
      <button class="btn-primary" onclick={saveSettings} disabled={savingSettings}>
        {savingSettings ? 'Đang lưu...' : 'Lưu cài đặt'}
      </button>
    </div>
  </div>
</div>
{/if}

<!-- Main App -->
<div class="app">
  <!-- Header -->
  <header>
    <div class="header-logo">
      <span class="logo-icon">▶</span>
      <span class="logo-text">VideoDownload</span>
    </div>
    <div class="header-actions">
      <button class="icon-btn theme-toggle" onclick={toggleTheme} title={theme === 'dark' ? 'Chuyển Light Mode' : 'Chuyển Dark Mode'} aria-label="Toggle theme">
        {#if theme === 'dark'}
          <!-- Sun icon -->
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="5"/>
            <line x1="12" y1="1" x2="12" y2="3"/>
            <line x1="12" y1="21" x2="12" y2="23"/>
            <line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/>
            <line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/>
            <line x1="1" y1="12" x2="3" y2="12"/>
            <line x1="21" y1="12" x2="23" y2="12"/>
            <line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/>
            <line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>
          </svg>
        {:else}
          <!-- Moon icon -->
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
          </svg>
        {/if}
      </button>
      <button class="icon-btn" onclick={openSettings} title="Cài đặt" aria-label="Settings">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="3"/>
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
        </svg>
      </button>
    </div>
  </header>

  {#if appUpdateInfo?.available}
    <div class="app-update-banner">
      <div class="app-update-content">
        <span>🆕 Có bản mới <strong>v{appUpdateInfo.latest}</strong> (hiện tại: v{appUpdateInfo.current})</span>
        <div class="app-update-actions">
          <button class="btn-primary btn-sm" onclick={openReleasePage}>Tải về</button>
          <button class="btn-ghost btn-sm" onclick={() => (appUpdateInfo = null)}>Bỏ qua</button>
        </div>
      </div>
    </div>
  {/if}

  <main>
    <!-- URL Input -->
    <section class="url-section card">
      <div class="url-row">
        <input
          type="url"
          class="url-input"
          placeholder="Dán link YouTube, Facebook, TikTok..."
          bind:value={url}
          onkeydown={handleUrlKeydown}
          disabled={probing}
        />
        <button class="btn-primary probe-btn" onclick={probe} disabled={probing || !url.trim()}>
          {#if probing}
            <span class="spinner"></span> Đang tải...
          {:else}
            Lấy thông tin
          {/if}
        </button>
      </div>
      {#if probeError}
        <div class="error-msg">{probeError}</div>
      {/if}
    </section>

    <!-- Video Info Card -->
    {#if videoInfo}
      <section class="video-card card">
        <div class="video-info">
          {#if videoInfo.thumbnail}
            <img src={videoInfo.thumbnail} alt="thumbnail" class="thumbnail" />
          {:else}
            <div class="thumbnail-placeholder">▶</div>
          {/if}
          <div class="video-meta">
            <h3 class="video-title">{videoInfo.title}</h3>
            <div class="video-details">
              {#if videoInfo.duration}
                <span>⏱ {formatDuration(videoInfo.duration)}</span>
              {/if}
              {#if videoInfo.uploader}
                <span>👤 {videoInfo.uploader}</span>
              {/if}
              {#if videoInfo.max_height}
                <span>📺 Max {videoInfo.max_height}p</span>
              {/if}
            </div>
          </div>
        </div>

        <div class="download-controls">
          {#if !ffmpegPath && !audioOnly}
            <div class="ffmpeg-warning">
              <span>⚠ Không tìm thấy ffmpeg — chất lượng video bị giới hạn (không merge được video+audio). Vào <button class="link-btn" onclick={openSettings}>Cài đặt</button> để kiểm tra.</span>
            </div>
          {/if}
          <div class="controls-row">
            <div class="control-group">
              <label for="dl-quality">Chất lượng</label>
              {#if audioOnly}
                <select id="dl-quality" bind:value={audioFormat} disabled>
                  {#each AUDIO_FORMATS as f}
                    <option value={f}>{f.toUpperCase()}</option>
                  {/each}
                </select>
              {:else}
                <select id="dl-quality" bind:value={selectedQuality}>
                  {#each availablePresets as preset}
                    <option value={preset.value}>{preset.label}</option>
                  {/each}
                </select>
              {/if}
            </div>

            <div class="control-group">
              <span class="control-label">Chỉ âm thanh</span>
              <label class="toggle" aria-label="Chỉ âm thanh">
                <input type="checkbox" bind:checked={audioOnly} />
                <span class="toggle-slider"></span>
              </label>
            </div>

            {#if audioOnly}
              <div class="control-group">
                <label for="dl-audio-fmt">Định dạng</label>
                <select id="dl-audio-fmt" bind:value={audioFormat}>
                  {#each AUDIO_FORMATS as f}
                    <option value={f}>{f.toUpperCase()}</option>
                  {/each}
                </select>
              </div>
            {/if}
          </div>

          <div class="output-row">
            <span class="output-label">📁</span>
            <span class="output-path">{settings?.output_dir || '...'}</span>
            <button class="btn-ghost" onclick={openSettings}>Thay đổi</button>
          </div>

          {#if downloadError}
            <div class="error-msg">{downloadError}</div>
          {/if}

          <button class="btn-download" onclick={startDownload} disabled={startingDownload || !settings}>
            {startingDownload ? 'Đang thêm vào hàng chờ...' : '⬇ Tải xuống'}
          </button>
        </div>
      </section>
    {/if}

    <!-- Active Downloads -->
    {#if activeDownloads.length > 0 || recentFinished.length > 0}
      <section class="downloads-section">
        <h4 class="section-title">Đang tải ({activeDownloads.length})</h4>
        {#each activeDownloads as dl (dl.id)}
          <div class="download-item card">
            <div class="dl-header">
              <span class="dl-title">{dl.title}</span>
              <button class="icon-btn cancel-btn" onclick={() => cancelDownload(dl.id)} title="Huỷ" aria-label="Huỷ download">✕</button>
            </div>
            <div class="progress-bar">
              <div class="progress-fill" style="width: {dl.percent}%; background: {statusColor(dl.status)}"></div>
            </div>
            <div class="dl-stats">
              <span class="status-badge" style="color: {statusColor(dl.status)}">{statusLabel(dl.status)}</span>
              <span>{dl.percent.toFixed(1)}%</span>
              {#if dl.speed}<span>{dl.speed}</span>{/if}
              {#if dl.eta}<span>ETA {dl.eta}</span>{/if}
            </div>
          </div>
        {/each}

        {#each recentFinished as dl (dl.id)}
          <div class="download-item card finished">
            <div class="dl-header">
              <span class="dl-title">{dl.title}</span>
              <span class="status-badge" style="color: {statusColor(dl.status)}">{statusLabel(dl.status)}</span>
            </div>
            {#if dl.error}
              <div class="error-msg">{dl.error}</div>
            {/if}
          </div>
        {/each}
      </section>
    {/if}

    <!-- History -->
    <section class="history-section">
      <div class="section-header" onclick={() => (showHistory = !showHistory)} onkeydown={() => {}} role="button" tabindex="0">
        <h4 class="section-title">Lịch sử ({history.length})</h4>
        <div class="history-actions">
          {#if history.length > 0}
            <button class="btn-ghost danger" onclick={(e) => { e.stopPropagation(); clearHistory(); }}>Xoá tất cả</button>
          {/if}
          <span class="chevron">{showHistory ? '▲' : '▼'}</span>
        </div>
      </div>

      {#if showHistory}
        {#if history.length === 0}
          <div class="empty-state">Chưa có lịch sử tải xuống</div>
        {:else}
          <div class="history-list">
            {#each history as record (record.id)}
              <div class="history-item card" class:is-error={record.status === 'error'}>
                <div class="history-main">
                  <span class="status-dot" style="background: {statusColor(record.status)}"></span>
                  <div class="history-info">
                    <span class="history-title">{record.title || record.url}</span>
                    <span class="history-meta">
                      {record.audio_only ? '🎵 Audio' : '🎬 Video'} •
                      {formatDate(record.started_at)}
                      {#if record.error_msg}
                        • <span class="error-text">{record.error_msg.slice(0, 60)}</span>
                      {/if}
                    </span>
                    {#if record.output_path}
                      <span class="history-path" title={record.output_path}>📁 {record.output_path}</span>
                    {/if}
                  </div>
                </div>
                {#if record.output_path}
                  <div class="history-actions-btns">
                    <button
                      class="icon-btn file-open-btn"
                      class:missing={missingFiles.has(record.output_path)}
                      onclick={() => openFile(record.output_path!)}
                      title={missingFiles.has(record.output_path) ? 'File không còn tồn tại' : 'Mở file'}
                      aria-label="Mở file"
                    >
                      {#if record.audio_only}
                        <!-- Music note -->
                        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                          <path d="M9 18V5l12-2v13"/>
                          <circle cx="6" cy="18" r="3"/>
                          <circle cx="18" cy="16" r="3"/>
                        </svg>
                      {:else}
                        <!-- Play circle -->
                        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                          <circle cx="12" cy="12" r="10"/>
                          <polygon points="10 8 16 12 10 16 10 8"/>
                        </svg>
                      {/if}
                    </button>
                    <button
                      class="icon-btn"
                      onclick={() => openFolder(record.output_path!)}
                      title="Mở thư mục chứa file"
                      aria-label="Mở thư mục"
                    >
                      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                      </svg>
                    </button>
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      {/if}
    </section>
  </main>
</div>

<!-- Toast Notifications -->
<div class="toast-container">
  {#each toasts as toast (toast.id)}
    <div class="toast" class:toast-error={toast.type === 'error'}>
      {toast.msg}
    </div>
  {/each}
</div>

<style>
  /* ==================== CSS Variables ==================== */
  :global(body) {
    /* Dark theme (default) */
    --bg:               #0d0d0d;
    --surface:          #111827;
    --surface-input:    #0d0d0d;
    --border:           #1e293b;
    --border-strong:    #334155;
    --text:             #e2e8f0;
    --text-strong:      #f1f5f9;
    --text-muted:       #64748b;
    --text-subtle:      #475569;
    --text-path:        #94a3b8;
    --accent:           #6366f1;
    --accent-hover:     #5558e3;
    --btn-sec-bg:       #1e293b;
    --btn-sec-hover:    #2d3748;
    --btn-sec-text:     #e2e8f0;
    --btn-sec-border:   #334155;
    --toggle-off:       #334155;
    --progress-track:   #1e293b;
    --modal-bg:         #111827;
    --thumb-ph-bg:      #1e293b;

    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    font-size: 14px;
    line-height: 1.5;
    background: var(--bg);
    color: var(--text);
    min-height: 100vh;
    transition: background 0.25s, color 0.25s;
  }

  :global(body[data-theme="light"]) {
    --bg:               #f1f5f9;
    --surface:          #ffffff;
    --surface-input:    #f8fafc;
    --border:           #e2e8f0;
    --border-strong:    #cbd5e1;
    --text:             #334155;
    --text-strong:      #0f172a;
    --text-muted:       #64748b;
    --text-subtle:      #94a3b8;
    --text-path:        #64748b;
    --accent:           #6366f1;
    --accent-hover:     #4f46e5;
    --btn-sec-bg:       #f1f5f9;
    --btn-sec-hover:    #e2e8f0;
    --btn-sec-text:     #334155;
    --btn-sec-border:   #cbd5e1;
    --toggle-off:       #cbd5e1;
    --progress-track:   #e2e8f0;
    --modal-bg:         #ffffff;
    --thumb-ph-bg:      #e2e8f0;
  }

  :global(*, *::before, *::after) {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
  }

  /* ==================== Layout ==================== */
  .app {
    display: flex;
    flex-direction: column;
    min-height: 100vh;
    max-width: 860px;
    margin: 0 auto;
    padding: 0 16px 32px;
  }

  /* ==================== Header ==================== */
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 0;
    border-bottom: 1px solid var(--border);
    margin-bottom: 20px;
  }

  .header-logo {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .logo-icon {
    background: var(--accent);
    color: white;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    font-weight: bold;
  }

  .logo-text {
    font-size: 16px;
    font-weight: 700;
    color: var(--text-strong);
    letter-spacing: -0.3px;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  /* ==================== Cards ==================== */
  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 16px;
    margin-bottom: 12px;
    transition: background 0.25s, border-color 0.25s;
  }

  /* ==================== URL Section ==================== */
  .url-section { padding: 12px; }

  .url-row {
    display: flex;
    gap: 8px;
  }

  .url-input {
    flex: 1;
    background: var(--surface-input);
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    padding: 10px 14px;
    color: var(--text);
    font-size: 14px;
    outline: none;
    transition: border-color 0.2s, background 0.25s;
  }

  .url-input:focus { border-color: var(--accent); }
  .url-input::placeholder { color: var(--text-subtle); }
  .url-input:disabled { opacity: 0.6; }

  .probe-btn {
    white-space: nowrap;
    padding: 10px 16px;
  }

  /* ==================== Video Card ==================== */
  .video-info {
    display: flex;
    gap: 14px;
    margin-bottom: 14px;
  }

  .thumbnail {
    width: 140px;
    height: 80px;
    object-fit: cover;
    border-radius: 8px;
    flex-shrink: 0;
  }

  .thumbnail-placeholder {
    width: 140px;
    height: 80px;
    background: var(--thumb-ph-bg);
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 24px;
    flex-shrink: 0;
  }

  .video-meta { flex: 1; min-width: 0; }

  .video-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-strong);
    margin-bottom: 6px;
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .video-details {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    color: var(--text-muted);
    font-size: 12px;
  }

  /* ==================== Download Controls ==================== */
  .download-controls { display: flex; flex-direction: column; gap: 10px; }

  .controls-row {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: flex-end;
  }

  .control-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .control-group label, .control-label {
    font-size: 11px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-weight: 600;
  }

  select {
    background: var(--surface-input);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    color: var(--text);
    padding: 7px 10px;
    font-size: 13px;
    outline: none;
    cursor: pointer;
    transition: background 0.25s, border-color 0.2s;
  }

  select:focus { border-color: var(--accent); }

  .output-row {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--surface-input);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px 12px;
    font-size: 12px;
    transition: background 0.25s;
  }

  .output-label { color: var(--text-muted); }

  .output-path {
    flex: 1;
    color: var(--text-path);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ==================== Toggle Switch ==================== */
  .toggle {
    position: relative;
    display: inline-flex;
    align-items: center;
    cursor: pointer;
  }

  .toggle input { position: absolute; opacity: 0; width: 0; height: 0; }

  .toggle-slider {
    width: 36px;
    height: 20px;
    background: var(--toggle-off);
    border-radius: 20px;
    transition: background 0.2s;
    position: relative;
  }

  .toggle-slider::after {
    content: '';
    position: absolute;
    width: 14px;
    height: 14px;
    background: white;
    border-radius: 50%;
    top: 3px;
    left: 3px;
    transition: transform 0.2s;
    box-shadow: 0 1px 3px rgba(0,0,0,0.2);
  }

  .toggle input:checked + .toggle-slider { background: var(--accent); }
  .toggle input:checked + .toggle-slider::after { transform: translateX(16px); }

  /* ==================== Buttons ==================== */
  .btn-primary {
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 8px;
    padding: 8px 16px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.2s;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .btn-primary:hover:not(:disabled) { background: var(--accent-hover); }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

  .btn-secondary {
    background: var(--btn-sec-bg);
    color: var(--btn-sec-text);
    border: 1px solid var(--btn-sec-border);
    border-radius: 8px;
    padding: 7px 14px;
    font-size: 13px;
    cursor: pointer;
    transition: background 0.2s;
  }

  .btn-secondary:hover { background: var(--btn-sec-hover); }

  .btn-ghost {
    background: transparent;
    color: var(--accent);
    border: none;
    padding: 4px 8px;
    font-size: 12px;
    cursor: pointer;
    border-radius: 4px;
    transition: background 0.15s;
  }

  .btn-ghost:hover { background: var(--border); }
  .btn-ghost.danger { color: #ef4444; }
  .btn-ghost.danger:hover { background: rgba(239, 68, 68, 0.1); }

  .icon-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 6px;
    border-radius: 6px;
    font-size: 14px;
    transition: all 0.15s;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .icon-btn:hover { background: var(--border); color: var(--text); }

  .theme-toggle {
    color: var(--text-muted);
  }

  .theme-toggle:hover {
    color: var(--accent);
    background: var(--border);
  }

  .btn-download {
    background: linear-gradient(135deg, var(--accent), #8b5cf6);
    color: white;
    border: none;
    border-radius: 10px;
    padding: 12px 24px;
    font-size: 15px;
    font-weight: 700;
    cursor: pointer;
    width: 100%;
    transition: opacity 0.2s, transform 0.1s;
    letter-spacing: 0.3px;
  }

  .btn-download:hover:not(:disabled) { opacity: 0.92; transform: translateY(-1px); }
  .btn-download:active:not(:disabled) { transform: translateY(0); }
  .btn-download:disabled { opacity: 0.5; cursor: not-allowed; }

  /* ==================== Downloads Section ==================== */
  .section-title {
    font-size: 12px;
    font-weight: 700;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 8px;
  }

  .download-item { padding: 12px; }

  .dl-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
    gap: 8px;
  }

  .dl-title {
    font-size: 13px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .progress-bar {
    height: 4px;
    background: var(--progress-track);
    border-radius: 2px;
    overflow: hidden;
    margin-bottom: 6px;
  }

  .progress-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.4s ease;
  }

  .dl-stats {
    display: flex;
    gap: 12px;
    font-size: 11px;
    color: var(--text-muted);
  }

  .cancel-btn { color: #ef4444 !important; }
  .cancel-btn:hover { background: rgba(239, 68, 68, 0.1) !important; color: #ef4444 !important; }

  .finished { opacity: 0.8; }

  /* ==================== History ==================== */
  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    cursor: pointer;
    padding: 4px 0;
    margin-bottom: 8px;
    user-select: none;
  }

  .section-header .section-title { margin-bottom: 0; }

  .history-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .chevron { color: var(--text-subtle); font-size: 11px; }

  .history-list { display: flex; flex-direction: column; gap: 6px; }

  .history-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
  }

  .history-item.is-error { border-color: rgba(239, 68, 68, 0.3); }

  .history-main {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
    min-width: 0;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .history-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .history-title {
    font-size: 13px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .history-meta {
    font-size: 11px;
    color: var(--text-subtle);
    margin-top: 2px;
  }

  .history-path {
    font-size: 10px;
    color: var(--text-subtle);
    margin-top: 2px;
    opacity: 0.7;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 400px;
    display: block;
    cursor: default;
  }

  .error-text { color: #ef4444; }

  .empty-state {
    text-align: center;
    color: var(--text-subtle);
    padding: 24px;
    font-size: 13px;
  }

  /* ==================== Status Badge ==================== */
  .status-badge {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  /* ==================== Error ==================== */
  .error-msg {
    color: #fca5a5;
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: 6px;
    padding: 8px 12px;
    font-size: 12px;
    margin-top: 8px;
    line-height: 1.5;
  }

  /* ==================== Spinner ==================== */
  .spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin { to { transform: rotate(360deg); } }

  /* ==================== Modal ==================== */
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    backdrop-filter: blur(3px);
  }

  .modal {
    background: var(--modal-bg);
    border: 1px solid var(--border);
    border-radius: 16px;
    width: 480px;
    max-width: 95vw;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 24px 48px rgba(0, 0, 0, 0.3);
    transition: background 0.25s;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border);
  }

  .modal-header h2 {
    font-size: 16px;
    font-weight: 700;
    color: var(--text-strong);
  }

  .modal-body {
    padding: 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 16px 20px;
    border-top: 1px solid var(--border);
  }

  /* ==================== Form ==================== */
  .form-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .form-group label {
    font-size: 12px;
    color: var(--text-muted);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .input-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .flex-1 { flex: 1; }

  input[type="text"], input[type="url"] {
    background: var(--surface-input);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    padding: 8px 12px;
    color: var(--text);
    font-size: 13px;
    outline: none;
    width: 100%;
    transition: border-color 0.2s, background 0.25s;
  }

  input[type="text"]:focus, input[type="url"]:focus { border-color: var(--accent); }
  input[readonly] { cursor: default; color: var(--text-muted); }

  .hint { font-size: 11px; color: var(--text-subtle); }
  .hint-ok { color: #22c55e; }
  .hint-err { color: #ef4444; }
  .mono { font-family: monospace; font-size: 11px; background: var(--border); padding: 1px 4px; border-radius: 3px; }

  .ytdlp-update-section { margin-top: 8px; }
  .update-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    background: rgba(99, 102, 241, 0.08);
    border: 1px solid rgba(99, 102, 241, 0.2);
    border-radius: 8px;
    font-size: 12px;
    color: var(--text);
  }
  .update-banner strong { color: var(--accent); }
  .btn-sm { padding: 4px 12px; font-size: 11px; border-radius: 6px; }
  .update-progress {
    position: relative;
    margin-top: 8px;
    height: 22px;
    background: var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  .update-progress-bar {
    height: 100%;
    background: var(--accent);
    border-radius: 6px;
    transition: width 0.3s ease;
  }
  .update-progress-text {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    color: var(--text);
    font-weight: 500;
  }

  /* ==================== History File Buttons ==================== */
  .history-actions-btns {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  .file-open-btn {
    color: var(--accent);
  }

  .file-open-btn:hover:not(.missing) {
    background: rgba(99, 102, 241, 0.12);
    color: var(--accent);
  }

  .file-open-btn.missing {
    color: var(--text-subtle);
    cursor: not-allowed;
    opacity: 0.5;
  }

  /* ==================== Toast Notifications ==================== */
  .toast-container {
    position: fixed;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    z-index: 200;
    pointer-events: none;
  }

  .toast {
    background: var(--surface);
    border: 1px solid var(--border-strong);
    color: var(--text);
    padding: 10px 18px;
    border-radius: 8px;
    font-size: 13px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
    animation: toast-in 0.2s ease, toast-out 0.3s ease 3.2s forwards;
    white-space: nowrap;
    max-width: 420px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .toast-error {
    background: #1c0a0a;
    border-color: rgba(239, 68, 68, 0.4);
    color: #fca5a5;
  }

  :global(body[data-theme="light"]) .toast-error {
    background: #fff5f5;
    border-color: rgba(239, 68, 68, 0.3);
    color: #dc2626;
  }

  @keyframes toast-in {
    from { opacity: 0; transform: translateY(8px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  @keyframes toast-out {
    from { opacity: 1; }
    to   { opacity: 0; }
  }

  .ffmpeg-warning {
    background: rgba(245, 158, 11, 0.08);
    border: 1px solid rgba(245, 158, 11, 0.25);
    border-radius: 8px;
    padding: 8px 14px;
    font-size: 12px;
    color: #f59e0b;
    margin-bottom: 8px;
  }
  :global(body[data-theme="light"]) .ffmpeg-warning {
    background: #fffbeb;
    border-color: rgba(245, 158, 11, 0.3);
    color: #b45309;
  }
  .link-btn {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    font-size: inherit;
    font-weight: 600;
    text-decoration: underline;
    padding: 0;
  }

  .app-update-banner {
    background: linear-gradient(135deg, rgba(99, 102, 241, 0.1), rgba(139, 92, 246, 0.1));
    border-bottom: 1px solid rgba(99, 102, 241, 0.2);
    padding: 10px 20px;
  }
  :global(body[data-theme="light"]) .app-update-banner {
    background: linear-gradient(135deg, #eef2ff, #f5f3ff);
    border-bottom-color: rgba(99, 102, 241, 0.15);
  }
  .app-update-content {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    font-size: 13px;
    color: var(--text);
    max-width: 860px;
    margin: 0 auto;
  }
  .app-update-content strong { color: var(--accent); }
  .app-update-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
</style>
