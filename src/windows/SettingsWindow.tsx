import { useEffect, useMemo, useRef, useState } from 'react'
import {
  authorizeGoogleDrive,
  getDefaultSettings,
  getSettings,
  listGoogleDriveFolders,
  listGoogleDriveVideos,
  listCaptureDevices,
  setCameraOverlayVisible,
  toggleSettingsWindow,
  updateSettings
} from '../tauri/commands'
import { useSettingsStore } from '../state/settingsStore'
import type {
  AppSettings,
  CaptureDevices,
  DriveFolder,
  DriveVideo
} from '../types'
import {
  ArrowLeft,
  Check,
  Clipboard,
  Cloud,
  Film,
  Folder,
  RefreshCw,
  Search,
  X
} from 'lucide-react'
import {
  DeviceIcon,
  DeviceSelect,
  HideWebcamToggle,
  SettingsButton,
  SettingsDivider,
  SettingsRow,
  SettingsSection,
  SettingsToggle,
  ShortcutCaptureField
} from '../features/settings/components/SettingsControls'

export function SettingsWindow() {
  const { setSettings } = useSettingsStore()
  const [committed, setCommitted] = useState<AppSettings | null>(null)
  const [draft, setDraft] = useState<AppSettings | null>(null)
  const [defaults, setDefaults] = useState<AppSettings | null>(null)
  const [devices, setDevices] = useState<CaptureDevices | null>(null)
  const [driveFolders, setDriveFolders] = useState<DriveFolder[]>([])
  const [driveVideos, setDriveVideos] = useState<DriveVideo[]>([])
  const [view, setView] = useState<'settings' | 'drive-folder'>('settings')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [driveLoading, setDriveLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    Promise.all([getSettings(), getDefaultSettings(), listCaptureDevices()])
      .then(([loadedSettings, defaultSettings, loadedDevices]) => {
        const normalizedSettings = withResolvedDevices(
          loadedSettings,
          loadedDevices
        )
        setCommitted(normalizedSettings)
        setDraft(normalizedSettings)
        setDefaults(defaultSettings)
        setDevices(loadedDevices)
        setSettings(normalizedSettings)
        if (normalizedSettings.googleDrive.refreshToken) {
          void refreshDriveFolders()
        }
      })
      .catch(err => {
        console.error('Failed to load settings', err)
        setError(
          err instanceof Error ? err.message : 'Unable to load settings.'
        )
      })
      .finally(() => setLoading(false))
  }, [setSettings])

  const hasChanges = useMemo(() => {
    if (!draft || !committed) return false
    return JSON.stringify(draft) !== JSON.stringify(committed)
  }, [committed, draft])

  const updateDraft = <K extends keyof AppSettings>(
    key: K,
    value: AppSettings[K]
  ) => {
    setDraft(current => (current ? { ...current, [key]: value } : current))
  }

  const updateDriveDraft = <K extends keyof AppSettings['googleDrive']>(
    key: K,
    value: AppSettings['googleDrive'][K]
  ) => {
    setDraft(current =>
      current
        ? {
            ...current,
            googleDrive: {
              ...current.googleDrive,
              [key]: value
            }
          }
        : current
    )
  }

  const persistDraft = async (next: AppSettings) => {
    await updateSettings(next)
    setSettings(next)
    setCommitted(next)
    setDraft(next)
  }

  const handleSave = async () => {
    if (!draft || !committed) return

    setSaving(true)
    setError(null)
    try {
      await persistDraft(draft)

      if (
        committed.cameraEnabled &&
        (committed.cameraDeviceId !== draft.cameraDeviceId ||
          committed.hideWebcamOnImmersiveMode !==
            draft.hideWebcamOnImmersiveMode)
      ) {
        await setCameraOverlayVisible(false)
        await setCameraOverlayVisible(true)
      }
    } catch (err) {
      console.error('Failed to save settings', err)
      setError(err instanceof Error ? err.message : 'Unable to save settings.')
    } finally {
      setSaving(false)
    }
  }

  const handleAuthorizeDrive = async () => {
    if (!draft) return
    setSaving(true)
    setError(null)
    try {
      if (hasChanges) {
        await persistDraft(draft)
      }
      const driveSettings = await authorizeGoogleDrive()
      const latestSettings = {
        ...useSettingsStore.getState().settings,
        googleDrive: driveSettings
      }
      setDraft(latestSettings)
      setCommitted(latestSettings)
      setSettings(latestSettings)
      await refreshDriveFolders()
    } catch (err) {
      console.error('Failed to authorize Google Drive', err)
      setError(errorMessage(err, 'Unable to authorize Google Drive.'))
    } finally {
      setSaving(false)
    }
  }

  const refreshDriveFolders = async () => {
    setDriveLoading(true)
    setError(null)
    try {
      const folders = await listGoogleDriveFolders()
      setDriveFolders(folders)
    } catch (err) {
      console.error('Failed to load Drive folders', err)
      setError(errorMessage(err, 'Unable to load Drive folders.'))
    } finally {
      setDriveLoading(false)
    }
  }

  const openDriveFolderView = async () => {
    setView('drive-folder')
    setDriveLoading(true)
    setError(null)
    try {
      const videos = await listGoogleDriveVideos()
      setDriveVideos(videos)
    } catch (err) {
      console.error('Failed to load Drive videos', err)
      setError(errorMessage(err, 'Unable to load Drive videos.'))
    } finally {
      setDriveLoading(false)
    }
  }

  const copyLink = async (link?: string) => {
    if (!link) return
    await navigator.clipboard.writeText(link)
  }

  const handleReset = async () => {
    if (!defaults || !devices) return
    const nextSettings = withResolvedDevices(defaults, devices)
    setSaving(true)
    setError(null)

    try {
      await updateSettings(nextSettings)
      setDraft(nextSettings)
      setCommitted(nextSettings)
      setSettings(nextSettings)
      if (committed?.cameraEnabled) {
        await setCameraOverlayVisible(false)
        if (nextSettings.cameraEnabled) {
          await setCameraOverlayVisible(true)
        }
      }
    } catch (err) {
      console.error('Failed to reset settings', err)
      setError(err instanceof Error ? err.message : 'Unable to reset settings.')
    } finally {
      setSaving(false)
    }
  }

  if (loading || !draft) {
    return (
      <div className="flex h-full w-full items-center justify-center bg-transparent text-neutral-300">
        Loading settings...
      </div>
    )
  }

  return (
    <div
      className="h-full w-full bg-transparent p-1.5 text-white select-none"
      data-tauri-drag-region="true"
    >
      <div
        className="relative flex h-full flex-col overflow-hidden rounded-[22px]"
        style={{
          background: 'rgb(30, 30, 30)',
          // background:
          //   'radial-gradient(circle at 18% 18%, rgba(255,255,255,0.11), transparent 34%), radial-gradient(circle at 84% 82%, rgba(255,255,255,0.055), transparent 34%), rgba(14,14,14,0.96)',
          border: '1px solid rgba(255,255,255,0.18)'
        }}
      >
        <div className="h-12 w-full shrink-0" data-tauri-drag-region="true" />

        <button
          type="button"
          onClick={() => toggleSettingsWindow()}
          className="absolute right-5 top-5 z-10 flex h-10 w-10 items-center justify-center rounded-xl bg-white/8 text-neutral-300 transition-colors hover:bg-white/14 hover:text-white"
          aria-label="Close settings"
          data-tauri-drag-region="false"
        >
          <X className="h-5 w-5" />
        </button>

        <main className="flex min-h-0 flex-1 flex-col overflow-y-auto px-7 pb-7 pt-3">
          <div className="flex items-center gap-3">
            {view === 'drive-folder' && (
              <button
                type="button"
                onClick={() => setView('settings')}
                className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-white/10 text-neutral-100 transition-colors hover:bg-white/16"
                aria-label="Go back"
                data-tauri-drag-region="false"
              >
                <ArrowLeft className="h-5 w-5" />
              </button>
            )}
            <h1 className="min-w-0 truncate text-[22px] font-semibold leading-none tracking-tight">
              {view === 'settings' ? 'Settings' : 'Drive folder'}
            </h1>
          </div>

          {view === 'settings' ? (
            <div className="mt-7 space-y-8">
              <SettingsSection title="Capture">
                <SettingsRow
                  icon={<DeviceIcon type="microphone" />}
                  title="Microphone"
                  description="Select the microphone to use for recording."
                >
                  <DeviceSelect
                    label="Microphone"
                    value={draft.microphoneDeviceId ?? ''}
                    devices={devices?.microphones ?? []}
                    onChange={value => updateDraft('microphoneDeviceId', value)}
                  />
                </SettingsRow>
                <SettingsDivider />
                <SettingsRow
                  icon={<DeviceIcon type="camera" />}
                  title="Camera"
                  description="Select the camera to use for recording."
                >
                  <DeviceSelect
                    label="Camera"
                    value={draft.cameraDeviceId ?? ''}
                    devices={devices?.cameras ?? []}
                    onChange={value => updateDraft('cameraDeviceId', value)}
                  />
                </SettingsRow>
              </SettingsSection>

              <SettingsSection title="Immersive Mode">
                <HideWebcamToggle
                  checked={draft.hideWebcamOnImmersiveMode}
                  onChange={checked =>
                    updateDraft('hideWebcamOnImmersiveMode', checked)
                  }
                />
                <SettingsDivider />
                <SettingsRow
                  icon={null}
                  title="Toggle Shortcut"
                  description="Press the keyboard shortcut to toggle immersive mode on or off during a recording."
                >
                  <ShortcutCaptureField
                    value={draft.immersiveShortcut}
                    onChange={value => updateDraft('immersiveShortcut', value)}
                  />
                </SettingsRow>
              </SettingsSection>

              <SettingsSection title="Google Drive Integration">
                <SettingsToggle
                  icon={<Cloud className="h-5 w-5" />}
                  title="Upload to google drive"
                  description="Upload finished recordings to Drive and copy a public link."
                  checked={draft.googleDrive.enabled}
                  onChange={checked => updateDriveDraft('enabled', checked)}
                />
                <SettingsDivider />
                <SettingsToggle
                  icon={<Folder className="h-5 w-5" />}
                  title="Save locally"
                  description="Keep a copy in Downloads or the configured save location."
                  checked={draft.saveRecordingsLocally}
                  onChange={checked =>
                    updateDraft('saveRecordingsLocally', checked)
                  }
                />
                <SettingsDivider />
                <SettingsRow
                  icon={<Cloud className="h-5 w-5" />}
                  title="Authorization"
                  description={
                    draft.googleDrive.refreshToken
                      ? draft.googleDrive.accountEmail
                        ? `Logged in as ${draft.googleDrive.accountEmail}`
                        : 'Logged in to Google Drive.'
                      : 'Authorize before loading folders or uploading.'
                  }
                >
                  <SettingsButton
                    icon={<Cloud className="h-4 w-4" />}
                    disabled={saving}
                    onClick={handleAuthorizeDrive}
                  >
                    {draft.googleDrive.refreshToken
                      ? 'Re-authorize'
                      : 'Authorize'}
                  </SettingsButton>
                </SettingsRow>
                <SettingsDivider />
                <SettingsRow
                  icon={<Folder className="h-5 w-5" />}
                  title="Drive folder"
                  description={
                    draft.googleDrive.folderName ||
                    'Choose where public videos are uploaded.'
                  }
                >
                  <div className="flex gap-2">
                    <DriveFolderCombobox
                      folders={driveFolders}
                      value={draft.googleDrive.folderId}
                      selectedName={draft.googleDrive.folderName}
                      disabled={driveLoading || !draft.googleDrive.refreshToken}
                      onSelect={folder => {
                        updateDriveDraft('folderId', folder.id)
                        updateDriveDraft('folderName', folder.name)
                      }}
                    />
                    <SettingsButton
                      ariaLabel="Refresh Drive folders"
                      icon={<RefreshCw className="h-4 w-4" />}
                      disabled={driveLoading || !draft.googleDrive.refreshToken}
                      onClick={refreshDriveFolders}
                    />
                  </div>
                </SettingsRow>
                <SettingsDivider />
                <SettingsRow
                  icon={<Clipboard className="h-5 w-5" />}
                  title="Public videos"
                  description="Review videos in the selected folder and copy their links."
                >
                  <SettingsButton
                    icon={<Clipboard className="h-4 w-4" />}
                    disabled={!draft.googleDrive.folderId}
                    onClick={openDriveFolderView}
                  >
                    Open folder page
                  </SettingsButton>
                </SettingsRow>
              </SettingsSection>
            </div>
          ) : (
            <DriveFolderPage
              videos={driveVideos}
              loading={driveLoading}
              folderName={draft.googleDrive.folderName}
              onRefresh={openDriveFolderView}
              onCopy={copyLink}
            />
          )}

          {error && (
            <div className="mt-5 rounded-xl border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-300">
              {error}
            </div>
          )}

          {view === 'settings' && (
            <div
              className="mt-7 flex shrink-0 gap-4 pt-1"
              data-tauri-drag-region="false"
            >
              <button
                type="button"
                disabled={!hasChanges || saving}
                onClick={handleSave}
                className="h-10 rounded-lg bg-blue-600 px-5 text-[15px] font-medium text-white shadow-lg shadow-blue-950/30 transition-colors hover:bg-blue-500 disabled:cursor-not-allowed disabled:bg-white/10 disabled:text-neutral-500 disabled:shadow-none"
              >
                {saving ? 'Saving...' : 'Save settings'}
              </button>
              <button
                type="button"
                disabled={saving}
                onClick={handleReset}
                className="h-10 rounded-lg bg-white/14 px-5 text-[15px] font-medium text-neutral-100 transition-colors hover:bg-white/20 disabled:cursor-not-allowed disabled:text-neutral-500"
              >
                Reset to Defaults
              </button>
            </div>
          )}
        </main>
      </div>
    </div>
  )
}

