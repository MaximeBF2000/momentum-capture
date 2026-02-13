import { listen } from '@tauri-apps/api/event'
import type { AppSettings, CameraFrame } from '../types'

export const RECORDING_EVENTS = {
  STARTED: 'recording-started',
  PAUSED: 'recording-paused',
  RESUMED: 'recording-resumed',
  STOPPED: 'recording-stopped',
  SAVED: 'recording-saved',
  ERROR: 'recording-error',
  ELAPSED: 'recording-elapsed',
  CAMERA_FRAME: 'camera-frame',
  CAMERA_ERROR: 'camera-error'
} as const

export const SETTINGS_EVENTS = {
  UPDATED: 'settings-updated'
} as const

export type RecordingEventName =
  (typeof RECORDING_EVENTS)[keyof typeof RECORDING_EVENTS]

export interface RecordingSavedPayload {
  path: string
}

export interface RecordingStartedPayload {
  startedAtMs: number
  elapsedMs: number
}

export interface RecordingPausedPayload {
  elapsedMs: number
}

export interface RecordingResumedPayload {
  elapsedMs: number
}

export interface RecordingStoppedPayload {
  elapsedMs: number
}

export interface RecordingErrorPayload {
  message: string
}

export interface RecordingElapsedPayload {
  elapsedMs: number
}

export const subscribeToRecordingEvents = (callbacks: {
  onStarted?: (payload: RecordingStartedPayload) => void
  onPaused?: (payload: RecordingPausedPayload) => void
  onResumed?: (payload: RecordingResumedPayload) => void
  onStopped?: (payload: RecordingStoppedPayload) => void
  onSaved?: (payload: RecordingSavedPayload) => void
  onError?: (payload: RecordingErrorPayload) => void
  onElapsed?: (payload: RecordingElapsedPayload) => void
  onCameraFrame?: (frame: CameraFrame) => void
  onCameraError?: (payload: RecordingErrorPayload) => void
}) => {
  const unsubscribers: Array<() => void> = []

  if (callbacks.onStarted) {
    listen<RecordingStartedPayload>(RECORDING_EVENTS.STARTED, event => {
      callbacks.onStarted?.(event.payload)
    }).then(unsub => unsubscribers.push(unsub))
  }

  if (callbacks.onPaused) {
    listen<RecordingPausedPayload>(RECORDING_EVENTS.PAUSED, event => {
      callbacks.onPaused?.(event.payload)
    }).then(unsub => unsubscribers.push(unsub))
  }

  if (callbacks.onResumed) {
    listen<RecordingResumedPayload>(RECORDING_EVENTS.RESUMED, event => {
      callbacks.onResumed?.(event.payload)
    }).then(unsub => unsubscribers.push(unsub))
  }

  if (callbacks.onStopped) {
    listen<RecordingStoppedPayload>(RECORDING_EVENTS.STOPPED, event => {
      callbacks.onStopped?.(event.payload)
    }).then(unsub => unsubscribers.push(unsub))
  }

  if (callbacks.onSaved) {
    listen<RecordingSavedPayload>(RECORDING_EVENTS.SAVED, event => {
      callbacks.onSaved?.(event.payload)
    }).then(unsub => unsubscribers.push(unsub))
  }

  if (callbacks.onError) {
    listen<RecordingErrorPayload>(RECORDING_EVENTS.ERROR, event => {
      callbacks.onError?.(event.payload)
    }).then(unsub => unsubscribers.push(unsub))
  }

  if (callbacks.onElapsed) {
    listen<RecordingElapsedPayload>(RECORDING_EVENTS.ELAPSED, event => {
      callbacks.onElapsed?.(event.payload)
    }).then(unsub => unsubscribers.push(unsub))
  }

  if (callbacks.onCameraFrame) {
    listen<CameraFrame>(RECORDING_EVENTS.CAMERA_FRAME, event => {
      callbacks.onCameraFrame?.(event.payload)
    }).then(unsub => unsubscribers.push(unsub))
  }

  if (callbacks.onCameraError) {
    listen<RecordingErrorPayload>(RECORDING_EVENTS.CAMERA_ERROR, event => {
      callbacks.onCameraError?.(event.payload)
    }).then(unsub => unsubscribers.push(unsub))
  }

  return () => {
    unsubscribers.forEach(unsub => unsub())
  }
}

export const subscribeToSettingsEvents = (callbacks: {
  onUpdated?: (settings: AppSettings) => void
}) => {
  const unsubscribers: Array<() => void> = []

  if (callbacks.onUpdated) {
    listen<AppSettings>(SETTINGS_EVENTS.UPDATED, event => {
      callbacks.onUpdated?.(event.payload)
    }).then(unsub => unsubscribers.push(unsub))
  }

  return () => {
    unsubscribers.forEach(unsub => unsub())
  }
}
