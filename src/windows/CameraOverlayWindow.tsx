import { CameraFrame } from '../components/camera/CameraFrame'
import '../App.css'

export function CameraOverlayWindow() {
  return (
    <div className="w-full h-full bg-transparent flex items-center justify-center p-2">
      <div
        className="w-full h-full rounded-full overflow-hidden bg-black shadow-lg aspect-square select-none"
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
