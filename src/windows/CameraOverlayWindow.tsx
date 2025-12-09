import { CameraFrame } from '../components/camera/CameraFrame'
import '../App.css'

export function CameraOverlayWindow() {
  return (
    <div
      className="w-full h-full bg-transparent flex items-center justify-center p-2"
      data-tauri-drag-region="true"
    >
      <div className="w-full h-full rounded-2xl overflow-hidden bg-black shadow-lg">
        <div className="w-full h-full">
          <CameraFrame />
        </div>
      </div>
    </div>
  )
}
