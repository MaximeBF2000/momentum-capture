import { create } from 'zustand'
import type { AppSettings } from '../types'

interface SettingsStore {
  settings: AppSettings
  setSettings: (settings: AppSettings) => void
  updateSetting: <K extends keyof AppSettings>(
    key: K,
    value: AppSettings[K]
  ) => void
}

export const useSettingsStore = create<SettingsStore>(set => ({
  settings: {
    micEnabled: false,
    cameraEnabled: false,
    immersiveShortcut: 'Option+I'
  },
  setSettings: settings => set({ settings }),
  updateSetting: (key, value) =>
    set(state => ({
      settings: { ...state.settings, [key]: value }
    }))
}))
