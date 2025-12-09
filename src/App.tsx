import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { OverlayWindow } from './windows/OverlayWindow'
import { CameraOverlayWindow } from './windows/CameraOverlayWindow'

function App() {
  const [windowLabel, setWindowLabel] = useState<string | null>(null)

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

  if (windowLabel === null) {
    return <div>Loading...</div>
  }

  if (windowLabel === 'camera-overlay') {
    return <CameraOverlayWindow />
  }

  return <OverlayWindow />
}

export default App
