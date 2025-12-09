import { useEffect, useState } from 'react'
import { subscribeToRecordingEvents } from '../../tauri/events'
import type { CameraFrame as CameraFrameType } from '../../types'

export function CameraFrame() {
  const [currentFrameUrl, setCurrentFrameUrl] = useState<string | null>(null)
  const [hasReceivedFrame, setHasReceivedFrame] = useState(false)

  useEffect(() => {
    console.log(
      '[CameraFrame] Component mounted, subscribing to camera frames...'
    )
    const unsubscribe = subscribeToRecordingEvents({
      onCameraFrame: (frame: CameraFrameType) => {
        console.log('[CameraFrame] Frame received:', frame.id)
        const dataUrl = `data:image/${frame.format};base64,${frame.data_base64}`
        setCurrentFrameUrl(dataUrl)
        setHasReceivedFrame(true)
      },
      onCameraError: payload => {
        console.error('[CameraFrame] Camera error:', payload.message)
        setHasReceivedFrame(false)
        setCurrentFrameUrl(null)
      }
    })

    // Log if no frames received after 5 seconds
    const timeout = setTimeout(() => {
      if (!hasReceivedFrame) {
        console.warn(
          '[CameraFrame] WARNING: No camera frames received after 5 seconds. Check if camera preview is running.'
        )
      }
    }, 5000)

    return () => {
      clearTimeout(timeout)
      unsubscribe()
    }
  }, [hasReceivedFrame])

  if (!hasReceivedFrame) {
    return (
      <div className="w-full h-full flex items-center justify-center bg-black rounded-full pointer-events-none">
        <span className="text-neutral-500 text-sm pointer-events-none">
          Camera Loading...
        </span>
      </div>
    )
  }

  if (!currentFrameUrl) {
    return (
      <div className="w-full h-full flex items-center justify-center bg-black rounded-full pointer-events-none">
        <span className="text-neutral-500 text-sm pointer-events-none">
          Camera Off
        </span>
      </div>
    )
  }

  return (
    <img
      src={currentFrameUrl}
      alt="Camera preview"
      className="w-full h-full object-cover rounded-full select-none pointer-events-none"
      style={{
        userSelect: 'none',
        WebkitUserSelect: 'none',
        pointerEvents: 'none'
      }}
      draggable={false}
    />
  )
}