function DriveFolderPage({
  videos,
  loading,
  folderName,
  onRefresh,
  onCopy
}: {
  videos: DriveVideo[]
  loading: boolean
  folderName?: string
  onRefresh: () => void
  onCopy: (link?: string) => Promise<void>
}) {
  const [copiedVideoId, setCopiedVideoId] = useState<string | null>(null)
  const copiedTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    return () => {
      if (copiedTimer.current) clearTimeout(copiedTimer.current)
    }
  }, [])

  const handleCopy = async (video: DriveVideo) => {
    await onCopy(video.webViewLink)
    setCopiedVideoId(video.id)
    if (copiedTimer.current) clearTimeout(copiedTimer.current)
    copiedTimer.current = setTimeout(() => setCopiedVideoId(null), 2000)
  }

  return (
    <div className="mt-7 space-y-4" data-tauri-drag-region="false">
      <div className="flex items-center justify-between gap-3">
        <p className="min-w-0 truncate text-[15px] text-neutral-400">
          {folderName ? `Public videos in ${folderName}` : 'Public videos'}
        </p>
        <SettingsButton
          icon={<RefreshCw className="h-4 w-4" />}
          disabled={loading}
          onClick={onRefresh}
        >
          Refresh
        </SettingsButton>
      </div>

      <div
        className="overflow-hidden rounded-[16px]"
        style={{
          background: 'rgba(255,255,255,0.045)',
          border: '1px solid rgba(255,255,255,0.12)'
        }}
      >
        {loading ? (
          <div className="px-5 py-8 text-[15px] text-neutral-400">
            Loading videos...
          </div>
        ) : videos.length === 0 ? (
          <div className="px-5 py-8 text-[15px] text-neutral-400">
            No videos found in this folder.
          </div>
        ) : (
          videos.map((video, index) => (
            <div key={video.id}>
              <div className="grid min-h-[84px] grid-cols-[72px_minmax(0,1fr)_auto] items-center gap-4 px-5 py-4">
                <div className="flex h-[45px] w-[72px] items-center justify-center overflow-hidden rounded-lg bg-black/40">
                  {video.thumbnailLink ? (
                    <img
                      src={video.thumbnailLink}
                      alt=""
                      className="h-full w-full object-cover"
                      draggable={false}
                    />
                  ) : (
                    <Film className="h-5 w-5 text-neutral-500" />
                  )}
                </div>
                <div className="min-w-0">
                  <h3 className="truncate text-[15px] font-semibold text-white">
                    {video.name}
                  </h3>
                  <p className="mt-1 text-[13px] text-neutral-500">
                    {formatDriveDate(video.createdTime ?? video.modifiedTime)}
                  </p>
                </div>
                <SettingsButton
                  ariaLabel={`Copy link for ${video.name}`}
                  icon={
                    copiedVideoId === video.id ? (
                      <Check className="h-4 w-4" />
                    ) : (
                      <Clipboard className="h-4 w-4" />
                    )
                  }
                  disabled={!video.webViewLink}
                  onClick={() => handleCopy(video)}
                >
                  {copiedVideoId === video.id ? 'Copied' : 'Copy link'}
                </SettingsButton>
              </div>
              {index < videos.length - 1 && <SettingsDivider />}
            </div>
          ))
        )}
      </div>
    </div>
  )
}

