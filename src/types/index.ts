export type RecordingState =
  | 'idle'
  | 'countdown'
  | 'recording'
  | 'paused'
  | 'stopping'

export interface RecordingOptions {
  includeMicrophone: boolean
  includeCamera: boolean
  microphoneDeviceId?: string
  cameraDeviceId?: string
  screenTarget?: string // For future: specific screen/window
}

export interface CaptureDevice {
  id: string
  name: string
  index: number
  isDefault: boolean
  isBuiltin: boolean
}

export interface CaptureDevices {
  microphones: CaptureDevice[]
  cameras: CaptureDevice[]
  selectedMicrophoneId?: string
  selectedCameraId?: string
}

export interface AppSettings {
  micEnabled: boolean
  cameraEnabled: boolean
  microphoneDeviceId?: string
  cameraDeviceId?: string
  hideWebcamOnImmersiveMode: boolean
  immersiveShortcut: string
  saveRecordingsLocally: boolean
  saveLocation?: string // Defaults to Downloads
  googleDrive: GoogleDriveSettings
}

export interface GoogleDriveSettings {
  enabled: boolean
  clientId?: string
  folderId?: string
  folderName?: string
  accountEmail?: string
  accessToken?: string
  refreshToken?: string
  tokenExpiresAtMs?: number
}

export interface DriveFolder {
  id: string
  name: string
  modifiedTime?: string
  webViewLink?: string
}

export interface DriveVideo {
  id: string
  name: string
  createdTime?: string
  modifiedTime?: string
  webViewLink?: string
  thumbnailLink?: string
}

export interface DriveUploadResult {
  id: string
  name: string
  webViewLink: string
}

export interface CameraFrame {
  id: number
  width: number
  height: number
  format: 'jpeg'
  data_base64: string
}
