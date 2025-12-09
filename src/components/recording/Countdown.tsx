import { useRecordingStore } from '../../state/recordingStore'

export function Countdown() {
  const countdownSecondsRemaining = useRecordingStore(
    state => state.countdownSecondsRemaining
  )

  if (countdownSecondsRemaining === null) {
    return null
  }

  return (
    <div className="absolute inset-0 flex items-center justify-center bg-neutral-900/98 backdrop-blur-md rounded-full z-50">
      <span className="text-white text-3xl font-bold">
        {countdownSecondsRemaining}
      </span>
    </div>
  )
}
