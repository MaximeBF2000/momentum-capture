export type RecordingState =
  | 'idle'
  | 'countdown'
  | 'recording'
  | 'paused'
  | 'stopping'

export interface RecordingOptions {
  includeMicrophone: boolean
  includeCamera: boolean
  screenTarget?: string // For future: specific screen/window
}

export interface AppSettings {
  micEnabled: boolean
  cameraEnabled: boolean
  saveLocation?: string // Defaults to Downloads
}

export interface CameraFrame {
  id: number
  width: number
  height: number
  format: 'jpeg'
  data_base64: string
}