function DriveFolderCombobox({
  folders,
  value,
  selectedName,
  disabled,
  onSelect
}: {
  folders: DriveFolder[]
  value?: string
  selectedName?: string
  disabled?: boolean
  onSelect: (folder: DriveFolder) => void
}) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState(selectedName ?? '')

  useEffect(() => {
    if (!open) {
      setQuery(selectedName ?? '')
    }
  }, [open, selectedName])

  const filtered = folders
    .filter(folder => folder.name.toLowerCase().includes(query.toLowerCase()))
    .slice(0, 8)

  return (
    <div className="relative min-w-0 flex-1">
      <div
        className="flex h-[38px] items-center gap-2 rounded-lg px-3"
        style={{
          background: 'rgba(18,18,18,0.56)',
          border: '1px solid rgba(255,255,255,0.15)'
        }}
      >
        <Search className="h-4 w-4 shrink-0 text-neutral-500" />
        <input
          aria-label="Search Drive folders"
          value={query}
          disabled={disabled}
          placeholder={
            disabled ? 'Authorize Google Drive first' : 'Search folders'
          }
          onFocus={event => {
            event.currentTarget.select()
            setOpen(true)
          }}
          onBlur={() => {
            window.setTimeout(() => setOpen(false), 150)
          }}
          onChange={event => {
            setQuery(event.currentTarget.value)
            setOpen(true)
          }}
          className="min-w-0 flex-1 bg-transparent text-[14px] font-medium text-white outline-none placeholder:text-neutral-600 disabled:cursor-not-allowed disabled:text-neutral-500"
          data-tauri-drag-region="false"
        />
      </div>
      {open && !disabled && (
        <div
          className="absolute left-0 right-0 top-11 z-20 max-h-40 overflow-y-auto rounded-xl py-1 shadow-2xl shadow-black/40"
          style={{
            background: 'rgb(28,28,28)',
            border: '1px solid rgba(255,255,255,0.14)'
          }}
          data-tauri-drag-region="false"
        >
          {filtered.length === 0 ? (
            <div className="px-3 py-3 text-[14px] text-neutral-500">
              No matching folders
            </div>
          ) : (
            filtered.map(folder => (
              <button
                key={folder.id}
                type="button"
                onMouseDown={event => {
                  event.preventDefault()
                  onSelect(folder)
                  setQuery(folder.name)
                  setOpen(false)
                }}
                className="flex h-10 w-full items-center justify-between gap-3 px-3 text-left text-[14px] text-neutral-100 transition-colors hover:bg-white/10"
              >
                <span className="min-w-0 truncate">{folder.name}</span>
                {value === folder.id && (
                  <Check className="h-4 w-4 shrink-0 text-blue-400" />
                )}
              </button>
            ))
          )}
        </div>
      )}
    </div>
  )
}

function formatDriveDate(value?: string) {
  if (!value) return 'No date'
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short'
  }).format(new Date(value))
}

function errorMessage(err: unknown, fallback: string) {
  if (err instanceof Error) return err.message
  if (typeof err === 'string' && err.trim()) return err
  return fallback
}

function withResolvedDevices(
  settings: AppSettings,
  devices: CaptureDevices
): AppSettings {
  return {
    ...settings,
    microphoneDeviceId: devices.microphones.some(
      device => device.id === settings.microphoneDeviceId
    )
      ? settings.microphoneDeviceId
      : devices.selectedMicrophoneId,
    cameraDeviceId: devices.cameras.some(
      device => device.id === settings.cameraDeviceId
    )
      ? settings.cameraDeviceId
      : devices.selectedCameraId
  }
}
