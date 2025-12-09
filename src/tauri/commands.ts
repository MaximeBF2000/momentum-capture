import { invoke } from '@tauri-apps/api/core'
import type { RecordingOptions, AppSettings } from '../types'

export const startRecording = async (
  options: RecordingOptions
): Promise<void> => {
  await invoke('start_recording', { options })
}

export const pauseRecording = async (): Promise<void> => {
  await invoke('pause_recording')
}

export const resumeRecording = async (): Promise<void> => {
  await invoke('resume_recording')
}

export const stopRecording = async (): Promise<void> => {
  await invoke('stop_recording')
}

export const getSettings = async (): Promise<AppSettings> => {
  return await invoke('get_settings')
}

export const updateSettings = async (settings: AppSettings): Promise<void> => {
  await invoke('update_settings', { settings })
}

export const setCameraOverlayVisible = async (
  visible: boolean
): Promise<void> => {
  await invoke('set_camera_overlay_visible', { visible })
}

export const toggleMicrophoneDuringRecording = async (
  enabled: boolean
): Promise<void> => {
  await invoke('toggle_microphone_during_recording', { enabled })
}
