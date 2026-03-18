import { useState } from 'react'
import type { EventFilter } from '../types'

interface FilterField {
  key: string
  label: string
  type: 'text' | 'select'
  options?: { value: string; label: string }[]
  placeholder?: string
}

interface FilterBoxProps {
  fields: FilterField[]
  onApply: (filters: EventFilter) => void
}

export function FilterBox({ fields, onApply }: FilterBoxProps) {
  const [values, setValues] = useState<Record<string, string>>({})
  const [open, setOpen] = useState(false)

  const activeCount = Object.values(values).filter(v => v !== '').length

  const handleChange = (key: string, value: string) => {
    setValues(prev => ({ ...prev, [key]: value }))
  }

  const handleApply = () => {
    const filters: Record<string, string> = {}
    for (const [key, value] of Object.entries(values)) {
      if (value !== '') filters[key] = value
    }
    onApply(filters as EventFilter)
    setOpen(false)
  }

  const handleClear = () => {
    setValues({})
    onApply({})
    setOpen(false)
  }

  return (
    <div className="filter-box">
      <button className="filter-toggle" onClick={() => setOpen(o => !o)}>
        Filter {activeCount > 0 && <span className="filter-badge">{activeCount}</span>}
      </button>

      {open && (
        <div className="filter-panel">
          <div className="filter-fields">
            {fields.map(field => (
              <label key={field.key} className="filter-field">
                <span>{field.label}</span>
                {field.type === 'select' ? (
                  <select
                    value={values[field.key] || ''}
                    onChange={e => handleChange(field.key, e.target.value)}
                  >
                    <option value="">All</option>
                    {field.options?.map(opt => (
                      <option key={opt.value} value={opt.value}>{opt.label}</option>
                    ))}
                  </select>
                ) : (
                  <input
                    type="text"
                    placeholder={field.placeholder || ''}
                    value={values[field.key] || ''}
                    onChange={e => handleChange(field.key, e.target.value)}
                  />
                )}
              </label>
            ))}
          </div>
          <div className="filter-actions">
            <button className="btn-secondary btn-sm" onClick={handleClear}>Clear</button>
            <button className="create-event-btn btn-sm" onClick={handleApply}>Apply</button>
          </div>
        </div>
      )}
    </div>
  )
}
