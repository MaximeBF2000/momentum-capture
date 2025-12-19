import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useSettingsStore } from '../../../state/settingsStore'
import { updateImmersiveShortcut } from '../../../tauri/commands'

const DEFAULT_SHORTCUT = 'Option+I'

type Status = 'idle' | 'saving' | 'success' | 'error'

export function ImmersiveShortcutForm() {
  const { settings, setSettings } = useSettingsStore()
  const [shortcut, setShortcut] = useState(
    settings.immersiveShortcut || DEFAULT_SHORTCUT
  )
  const [status, setStatus] = useState<Status>('idle')
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const captureRef = useRef<HTMLButtonElement | null>(null)

  useEffect(() => {
    setShortcut(settings.immersiveShortcut || DEFAULT_SHORTCUT)
  }, [settings.immersiveShortcut])

  const handleShortcutCaptured = useCallback(
    (event: React.KeyboardEvent<HTMLButtonElement>) => {
      event.preventDefault()
      const next = buildShortcutString(event)
      if (!next) {
        return
      }
      setShortcut(next)
      setErrorMessage(null)
      setStatus('idle')
    },
    []
  )

  const persistShortcut = useCallback(
    async (value: string) => {
    setStatus('saving')
    setErrorMessage(null)
    try {
      await updateImmersiveShortcut(value)
      setSettings({ ...settings, immersiveShortcut: value })
      setShortcut(value)
      setStatus('success')
      setTimeout(() => setStatus('idle'), 2000)
    } catch (err) {
      console.error('Failed to update immersive shortcut', err)
      setStatus('error')
      setErrorMessage(
        err instanceof Error
          ? err.message
          : 'Unable to update shortcut. Try a different combination.'
      )
    }
  },
    [setSettings, settings]
  )

  const handleSave = useCallback(() => persistShortcut(shortcut), [
    persistShortcut,
    shortcut
  ])

  const handleReset = useCallback(async () => {
    await persistShortcut(DEFAULT_SHORTCUT)
  }, [persistShortcut])

  const statusText = useMemo(() => {
    switch (status) {
      case 'saving':
        return 'Saving…'
      case 'success':
        return 'Shortcut updated.'
      case 'error':
        return errorMessage ?? 'Unable to update shortcut.'
      default:
        return null
    }
  }, [errorMessage, status])

  const hasChanges = shortcut !== (settings.immersiveShortcut || DEFAULT_SHORTCUT)

  return (
    <div className="flex flex-col gap-4 bg-neutral-900/80 p-4 rounded-2xl border border-neutral-800">
      <div>
        <p className="text-sm text-neutral-400 mb-2">
          Click the field below, then press the key combination you want to use
          for toggling immersive mode.
        </p>
        <button
          ref={captureRef}
          type="button"
          onKeyDown={handleShortcutCaptured}
          className="w-full text-left font-mono text-base tracking-wide bg-neutral-800 border border-neutral-700 rounded-xl px-4 py-3 focus:outline-none focus:ring-2 focus:ring-blue-500"
        >
          {shortcut}
        </button>
      </div>

      <div className="flex gap-3">
          <button
            type="button"
            disabled={!hasChanges || status === 'saving'}
            onClick={handleSave}
            className="px-4 py-2 rounded-xl bg-blue-500 disabled:bg-neutral-700 disabled:text-neutral-400 text-white transition-colors"
          >
            Save Shortcut
          </button>
          <button
            type="button"
            disabled={shortcut === DEFAULT_SHORTCUT || status === 'saving'}
            onClick={handleReset}
            className="px-4 py-2 rounded-xl border border-neutral-700 text-neutral-200 hover:bg-neutral-800 transition-colors disabled:text-neutral-500 disabled:border-neutral-800"
          >
            Reset to Default
          </button>
      </div>

      {statusText && (
        <p
          className={`text-sm ${
            status === 'error' ? 'text-red-400' : 'text-green-400'
          }`}
        >
          {statusText}
        </p>
      )}
    </div>
  )
}

function buildShortcutString(
  event: React.KeyboardEvent<HTMLButtonElement>
): string | null {
  const key = normalizeKey(event.key)
  if (!key) {
    return null
  }

  const segments: string[] = []
  if (event.metaKey) segments.push('Command')
  if (event.ctrlKey) segments.push('Control')
  if (event.altKey) segments.push('Option')
  if (event.shiftKey) segments.push('Shift')
  segments.push(key)

  return segments.join('+')
}

function normalizeKey(key: string): string | null {
  const ignored = ['Shift', 'Control', 'Alt', 'Meta', 'Fn', 'Dead']
  if (ignored.includes(key)) {
    return null
  }

  if (key === ' ') {
    return 'Space'
  }

  if (key.length === 1) {
    return key.toUpperCase()
  }

  if (key.startsWith('Arrow')) {
    return key
  }

  return key.charAt(0).toUpperCase() + key.slice(1)
}
