import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
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
      })
      .catch(err => {
        console.error('Failed to load settings:', err)
      })

    // Subscribe to recording events
    const unsubscribe = subscribeToRecordingEvents({
      onStarted: payload => {
        console.log('Recording started event received')
        setRecordingState('recording')
        setElapsedTime(payload.elapsedMs)
      },
      onPaused: payload => {
        console.log('Recording paused event received')
        setRecordingState('paused')
        setElapsedTime(payload.elapsedMs)
      },
      onResumed: payload => {
        console.log('Recording resumed event received')
        setRecordingState('recording')
        setElapsedTime(payload.elapsedMs)
      },
      onStopped: payload => {
        console.log('Recording stopped event received')
        // Keep in stopping state until saved
        setElapsedTime(payload.elapsedMs)
      },
      onSaved: () => {
        console.log('Recording saved event received')
        reset()
      },
      onError: payload => {
        console.error('Recording error event received:', payload)
        setError(payload.message)
        reset()
      },
      onElapsed: payload => {
        setElapsedTime(payload.elapsedMs)
      }
    })

    const immersiveShortcutListener = listen<{ shortcut: string }>(
      'immersive-shortcut-updated',
      event => {
        setSettings({
          ...useSettingsStore.getState().settings,
          immersiveShortcut: event.payload.shortcut
        })
      }
    )

    return () => {
      unsubscribe()
      immersiveShortcutListener.then(unsub => unsub()).catch(() => {
        /* ignore */
      })
    }
  }, [setRecordingState, setElapsedTime, reset, setError, setSettings])

  return (
    <div className="w-full h-full flex items-center justify-center bg-transparent p-6">
      <ControlBar />
    </div>
  )
}
