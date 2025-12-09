import { useEffect, useState } from 'react'
import { subscribeToRecordingEvents } from '../../tauri/events'
import type { CameraFrame as CameraFrameType } from '../../types'

export function CameraFrame() {
  const [currentFrameUrl, setCurrentFrameUrl] = useState<string | null>(null)
  const [hasReceivedFrame, setHasReceivedFrame] = useState(false)

  useEffect(() => {
    const unsubscribe = subscribeToRecordingEvents({
      onCameraFrame: (frame: CameraFrameType) => {
        console.log('Camera frame received:', frame.id)
        const dataUrl = `data:image/${frame.format};base64,${frame.data_base64}`
        setCurrentFrameUrl(dataUrl)
        setHasReceivedFrame(true)
      }
    })

    return unsubscribe
  }, [])

  if (!hasReceivedFrame) {
    return (
      <div className="w-full h-full flex items-center justify-center bg-black rounded-full">
        <span className="text-neutral-500 text-sm">Camera Loading...</span>
      </div>
    )
  }

  if (!currentFrameUrl) {
    return (
      <div className="w-full h-full flex items-center justify-center bg-black rounded-full">
        <span className="text-neutral-500 text-sm">Camera Off</span>
      </div>
    )
  }

  return (
    <img
      src={currentFrameUrl}
      alt="Camera preview"
      className="w-full h-full object-cover rounded-full"
    />
  )
}
