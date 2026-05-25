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
  saveLocation?: string // Defaults to Downloads
}

export interface CameraFrame {
  id: number
  width: number
  height: number
  format: 'jpeg'
  data_base64: string
}
