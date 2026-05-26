import { invoke } from '@tauri-apps/api/core'
import type {
  RecordingOptions,
  AppSettings,
  CaptureDevices,
  DriveFolder,
  DriveVideo,
  GoogleDriveSettings
} from '../types'

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

export const getDefaultSettings = async (): Promise<AppSettings> => {
  return await invoke('get_default_settings')
}

export const listCaptureDevices = async (): Promise<CaptureDevices> => {
  return await invoke('list_capture_devices')
}

export const updateSettings = async (settings: AppSettings): Promise<void> => {
  await invoke('update_settings', { settings })
}

export const authorizeGoogleDrive =
  async (): Promise<GoogleDriveSettings> => {
    return await invoke('authorize_google_drive')
  }

export const listGoogleDriveFolders = async (): Promise<DriveFolder[]> => {
  return await invoke('list_google_drive_folders')
}

export const listGoogleDriveVideos = async (): Promise<DriveVideo[]> => {
  return await invoke('list_google_drive_videos')
}

export const showNativeNotification = async (
  title: string,
  message: string
): Promise<void> => {
  await invoke('show_native_notification', { title, message })
}

export const toggleSettingsWindow = async (): Promise<void> => {
  await invoke('toggle_settings_window')
}

export const setCameraOverlayVisible = async (
  visible: boolean
): Promise<void> => {
  await invoke('set_camera_overlay_visible', { visible })
}

export const toggleImmersiveMode = async (): Promise<void> => {
  await invoke('toggle_immersive_mode')
}

export const setImmersiveMode = async (enabled: boolean): Promise<void> => {
  await invoke('set_immersive_mode', { enabled })
}

export const toggleMicrophoneDuringRecording = async (
  enabled: boolean
): Promise<void> => {
  await invoke('toggle_microphone_during_recording', { enabled })
}

export const setMicMuted = async (muted: boolean): Promise<void> => {
  await invoke('set_mic_muted', { muted })
}

export const setSystemAudioMuted = async (muted: boolean): Promise<void> => {
  await invoke('set_system_audio_muted', { muted })
}

export const updateImmersiveShortcut = async (
  shortcut: string
): Promise<void> => {
  await invoke('update_immersive_shortcut', { shortcut })
}
