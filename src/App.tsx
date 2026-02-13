import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { OverlayWindow } from './windows/OverlayWindow'
import { CameraOverlayWindow } from './windows/CameraOverlayWindow'
import { SettingsWindow } from './windows/SettingsWindow'
import { subscribeToSettingsEvents } from './tauri/events'
import { useSettingsStore } from './state/settingsStore'

function App() {
  const [windowLabel, setWindowLabel] = useState<string | null>(null)
  const { setSettings } = useSettingsStore()

  useEffect(() => {
    try {
      const window = getCurrentWindow()
      setWindowLabel(window.label)
    } catch (err: unknown) {
      console.error('Failed to get window label:', err)
      // Default to overlay if we can't determine
      setWindowLabel('overlay')
    }
  }, [])

  useEffect(() => {
    const unsubscribe = subscribeToSettingsEvents({
      onUpdated: settings => {
        setSettings(settings)
      }
    })

    return () => {
      unsubscribe()
    }
  }, [setSettings])

  if (windowLabel === null) {
    return <div>Loading...</div>
  }

  if (windowLabel === 'camera-overlay') {
    return <CameraOverlayWindow />
  }

  if (windowLabel === 'settings') {
    return <SettingsWindow />
  }

  return <OverlayWindow />
}

export default App
