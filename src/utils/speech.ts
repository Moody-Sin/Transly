/**
 * 文本发音（TTS）辅助工具。
 * 规范化 BCP-47 语言代码，消除 "auto" 导致 Web Speech API 静默失败的缺陷。
 */
export function normalizeLanguageCode(lang?: string | null, text?: string): string {
  const clean = (lang || '').trim().toLowerCase()
  if (clean && clean !== 'auto') {
    switch (clean) {
      case 'zh':
      case 'zh-cn':
      case 'zh-hans':
        return 'zh-CN'
      case 'zh-tw':
      case 'zh-hant':
        return 'zh-TW'
      case 'en':
      case 'en-us':
        return 'en-US'
      case 'en-gb':
        return 'en-GB'
      case 'ja':
        return 'ja-JP'
      case 'ko':
        return 'ko-KR'
      case 'fr':
        return 'fr-FR'
      case 'de':
        return 'de-DE'
      case 'es':
        return 'es-ES'
      case 'ru':
        return 'ru-RU'
      default:
        return lang!
    }
  }

  // 若无语言或为 auto，根据文本字符区段特征推断发音代码
  if (text) {
    if (/[\u3040-\u30ff]/.test(text)) return 'ja-JP'
    if (/[\uac00-\ud7af]/.test(text)) return 'ko-KR'
    if (/[\u4e00-\u9fa5]/.test(text)) return 'zh-CN'
    if (/[\u0400-\u04ff]/.test(text)) return 'ru-RU'
  }

  return 'en-US'
}

export interface SpeakOptions {
  onStart?: () => void
  onEnd?: () => void
  onError?: (err: unknown) => void
}

let activeUtterance: SpeechSynthesisUtterance | null = null

export function stopSpeaking(): void {
  if (typeof window === 'undefined' || !('speechSynthesis' in window)) return
  try {
    window.speechSynthesis.cancel()
    activeUtterance = null
  } catch (err) {
    console.warn('[Transly TTS] 停止语音播放异常：', err)
  }
}

export function isSpeaking(): boolean {
  if (typeof window === 'undefined' || !('speechSynthesis' in window)) return false
  return Boolean(activeUtterance || window.speechSynthesis.speaking)
}

export function speakText(
  text: string,
  lang?: string | null,
  detectedLang?: string | null,
  options?: SpeakOptions,
): void {
  if (typeof window === 'undefined' || !('speechSynthesis' in window)) return
  const cleanText = text.trim()
  if (!cleanText) return

  try {
    stopSpeaking()
    const targetLang = normalizeLanguageCode(lang || detectedLang, cleanText)
    const utterance = new SpeechSynthesisUtterance(cleanText)
    utterance.lang = targetLang
    activeUtterance = utterance

    utterance.onstart = () => {
      options?.onStart?.()
    }
    utterance.onend = () => {
      activeUtterance = null
      options?.onEnd?.()
    }
    utterance.onerror = (e) => {
      activeUtterance = null
      options?.onError?.(e)
    }
    window.speechSynthesis.speak(utterance)
  } catch (err) {
    activeUtterance = null
    console.warn('[Transly TTS] 语音播放异常：', err)
    options?.onError?.(err)
  }
}
