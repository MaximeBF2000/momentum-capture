import {
  Pause,
  Play,
  Square,
  Mic,
  MicOff,
  Camera,
  CameraOff
} from 'lucide-react'
import { useRecordingStore } from '../../state/recordingStore'
import { TimerDisplay } from './TimerDisplay'
import { Countdown } from './Countdown'
import {
  pauseRecording,
  resumeRecording,
  stopRecording,
  startRecording,
  setCameraOverlayVisible,
  updateSettings
} from '../../tauri/commands'
import { useSettingsStore } from '../../state/settingsStore'
import { useEffect } from 'react'

export function ControlBar() {
  const {
    recordingState,
    isMicEnabled,
    isCameraEnabled,
    startCountdown,
    setRecordingState,
    toggleMic,
    toggleCamera,
    setError
  } = useRecordingStore()

  const { settings, updateSetting } = useSettingsStore()

  useEffect(() => {
    // Sync store with settings
    useRecordingStore.setState({
      isMicEnabled: settings.micEnabled,
      isCameraEnabled: settings.cameraEnabled
    })
  }, [settings])

  const handleStart = async () => {
    try {
      startCountdown()

      // Countdown logic - use ref to store interval
      let countdownInterval: ReturnType<typeof setInterval> | null = null

      const tickAndCheck = async () => {
        const state = useRecordingStore.getState()
        const currentCountdown = state.countdownSecondsRemaining

        console.log('Countdown tick, remaining:', currentCountdown)

        // If countdown is done, start recording
        if (currentCountdown === null || currentCountdown <= 0) {
          if (countdownInterval) {
            clearInterval(countdownInterval)
            countdownInterval = null
          }

          // Start recording - set state optimistically to avoid UI delay
          try {
            const recordingState = useRecordingStore.getState()
            console.log(
              'Countdown finished, starting recording with options:',
              {
                mic: recordingState.isMicEnabled,
                camera: recordingState.isCameraEnabled
              }
            )
            // Set state to recording immediately to enable buttons and start timer
            setRecordingState('recording')
            await startRecording({
              includeMicrophone: recordingState.isMicEnabled,
              includeCamera: recordingState.isCameraEnabled
            })
            console.log('Recording command sent successfully')
          } catch (err: any) {
            console.error('Failed to start recording:', err)
            setError(err.message || 'Failed to start recording')
            useRecordingStore.getState().reset()
          }
          return
        }

        // Continue countdown
        useRecordingStore.getState().tickCountdown()
      }

      // Start countdown immediately (shows 3)
      // Don't tick immediately - let the UI show 3 first
      // Then tick every second starting after 1 second
      countdownInterval = setInterval(() => {
        tickAndCheck()
      }, 1000)

      // Store interval in a way that can be cleaned up
      return () => {
        if (countdownInterval) {
          clearInterval(countdownInterval)
        }
      }
    } catch (err: any) {
      console.error('Error in handleStart:', err)
      setError(err.message || 'Failed to start recording')
    }
  }

  const handlePause = async () => {
    try {
      console.log('Handle pause called, current state:', recordingState)
      if (recordingState === 'recording') {
        console.log('Pausing recording...')
        await pauseRecording()
      } else if (recordingState === 'paused') {
        console.log('Resuming recording...')
        await resumeRecording()
      }
    } catch (err: any) {
      console.error('Pause/resume error:', err)
      setError(err.message || 'Failed to pause/resume recording')
    }
  }

  const handleStop = async () => {
    try {
      console.log('Handle stop called, current state:', recordingState)
      setRecordingState('stopping')
      await stopRecording()
      console.log('Stop recording command sent')
    } catch (err: any) {
      console.error('Stop error:', err)
      setError(err.message || 'Failed to stop recording')
      setRecordingState('idle')
    }
  }

  const handleMicToggle = async () => {
    const newValue = !isMicEnabled
    toggleMic()
    updateSetting('micEnabled', newValue)

    // If recording, toggle microphone in the active recording
    if (recordingState === 'recording' || recordingState === 'paused') {
      try {
        const { toggleMicrophoneDuringRecording } = await import(
          '../../tauri/commands'
        )
        await toggleMicrophoneDuringRecording(newValue)
      } catch (err) {
        console.error('Failed to toggle microphone during recording:', err)
      }
    }

    // Update settings in backend
    try {
      await updateSettings({ ...settings, micEnabled: newValue })
    } catch (err) {
      console.error('Failed to update mic setting:', err)
    }
  }

  const handleCameraToggle = async () => {
    toggleCamera()
    const newValue = !isCameraEnabled
    updateSetting('cameraEnabled', newValue)
    // Update settings and camera overlay visibility
    try {
      await setCameraOverlayVisible(newValue)
      await updateSettings({ ...settings, cameraEnabled: newValue })
    } catch (err) {
      console.error('Failed to update camera setting:', err)
    }
  }

  const isRecording = recordingState === 'recording'
  const isPaused = recordingState === 'paused'
  const isIdle = recordingState === 'idle'
  const isCountdown = recordingState === 'countdown'

  return (
    <div className="relative">
      <div
        style={{
          padding: '10px',
          userSelect: 'none',
          WebkitUserSelect: 'none'
        }}
        className="flex items-center gap-x-6 bg-neutral-900 backdrop-blur-md rounded-full border border-neutral-700/60 shadow-2xl select-none"
        data-tauri-drag-region="true"
      >
        {/* Recording Indicator */}
        <div className="flex items-center gap-2 pointer-events-none select-none">
          <div
            className={`w-2 h-2 rounded-full ${
              isRecording ? 'bg-red-500' : 'bg-neutral-600'
            }`}
          />
          <span
            className={`text-xs uppercase ${
              isRecording ? 'text-neutral-200' : 'text-neutral-500'
            }`}
          >
            RECORDING
          </span>
        </div>

        {/* Timer */}
        <TimerDisplay />

        {/* Divider */}
        <div className="w-px h-6 bg-neutral-700" />

        {/* Control Buttons */}
        <div className="flex items-center gap-2">
          {/* Pause/Resume Button */}
          <button
            onClick={handlePause}
            disabled={isIdle || isCountdown || recordingState === 'stopping'}
            className="w-8 h-8 rounded-full bg-white flex items-center justify-center hover:bg-neutral-100 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            aria-label={isPaused ? 'Resume' : 'Pause'}
            data-tauri-drag-region="false"
          >
            {isPaused ? (
              <Play className="w-4 h-4 text-black" fill="black" />
            ) : (
              <Pause className="w-4 h-4 text-black" fill="black" />
            )}
          </button>

          {/* Start/Stop Button */}
          {isIdle || isCountdown ? (
            <button
              onClick={handleStart}
              disabled={isCountdown}
              className="cursor-pointer w-8 h-8 rounded-full bg-white flex items-center justify-center hover:bg-neutral-100 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              aria-label="Start Recording"
              data-tauri-drag-region="false"
            >
              <Play className="w-4 h-4 text-black" fill="black" />
            </button>
          ) : (
            <button
              onClick={handleStop}
              disabled={recordingState === 'stopping'}
              className="cursor-pointer w-8 h-8 rounded-full bg-red-500 flex items-center justify-center hover:bg-red-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              aria-label="Stop Recording"
              data-tauri-drag-region="false"
            >
              <Square className="w-4 h-4 text-white" fill="white" />
            </button>
          )}

          {/* Microphone Toggle */}
          <button
            onClick={handleMicToggle}
            disabled={isCountdown || recordingState === 'stopping'}
            className={`cursor-pointer w-8 h-8 rounded-full flex items-center justify-center transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
              isMicEnabled
                ? 'bg-neutral-800 border-2 border-green-500'
                : 'bg-neutral-800 hover:bg-neutral-700'
            }`}
            aria-label={
              isMicEnabled ? 'Disable Microphone' : 'Enable Microphone'
            }
            data-tauri-drag-region="false"
          >
            {isMicEnabled ? (
              <Mic className="w-4 h-4 text-green-500" />
            ) : (
              <MicOff className="w-4 h-4 text-neutral-400" />
            )}
          </button>

          {/* Camera Toggle */}
          <button
            onClick={handleCameraToggle}
            disabled={isCountdown || recordingState === 'stopping'}
            className={`cursor-pointer w-8 h-8 rounded-full flex items-center justify-center transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
              isCameraEnabled
                ? 'bg-neutral-800 border-2 border-blue-500'
                : 'bg-neutral-800 hover:bg-neutral-700'
            }`}
            aria-label={isCameraEnabled ? 'Disable Camera' : 'Enable Camera'}
            data-tauri-drag-region="false"
          >
            {isCameraEnabled ? (
              <Camera className="w-4 h-4 text-blue-500" />
            ) : (
              <CameraOff className="w-4 h-4 text-neutral-400" />
            )}
          </button>
        </div>
      </div>
      {/* Countdown Overlay */}
      {isCountdown && <Countdown />}
    </div>
  )
}
