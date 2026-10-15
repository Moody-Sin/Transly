export const DEFAULT_LOCAL_SERVER_URL = 'http://127.0.0.1:39281'

/**
 * 校验并归一化本地桌面端服务 URL。
 * 严格限制仅允许回环地址（127.0.0.1、localhost、[::1]）以及 http 协议。
 * 任何远程/非回环地址一律拒绝并回退到默认地址，防止 Token 外泄。
 */
export function normalizeLoopbackServerUrl(urlStr: string | null | undefined): string | null {
  if (!urlStr || typeof urlStr !== 'string') return null
  const trimmed = urlStr.trim()
  if (!trimmed) return null

  try {
    const parsed = new URL(trimmed)
    if (parsed.protocol !== 'http:') {
      return null
    }

    const host = parsed.hostname.toLowerCase()
    // 仅允许本地回环主机名
    if (host !== '127.0.0.1' && host !== 'localhost' && host !== '::1' && host !== '[::1]') {
      return null
    }

    const port = parsed.port ? `:${parsed.port}` : ''
    return `${parsed.protocol}//${parsed.hostname}${port}`
  } catch {
    return null
  }
}

/**
 * 校验端口号是否在合法非特权/自定义范围内（1-65535）
 */
export function normalizeServerPort(port: unknown): number | null {
  if (typeof port === 'number' && Number.isInteger(port) && port >= 1 && port <= 65535) {
    return port
  }
  if (typeof port === 'string') {
    const trimmed = port.trim()
    if (/^\d+$/.test(trimmed)) {
      const num = parseInt(trimmed, 10)
      if (num >= 1 && num <= 65535) {
        return num
      }
    }
  }
  return null
}
