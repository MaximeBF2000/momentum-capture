import { useRecordingStore } from '../../state/recordingStore'

export function TimerDisplay() {
  const elapsedTimeMs = useRecordingStore(state => state.elapsedTimeMs)

  const formatTime = (ms: number): string => {
    const totalSeconds = Math.floor(ms / 1000)
    const hours = Math.floor(totalSeconds / 3600)
    const minutes = Math.floor((totalSeconds % 3600) / 60)
    const seconds = totalSeconds % 60

    return `${hours.toString().padStart(2, '0')}:${minutes
      .toString()
      .padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`
  }

  return (
    <span className="font-mono text-white text-sm">
      {formatTime(elapsedTimeMs)}
    </span>
  )
}
