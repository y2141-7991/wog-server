import { useEffect, useRef, useState } from 'react'
import type { KeyboardEvent, RefObject } from 'react'
import { MAX_BODY_CHARS } from './chatFormat'

interface Props {
  onSend: (body: string) => void
  /** Called with true while the user types (throttled) and false when they stop. */
  onTyping?: (typing: boolean) => void
  placeholder: string
  disabled?: boolean
  inputRef?: RefObject<HTMLTextAreaElement | null>
}

const TYPING_REPEAT_MS = 2_500
const TYPING_IDLE_MS = 3_000

/** Multiline input: Enter sends, Shift+Enter adds a line. */
export function ChatComposer({ onSend, onTyping, placeholder, disabled, inputRef }: Props) {
  const [text, setText] = useState('')
  const idleTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const repeatTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const typing = useRef(false)

  const trimmed = text.trim()
  const over = text.length > MAX_BODY_CHARS
  const canSend = !disabled && trimmed.length > 0 && !over

  useEffect(
    () => () => {
      clearTimeout(idleTimer.current)
      clearTimeout(repeatTimer.current)
    },
    [],
  )

  /** Tell the other side we are typing: once, then again every few seconds, then "stopped" after a pause. */
  function setTyping(on: boolean) {
    if (!onTyping) return
    if (on) {
      if (!typing.current || repeatTimer.current === undefined) {
        typing.current = true
        onTyping(true)
        repeatTimer.current = setTimeout(() => {
          repeatTimer.current = undefined
        }, TYPING_REPEAT_MS)
      }
      clearTimeout(idleTimer.current)
      idleTimer.current = setTimeout(() => setTyping(false), TYPING_IDLE_MS)
    } else if (typing.current) {
      typing.current = false
      clearTimeout(idleTimer.current)
      clearTimeout(repeatTimer.current)
      repeatTimer.current = undefined
      onTyping(false)
    }
  }

  function submit() {
    if (!canSend) return
    setTyping(false)
    onSend(trimmed)
    setText('')
  }

  function onKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault()
      submit()
    }
  }

  return (
    <form className="chat-composer" onSubmit={(e) => { e.preventDefault(); submit() }}>
      <textarea
        ref={inputRef}
        value={text}
        rows={2}
        placeholder={placeholder}
        aria-label="Message"
        onChange={(e) => { setText(e.target.value); setTyping(e.target.value.length > 0) }}
        onKeyDown={onKeyDown}
        onBlur={() => setTyping(false)}
      />
      <div className="chat-composer-row">
        <span className={`chat-count${over ? ' chat-count-over' : ''}`} aria-live="polite">
          {text.length > MAX_BODY_CHARS * 0.8 || over ? `${text.length}/${MAX_BODY_CHARS}` : 'Enter to send · Shift+Enter for a new line'}
        </span>
        <button type="submit" className="btn-primary btn-sm" disabled={!canSend}>Send</button>
      </div>
    </form>
  )
}
