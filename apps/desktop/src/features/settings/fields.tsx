import type { ReactNode } from "react"
import { Controller, useFormContext, type FieldPathByValue } from "react-hook-form"

import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import type { Settings } from "@/lib/native"

import { describedBy, rowIds, SettingRow } from "./SettingRow.tsx"

type BooleanKey = FieldPathByValue<Settings, boolean>

/** Boolean preference rendered as a Switch row. `onTurnOn` lets a row require confirmation first. */
export function SwitchRow({
  name,
  label,
  description,
  note,
  disabled = false,
  onTurnOn,
}: {
  name: BooleanKey
  label: string
  description: ReactNode
  note?: ReactNode
  disabled?: boolean
  onTurnOn?: () => void
}) {
  const { control } = useFormContext<Settings>()
  const ids = rowIds(name)
  return (
    <Controller
      control={control}
      name={name}
      render={({ field }) => (
        <SettingRow
          name={name}
          label={label}
          description={description}
          note={note}
          control={
            <Switch
              id={ids.control}
              name={field.name}
              checked={field.value}
              disabled={disabled}
              inputRef={field.ref}
              aria-labelledby={ids.label}
              aria-describedby={describedBy(name, true, false)}
              onCheckedChange={(checked) => {
                if (checked && !field.value && onTurnOn) onTurnOn()
                else field.onChange(checked)
              }}
              onBlur={field.onBlur}
            />
          }
        />
      )}
    />
  )
}

export type Option<V> = { value: V; label: string; disabled?: boolean }

/** Select over a fixed option list. Disabled options keep their reason visible in the row note. */
export function SelectControl<V extends string | number>({
  name,
  value,
  options,
  onChange,
  onBlur,
  disabled = false,
  invalid = false,
  hasError = false,
  width = "w-60",
}: {
  name: string
  value: V
  options: Option<V>[]
  onChange: (value: V) => void
  onBlur?: () => void
  disabled?: boolean
  invalid?: boolean
  hasError?: boolean
  width?: string
}) {
  const ids = rowIds(name)
  return (
    <Select<V>
      items={options.map((option) => ({ value: option.value, label: option.label }))}
      value={value}
      disabled={disabled}
      onValueChange={(next) => {
        if (next !== null) onChange(next)
      }}
    >
      <SelectTrigger
        id={ids.control}
        className={width}
        aria-labelledby={ids.label}
        aria-describedby={describedBy(name, true, hasError)}
        aria-invalid={invalid || undefined}
        onBlur={onBlur}
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map((option) => (
          <SelectItem key={String(option.value)} value={option.value} disabled={option.disabled}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
