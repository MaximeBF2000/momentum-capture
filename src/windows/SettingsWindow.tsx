import { useEffect, useState } from 'react'
import { useSettingsStore } from '../state/settingsStore'
import { getSettings } from '../tauri/commands'
import { ImmersiveShortcutForm } from '../features/settings/components/ImmersiveShortcutForm'

export function SettingsWindow() {
  const { settings, setSettings } = useSettingsStore()
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    getSettings()
      .then(setSettings)
      .catch(err => {
        console.error('Failed to load settings', err)
        setError(
          err instanceof Error ? err.message : 'Unable to load settings.'
        )
      })
      .finally(() => setLoading(false))
  }, [setSettings])

  if (loading) {
    return (
      <div className="w-full h-full flex items-center justify-center bg-neutral-950 text-neutral-300">
        Loading settings…
      </div>
    )
  }

  if (error) {
    return (
      <div className="w-full h-full flex items-center justify-center bg-neutral-950 text-red-400 px-4 text-center">
        {error}
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-neutral-950 text-neutral-100 p-6 select-none">
      <div className="max-w-xl mx-auto flex flex-col gap-6">
        <header>
          <p className="text-sm uppercase tracking-widest text-neutral-500 mb-1">
            Momentum
          </p>
          <h1 className="text-2xl font-semibold mb-2">Settings</h1>
          <p className="text-neutral-400 leading-relaxed">
            Customize keyboard shortcuts and behavior for immersive mode. Changes
            apply immediately and do not restart your recording.
          </p>
        </header>

        <section className="flex flex-col gap-4">
          <div>
            <h2 className="text-lg font-medium">Immersive Mode Shortcut</h2>
            <p className="text-sm text-neutral-500">
              Current value: {settings.immersiveShortcut}
            </p>
          </div>
          <ImmersiveShortcutForm />
        </section>
      </div>
    </div>
  )
}
