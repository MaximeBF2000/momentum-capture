import { useEffect } from 'react'
import { ControlBar } from '../components/recording/ControlBar'
import { useRecordingStore } from '../state/recordingStore'
import { useSettingsStore } from '../state/settingsStore'
import { subscribeToRecordingEvents } from '../tauri/events'
import { getSettings } from '../tauri/commands'
import '../App.css'

export function OverlayWindow() {
  const { setRecordingState, setElapsedTime, reset, setError } =
    useRecordingStore()
  const { setSettings } = useSettingsStore()

  useEffect(() => {
    // Load settings on mount
    getSettings()
      .then(settings => {
        setSettings(settings)
        useRecordingStore.setState({
          isMicEnabled: settings.micEnabled,
          isCameraEnabled: settings.cameraEnabled
        })
      })
      .catch(err => {
        console.error('Failed to load settings:', err)
      })

    // Subscribe to recording events
    const unsubscribe = subscribeToRecordingEvents({
      onStarted: () => {
        console.log('Recording started event received')
        setRecordingState('recording')
      },
      onPaused: () => {
        console.log('Recording paused event received')
        setRecordingState('paused')
      },
      onResumed: () => {
        console.log('Recording resumed event received')
        setRecordingState('recording')
      },
      onStopped: () => {
        console.log('Recording stopped event received')
        // Keep in stopping state until saved
      },
      onSaved: () => {
        console.log('Recording saved event received')
        reset()
      },
      onError: payload => {
        console.error('Recording error event received:', payload)
        setError(payload.message)
        reset()
      }
    })

    // Timer logic - use ref to persist interval ID
    const intervalIdRef = { current: null as number | null }

    const startTimer = () => {
      if (intervalIdRef.current !== null) {
        return // Timer already running
      }
      intervalIdRef.current = window.setInterval(() => {
        const state = useRecordingStore.getState()
        if (state.recordingState === 'recording') {
          setElapsedTime(state.elapsedTimeMs + 1000)
        } else {
          // Stop timer if not recording
          if (intervalIdRef.current !== null) {
            clearInterval(intervalIdRef.current)
            intervalIdRef.current = null
          }
        }
      }, 1000)
    }

    const stopTimer = () => {
      if (intervalIdRef.current !== null) {
        clearInterval(intervalIdRef.current)
        intervalIdRef.current = null
      }
    }

    // Start timer if already recording
    const state = useRecordingStore.getState()
    if (state.recordingState === 'recording') {
      startTimer()
    }

    // Subscribe to state changes
    const unsubscribeStore = useRecordingStore.subscribe(state => {
      if (state.recordingState === 'recording') {
        startTimer()
      } else {
        stopTimer()
      }
    })

    return () => {
      unsubscribe()
      unsubscribeStore()
      stopTimer()
    }
  }, [setRecordingState, setElapsedTime, reset, setError, setSettings])

  return (
    <div
      className="w-full h-full flex items-center justify-center bg-transparent p-6"
      data-tauri-drag-region="true"
    >
      <ControlBar />
    </div>
  )
}
