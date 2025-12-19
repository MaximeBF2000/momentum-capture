import { create } from 'zustand'
import type { RecordingState } from '../types'

interface RecordingStore {
  recordingState: RecordingState
  elapsedTimeMs: number
  countdownSecondsRemaining: number | null
  isMicEnabled: boolean
  isCameraEnabled: boolean
  isMicMuted: boolean
  isSystemAudioMuted: boolean
  errorMessage: string | null

  // Actions
  startCountdown: () => void
  tickCountdown: () => void
  setRecordingState: (state: RecordingState) => void
  setElapsedTime: (ms: number) => void
  toggleMic: () => void
  toggleCamera: () => void
  toggleMicMute: () => void
  toggleSystemAudioMute: () => void
  setError: (message: string | null) => void
  reset: () => void
}

export const useRecordingStore = create<RecordingStore>(set => ({
  recordingState: 'idle',
  elapsedTimeMs: 0,
  countdownSecondsRemaining: null,
  isMicEnabled: false,
  isCameraEnabled: false,
  isMicMuted: false,
  isSystemAudioMuted: false,
  errorMessage: null,

  startCountdown: () => {
    set({
      recordingState: 'countdown',
      countdownSecondsRemaining: 3
    })
  },

  tickCountdown: () => {
    set(state => {
      if (state.countdownSecondsRemaining === null) return state
      const next = state.countdownSecondsRemaining - 1
      if (next <= 0) {
        return {
          countdownSecondsRemaining: null
          // Don't set recordingState here - let the backend event handle it
        }
      }
      return { countdownSecondsRemaining: next }
    })
  },

  setRecordingState: state => {
    set({ recordingState: state })
  },

  setElapsedTime: ms => {
    set({ elapsedTimeMs: ms })
  },

  toggleMic: () => {
    set(state => ({ isMicEnabled: !state.isMicEnabled }))
  },

  toggleCamera: () => {
    set(state => ({ isCameraEnabled: !state.isCameraEnabled }))
  },

  toggleMicMute: () => {
    set(state => ({ isMicMuted: !state.isMicMuted }))
  },

  toggleSystemAudioMute: () => {
    set(state => ({ isSystemAudioMuted: !state.isSystemAudioMuted }))
  },

  setError: message => {
    set({ errorMessage: message })
  },

  reset: () => {
    set({
      recordingState: 'idle',
      elapsedTimeMs: 0,
      countdownSecondsRemaining: null,
      isMicMuted: false,
      isSystemAudioMuted: false,
      errorMessage: null
    })
  }
}))
