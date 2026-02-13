import { beforeEach, describe, expect, it } from 'vitest'

import { useSettingsStore } from './settingsStore'

describe('settingsStore', () => {
  beforeEach(() => {
    useSettingsStore.setState({
      settings: {
        micEnabled: false,
        cameraEnabled: false,
        immersiveShortcut: 'Option+I'
      }
    })
  })

  it('updates individual settings', () => {
    const store = useSettingsStore.getState()
    store.updateSetting('micEnabled', true)
    store.updateSetting('cameraEnabled', true)

    const updated = useSettingsStore.getState().settings
    expect(updated.micEnabled).toBe(true)
    expect(updated.cameraEnabled).toBe(true)
  })

  it('replaces settings', () => {
    const store = useSettingsStore.getState()
    store.setSettings({
      micEnabled: true,
      cameraEnabled: true,
      immersiveShortcut: 'Command+Shift+I'
    })

    const updated = useSettingsStore.getState().settings
    expect(updated.immersiveShortcut).toBe('Command+Shift+I')
  })
})
