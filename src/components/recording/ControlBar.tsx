import {
  Pause,
  Play,
  Square,
  Mic,
  MicOff,
  Camera,
  CameraOff,
  Volume2,
  VolumeX,
  Settings
} from 'lucide-react'
import { useRecordingStore } from '../../state/recordingStore'
import { TimerDisplay } from './TimerDisplay'
import {
  pauseRecording,
  resumeRecording,
  stopRecording,
  startRecording,
  setCameraOverlayVisible,
  updateSettings,
  setMicMuted,
  setSystemAudioMuted,
  toggleSettingsWindow
} from '../../tauri/commands'
import { useSettingsStore } from '../../state/settingsStore'

export function ControlBar() {
  const {
    recordingState,
    isMicMuted,
    isSystemAudioMuted,
    startCountdown,
    setRecordingState,
    toggleMicMute,
    toggleSystemAudioMute,
    setError
  } = useRecordingStore()

  const { settings, updateSetting } = useSettingsStore()

  const handleStart = async () => {
    try {
      startCountdown()
      setTimeout(async () => {
        useRecordingStore.getState().tickCountdown() // 3 -> 2
        setTimeout(async () => {
          useRecordingStore.getState().tickCountdown() // 2 -> 1
          setTimeout(async () => {
            useRecordingStore.getState().tickCountdown() // 1 -> 0 and clear
            try {
              console.log('Countdown finished, starting recording with options:', {
                mic: settings.micEnabled,
                camera: settings.cameraEnabled
              })
              setRecordingState('recording')
              await startRecording({
                includeMicrophone: settings.micEnabled,
                includeCamera: settings.cameraEnabled,
                microphoneDeviceId: settings.microphoneDeviceId,
                cameraDeviceId: settings.cameraDeviceId
              })
              console.log('Recording command sent successfully')
            } catch (err: any) {
              console.error('Failed to start recording:', err)
              setError(err.message || 'Failed to start recording')
              useRecordingStore.getState().reset()
            }
          }, 1000)
        }, 1000)
      }, 1000)
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

  const handleCameraToggle = async () => {
    const newValue = !settings.cameraEnabled
    updateSetting('cameraEnabled', newValue)
    // Update settings and camera overlay visibility
    try {
      await setCameraOverlayVisible(newValue)
      await updateSettings({ ...settings, cameraEnabled: newValue })
    } catch (err) {
      console.error('Failed to update camera setting:', err)
    }
  }

  const handleMicMuteToggle = async () => {
    const newMuted = !isMicMuted
    toggleMicMute()
    // Always send the mute state to backend during recording
    if (recordingState === 'recording' || recordingState === 'paused') {
      try {
        await setMicMuted(newMuted)
      } catch (err) {
        console.error('Failed to set mic mute:', err)
      }
    }
  }

  const handleSystemAudioMuteToggle = async () => {
    const newMuted = !isSystemAudioMuted
    toggleSystemAudioMute()
    // Always send the mute state to backend during recording
    if (recordingState === 'recording' || recordingState === 'paused') {
      try {
        await setSystemAudioMuted(newMuted)
      } catch (err) {
        console.error('Failed to set system audio mute:', err)
      }
    }
  }

  const handleSettingsToggle = async () => {
    try {
      await toggleSettingsWindow()
    } catch (err: any) {
      console.error('Failed to toggle settings window:', err)
      setError(err.message || 'Failed to toggle settings window')
    }
  }

  const isRecording = recordingState === 'recording'
  const isPaused = recordingState === 'paused'
  const isIdle = recordingState === 'idle'
  const isCountdown = recordingState === 'countdown'
  const isStopping = recordingState === 'stopping'
  const isBusy = isCountdown || isStopping
  const countdownSecondsRemaining = useRecordingStore(
    state => state.countdownSecondsRemaining
  )
  const buttonBase =
    'cursor-pointer w-8 h-8 rounded flex items-center justify-center transition-colors disabled:opacity-45 disabled:cursor-not-allowed'
  const inactiveButton = 'bg-neutral-800/95 hover:bg-neutral-700/95'

  return (
    <div className="h-full relative flex items-center justify-center">
      <div
        style={{
          userSelect: 'none',
          WebkitUserSelect: 'none'
        }}
        className="flex h-80 w-14 flex-col items-center justify-center rounded-2xl border border-neutral-700/70 bg-neutral-950 backdrop-blur-md select-none gap-4"
        data-tauri-drag-region="true"
      >
        {isCountdown && countdownSecondsRemaining !== null ? (
          <div className="flex h-10 w-full items-center justify-center pointer-events-none select-none">
            <span className="font-mono text-2xl font-bold leading-none text-white">
              {countdownSecondsRemaining}
            </span>
          </div>
        ) : (
          <div className="flex flex-col items-center gap-2 pointer-events-none select-none">
            <div
              className={`h-2.5 w-2.5 rounded-full ${
                isRecording ? 'bg-red-500' : 'bg-neutral-600'
              }`}
              aria-label={isRecording ? 'Recording' : 'Not recording'}
            />
            <TimerDisplay
              compact
              className="font-mono text-[11px] leading-none text-neutral-300"
            />
          </div>
        )}

        <div className="flex flex-col items-center gap-2">
          <button
            onClick={handlePause}
            disabled={isIdle || isCountdown || isStopping}
            className={`${buttonBase} bg-neutral-200 hover:bg-white`}
            aria-label={isPaused ? 'Resume' : 'Pause'}
            data-tauri-drag-region="false"
          >
            {isPaused ? (
              <Play className="w-5 h-5 text-black" fill="black" />
            ) : (
              <Pause className="w-5 h-5 text-black" fill="black" />
            )}
          </button>

          {isIdle || isCountdown ? (
            <button
              onClick={handleStart}
              disabled={isCountdown}
              className={`${buttonBase} bg-neutral-200 hover:bg-white`}
              aria-label="Start Recording"
              data-tauri-drag-region="false"
            >
              <Play className="w-5 h-5 text-black" fill="black" />
            </button>
          ) : (
            <button
              onClick={handleStop}
              disabled={isStopping}
              className={`${buttonBase} bg-red-500 hover:bg-red-600`}
              aria-label="Stop Recording"
              data-tauri-drag-region="false"
            >
              <Square className="w-5 h-5 text-white" fill="white" />
            </button>
          )}

          <button
            onClick={handleMicMuteToggle}
            disabled={isBusy}
            className={`${buttonBase} ${
              !isMicMuted
                ? 'border border-green-500/80 bg-neutral-800/95'
                : inactiveButton
            }`}
            aria-label={isMicMuted ? 'Unmute Microphone' : 'Mute Microphone'}
            data-tauri-drag-region="false"
          >
            {!isMicMuted ? (
              <Mic className="w-5 h-5 text-green-500" />
            ) : (
              <MicOff className="w-5 h-5 text-neutral-400" />
            )}
          </button>

          <button
            onClick={handleSystemAudioMuteToggle}
            disabled={isBusy}
            className={`${buttonBase} ${
              !isSystemAudioMuted
                ? 'border border-green-500/80 bg-neutral-800/95'
                : inactiveButton
            }`}
            aria-label={
              isSystemAudioMuted ? 'Unmute System Audio' : 'Mute System Audio'
            }
            data-tauri-drag-region="false"
          >
            {!isSystemAudioMuted ? (
              <Volume2 className="w-5 h-5 text-green-500" />
            ) : (
              <VolumeX className="w-5 h-5 text-neutral-400" />
            )}
          </button>

          <button
            onClick={handleCameraToggle}
            disabled={isBusy}
            className={`${buttonBase} ${
              settings.cameraEnabled
                ? 'border border-blue-500/80 bg-neutral-800/95'
                : inactiveButton
            }`}
            aria-label={
              settings.cameraEnabled ? 'Disable Camera' : 'Enable Camera'
            }
            data-tauri-drag-region="false"
          >
            {settings.cameraEnabled ? (
              <Camera className="w-5 h-5 text-blue-500" />
            ) : (
              <CameraOff className="w-5 h-5 text-neutral-400" />
            )}
          </button>

          <div className="my-0.5 h-px w-8 bg-neutral-700/80" />

          <button
            onClick={handleSettingsToggle}
            className={`${buttonBase} ${inactiveButton}`}
            aria-label="Toggle Settings"
            data-tauri-drag-region="false"
          >
            <Settings className="w-5 h-5 text-neutral-300" />
          </button>
        </div>
      </div>
    </div>
  )
}
