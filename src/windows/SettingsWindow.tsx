import { useEffect, useMemo, useState } from 'react'
import {
  getDefaultSettings,
  getSettings,
  listCaptureDevices,
  setCameraOverlayVisible,
  toggleSettingsWindow,
  updateSettings
} from '../tauri/commands'
import { useSettingsStore } from '../state/settingsStore'
import type { AppSettings, CaptureDevices } from '../types'
import { X } from 'lucide-react'
import {
  DeviceIcon,
  DeviceSelect,
  HideWebcamToggle,
  SettingsDivider,
  SettingsRow,
  SettingsSection,
  ShortcutCaptureField
} from '../features/settings/components/SettingsControls'

export function SettingsWindow() {
  const { setSettings } = useSettingsStore()
  const [committed, setCommitted] = useState<AppSettings | null>(null)
  const [draft, setDraft] = useState<AppSettings | null>(null)
  const [defaults, setDefaults] = useState<AppSettings | null>(null)
  const [devices, setDevices] = useState<CaptureDevices | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
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

  const handleSave = async () => {
    if (!draft || !committed) return

    setSaving(true)
    setError(null)
    try {
      await updateSettings(draft)
      setSettings(draft)
      setCommitted(draft)

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
          <h1 className="text-[22px] font-semibold leading-none tracking-tight">
            Settings
          </h1>

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
          </div>

          {error && (
            <div className="mt-5 rounded-xl border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-300">
              {error}
            </div>
          )}

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
        </main>
      </div>
    </div>
  )
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
