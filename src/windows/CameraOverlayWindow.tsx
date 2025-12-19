import { CameraFrame } from '../components/camera/CameraFrame'
import { useEffect } from 'react'
import '../App.css'

export function CameraOverlayWindow() {
  useEffect(() => {
    console.log('[CameraOverlayWindow] Component mounted')
  }, [])

  return (
    <div className="w-full h-full bg-transparent flex items-center justify-center p-2">
      <div
        className="w-full h-full rounded-full overflow-hidden bg-black aspect-square select-none"
        data-tauri-drag-region="true"
        style={{ userSelect: 'none', WebkitUserSelect: 'none' }}
      >
        <div
          className="w-full h-full select-none pointer-events-none"
          style={{
            userSelect: 'none',
            WebkitUserSelect: 'none',
            pointerEvents: 'none'
          }}
        >
          <CameraFrame />
        </div>
      </div>
    </div>
  )
}
