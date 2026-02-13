import { beforeEach, describe, expect, it } from 'vitest'

import { useRecordingStore } from './recordingStore'

describe('recordingStore', () => {
  beforeEach(() => {
    useRecordingStore.getState().reset()
  })

  it('starts in idle state', () => {
    const state = useRecordingStore.getState()
    expect(state.recordingState).toBe('idle')
    expect(state.elapsedTimeMs).toBe(0)
    expect(state.countdownSecondsRemaining).toBeNull()
  })

  it('runs countdown ticks', () => {
    const store = useRecordingStore.getState()
    store.startCountdown()
    expect(useRecordingStore.getState().recordingState).toBe('countdown')
    expect(useRecordingStore.getState().countdownSecondsRemaining).toBe(3)

    store.tickCountdown()
    expect(useRecordingStore.getState().countdownSecondsRemaining).toBe(2)

    store.tickCountdown()
    store.tickCountdown()
    expect(useRecordingStore.getState().countdownSecondsRemaining).toBeNull()
  })

  it('toggles mute flags', () => {
    const store = useRecordingStore.getState()
    expect(store.isMicMuted).toBe(false)
    expect(store.isSystemAudioMuted).toBe(false)

    store.toggleMicMute()
    store.toggleSystemAudioMute()

    expect(useRecordingStore.getState().isMicMuted).toBe(true)
    expect(useRecordingStore.getState().isSystemAudioMuted).toBe(true)
  })
})
