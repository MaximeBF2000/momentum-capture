import type { KeyboardEvent, ReactNode } from 'react'
import { Camera, Check, ChevronDown, Mic } from 'lucide-react'
import type { CaptureDevice } from '../../../types'

export function SettingsSection({
  title,
  children
}: {
  title: string
  children: ReactNode
}) {
  return (
    <section className="space-y-4">
      <h2 className="pl-0.5 text-[18px] font-semibold leading-none tracking-tight text-white">
        {title}
      </h2>
      <div
        className="overflow-hidden rounded-[16px] shadow-inner shadow-white/[0.035]"
        style={{
          background: 'rgba(255,255,255,0.045)',
          border: '1px solid rgba(255,255,255,0.12)'
        }}
      >
        {children}
      </div>
    </section>
  )
}

export function SettingsRow({
  icon,
  title,
  description,
  children
}: {
  icon: ReactNode
  title: string
  description?: string
  children: ReactNode
}) {
  if (!icon) {
    return (
      <div
        className="grid min-h-[104px] grid-cols-[minmax(0,1fr)_minmax(160px,200px)] items-start gap-x-5 gap-y-5 px-5 py-7"
        data-tauri-drag-region="false"
      >
        <h3 className="text-[17px] font-semibold leading-none tracking-tight text-white">
          {title}
        </h3>
        <div className="row-span-2 self-start">{children}</div>
        {description && (
          <p className="max-w-[520px] text-[15px] leading-6 text-neutral-400">
            {description}
          </p>
        )}
      </div>
    )
  }

  return (
    <div
      className="grid min-h-[78px] grid-cols-[minmax(0,1fr)_minmax(220px,280px)] items-center gap-5 px-5 py-5"
      data-tauri-drag-region="false"
    >
      <div className="flex min-w-0 items-center gap-5">
        {icon && (
          <div
            className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl text-white"
            style={{ background: 'rgba(255,255,255,0.10)' }}
          >
            {icon}
          </div>
        )}
        <div className="min-w-0">
          <h3 className="text-[16px] font-semibold leading-none tracking-tight text-white">
            {title}
          </h3>
          {description && (
            <p className="mt-2 truncate text-[14px] leading-5 text-neutral-400">
              {description}
            </p>
          )}
        </div>
      </div>
      {children}
    </div>
  )
}

export function SettingsDivider() {
  return (
    <div
      className="mx-5 h-px"
      style={{ background: 'rgba(255,255,255,0.10)' }}
    />
  )
}

export function DeviceSelect({
  label,
  value,
  devices,
  disabled,
  onChange
}: {
  label: string
  value: string
  devices: CaptureDevice[]
  disabled?: boolean
  onChange: (value: string) => void
}) {
  return (
    <div className="relative">
      <select
        aria-label={label}
        value={value}
        disabled={disabled || devices.length === 0}
        onChange={event => onChange(event.currentTarget.value)}
        className="h-[38px] w-full truncate appearance-none rounded-lg px-4 pr-10 text-[15px] font-medium text-white outline-none transition-colors focus:border-blue-500 disabled:cursor-not-allowed disabled:text-neutral-500"
        style={{
          background: 'rgba(18,18,18,0.56)',
          border: '1px solid rgba(255,255,255,0.15)'
        }}
      >
        {devices.length === 0 ? (
          <option value="">No device found</option>
        ) : (
          devices.map(device => (
            <option key={device.id} value={device.id}>
              {deviceLabel(device)}
            </option>
          ))
        )}
      </select>
      <ChevronDown className="pointer-events-none absolute right-4 top-1/2 h-5 w-5 -translate-y-1/2 text-neutral-300" />
    </div>
  )
}

export function HideWebcamToggle({
  checked,
  onChange
}: {
  checked: boolean
  onChange: (checked: boolean) => void
}) {
  return (
    <label
      className="flex min-h-[78px] cursor-pointer items-center gap-4 px-5 py-5"
      data-tauri-drag-region="false"
    >
      <span
        className="relative flex h-6 w-6 shrink-0 items-center justify-center rounded-md"
        style={{
          background: 'rgba(10,10,10,0.95)',
          border: '1px solid rgba(255,255,255,0.15)'
        }}
      >
        <input
          type="checkbox"
          checked={checked}
          onChange={event => onChange(event.currentTarget.checked)}
          className="peer absolute inset-0 cursor-pointer opacity-0"
        />
        <span className="absolute inset-0 rounded-md bg-blue-600 opacity-0 peer-checked:opacity-100" />
        <Check className="relative h-5 w-5 text-white opacity-0 peer-checked:opacity-100" />
      </span>
      <span>
        <span className="block text-[16px] font-semibold leading-none tracking-tight text-white">
          Include webcam
        </span>
        <span className="mt-2 block max-w-[520px] text-[14px] leading-5 text-neutral-400">
          Hide webcam when immersive mode is enabled.
        </span>
      </span>
    </label>
  )
}

export function ShortcutCaptureField({
  value,
  onChange
}: {
  value: string
  onChange: (value: string) => void
}) {
  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    event.preventDefault()
    const next = buildShortcutString(event)
    if (next) {
      onChange(next)
    }
  }

  return (
    <button
      type="button"
      onKeyDown={handleKeyDown}
      className="flex h-[38px] w-full items-center justify-between rounded-lg px-4 text-left outline-none transition-colors focus:border-blue-500"
      style={{
        background: 'rgba(18,18,18,0.56)',
        border: '1px solid rgba(255,255,255,0.15)'
      }}
      data-tauri-drag-region="false"
    >
      <span className="flex items-center gap-5 font-mono text-[15px] leading-none text-white">
        {shortcutSegments(value).map((segment, index) => (
          <span key={`${segment}-${index}`}>{segment}</span>
        ))}
      </span>
      <ChevronDown className="h-5 w-5 text-neutral-300" />
    </button>
  )
}

export function DeviceIcon({ type }: { type: 'microphone' | 'camera' }) {
  return type === 'microphone' ? (
    <Mic className="h-6 w-6" />
  ) : (
    <Camera className="h-6 w-6" />
  )
}

function deviceLabel(device: CaptureDevice) {
  const details = [
    device.isBuiltin ? 'Built-in' : null,
    device.isDefault ? 'Default' : null
  ].filter(Boolean)

  return details.length > 0
    ? `${device.name} (${details.join(', ')})`
    : device.name
}

function buildShortcutString(event: KeyboardEvent<HTMLButtonElement>) {
  const key = normalizeKey(event.key)
  if (!key) {
    return null
  }

  const segments: string[] = []
  if (event.metaKey) segments.push('Command')
  if (event.ctrlKey) segments.push('Control')
  if (event.altKey) segments.push('Option')
  if (event.shiftKey) segments.push('Shift')
  segments.push(key)

  return segments.join('+')
}

function normalizeKey(key: string) {
  const ignored = ['Shift', 'Control', 'Alt', 'Meta', 'Fn', 'Dead']
  if (ignored.includes(key)) {
    return null
  }

  if (key === ' ') {
    return 'Space'
  }

  if (key.length === 1) {
    return key.toUpperCase()
  }

  if (key.startsWith('Arrow')) {
    return key
  }

  return key.charAt(0).toUpperCase() + key.slice(1)
}

function shortcutSegments(value: string) {
  return value.split('+').map(segment => {
    switch (segment) {
      case 'Command':
        return '⌘'
      case 'Option':
        return '⌥'
      case 'Control':
        return '⌃'
      case 'Shift':
        return '⇧'
      default:
        return segment
    }
  })
}
